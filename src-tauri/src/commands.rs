use crate::args::{build_launch_args, DEFAULT_ARGS};
use crate::buildroot::find_build_root;
use crate::config::{self, Config};
use crate::downloader::{self, AbortHandle, AbortReason};
use crate::extractor;
use crate::live::{self, Endpoints, LiveCache};
use aes_gcm::aead::AeadInPlace;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, Tag};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

const CONSOLE_FILE: &str = "edit.dll.enc";
const EXPECTED_BIN_HASHES: [(&str, &str); 3] = [
    ("Injector.exe", "f733438319af6f3c7dc40b3057b1c9239fc691b0b808311ba3e5b699f22157e4"),
    ("projectlegacy.dll", "94f9822a72d169f4c6a6303ecabf9f68fee0a9dac24c8dacc29161760a8b9905"),
    ("edit.dll.enc", "bf80ea4d642f9ca75363c626ac76000bd1149353e1160f61983b49a5022e3f25"),
];

pub struct AppState {
    pub config: Mutex<Config>,
    pub app_data_dir: PathBuf,
    pub resource_dir: PathBuf,
    pub endpoints: Endpoints,
    pub install_phase: Mutex<String>,
    pub abort: Mutex<Option<AbortHandle>>,
    pub game_pid: Mutex<Option<u32>>,
    pub game_message: Mutex<String>,
    pub ui_lang: Mutex<String>,
    pub live_cache: LiveCache,
    pub console_unlock_failures: Mutex<u32>,
    pub console_unlock_locked_until: Mutex<Option<Instant>>,
}

fn save_cfg(state: &AppState) -> Result<(), String> {
    let cfg = state.config.lock().map_err(|_| "Configuration is unavailable.".to_string())?.clone();
    config::save(&state.app_data_dir, &cfg).map_err(|e| format!("Could not save launcher settings: {e}"))
}

fn write_log(state: &AppState, message: &str) {
    let path = state.app_data_dir.join("launcher.log");
    let _ = fs::create_dir_all(&state.app_data_dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let line = format!("[{}] {}\r\n", chrono::Utc::now().to_rfc3339(), message.replace('\r', " " ).replace('\n', " "));
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

fn install_busy(state: &AppState) -> bool {
    state.install_phase.lock().map(|phase| matches!(phase.as_str(), "downloading" | "extracting")).unwrap_or(true)
}

fn set_game_status(app: &AppHandle, state: &AppState, game_state: &str, message: impl Into<String>) {
    let message = message.into();
    if let Ok(mut current) = state.game_message.lock() { *current = message.clone(); }
    let _ = app.emit("game-status", serde_json::json!({ "state": game_state, "message": message }));
    write_log(state, &format!("game state={game_state}: {message}"));
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn verify_bundled_files(resource_dir: &Path) -> Vec<serde_json::Value> {
    EXPECTED_BIN_HASHES.iter().map(|(name, expected)| {
        let path = resource_dir.join("bin").join(name);
        let actual = file_sha256(&path).ok();
        let ok = actual.as_deref() == Some(*expected);
        serde_json::json!({
            "label": format!("Bundled {}", name),
            "ok": ok,
            "detail": if ok { "Original project binary is present and unchanged.".to_string() }
                      else if actual.is_none() { "File is missing from the installed launcher resources.".to_string() }
                      else { "The file hash differs from the supplied original project. Reinstall the launcher package.".to_string() }
        })
    }).collect()
}

fn decrypt_console_payload_bytes(bytes: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 48 || &bytes[..4] != b"PLE1" || key.len() != 32 {
        return Err("The encrypted console module has an unsupported format.".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Could not initialise console decryption.".to_string())?;
    let nonce = Nonce::from_slice(&bytes[20..32]);
    let tag = Tag::from_slice(&bytes[32..48]);
    let mut payload = bytes[48..].to_vec();
    cipher.decrypt_in_place_detached(nonce, b"", &mut payload, tag)
        .map_err(|_| "The console access code is no longer valid.".to_string())?;
    Ok(payload)
}

fn console_ciphertext_valid_bytes(bytes: &[u8], key: &[u8]) -> bool {
    decrypt_console_payload_bytes(bytes, key).is_ok()
}

async fn run_bundled_injector(state: &AppState, pid: u32, dll_path: &Path) -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("Module loading is available only on Windows.".into());
    }
    let bin_dir = state.resource_dir.join("bin");
    let injector = bin_dir.join("Injector.exe");
    if file_sha256(&injector).ok().as_deref() != Some(EXPECTED_BIN_HASHES[0].1) {
        return Err("Injector.exe is missing or does not match the supplied original binary.".into());
    }
    if !dll_path.is_file() {
        return Err(format!("Module file is missing: {}", dll_path.display()));
    }
    let mut injector_command = tokio::process::Command::new(&injector);
    injector_command
        .arg("-p")
        .arg(pid.to_string())
        .arg("-i")
        .arg(dll_path)
        .current_dir(&bin_dir)
        .kill_on_drop(true);
    #[cfg(windows)]
    injector_command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(Duration::from_secs(20), injector_command.output())
        .await.map_err(|_| "Injector timed out after 20 seconds.".to_string())?
        .map_err(|e| format!("Could not start Injector.exe: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let details = format!("{stdout}{stderr}").trim().to_string();
    if output.status.success() && details.to_ascii_lowercase().contains("successfully injected") {
        Ok(details)
    } else {
        let reason = if details.is_empty() { format!("Injector exited with status {}.", output.status) } else { details };
        Err(reason)
    }
}

fn schedule_redirector_injection(app: AppHandle, pid: u32, delay_ms: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms.max(2_500).min(30_000))).await;
        let Some(state) = app.try_state::<AppState>() else { return; };
        if *state.game_pid.lock().unwrap() != Some(pid) { return; }
        let dll = state.resource_dir.join("bin").join("projectlegacy.dll");
        match run_bundled_injector(&state, pid, &dll).await {
            Ok(details) => {
                write_log(&state, &format!("Redirector injection completed: {details}"));
                set_game_status(&app, &state, "running", "Game running, redirector loaded.");
            }
            Err(error) => {
                write_log(&state, &format!("Redirector injection failed: {error}"));
                set_game_status(&app, &state, "running", format!("Game running, but redirector injection failed: {error}"));
            }
        }
    });
}

fn schedule_temp_dll_cleanup(path: PathBuf) {
    std::thread::spawn(move || {
        for _ in 0..120 {
            match fs::remove_file(&path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(Duration::from_secs(10)),
            }
        }
    });
}

fn console_key(state: &AppState) -> Option<Vec<u8>> {
    let cfg = state.config.lock().ok()?.clone();
    let raw = config::read_password(&cfg.console_key_enc, &cfg.console_key_mode).ok()?;
    hex::decode(raw).ok()
}

fn console_status_for(state: &AppState) -> serde_json::Value {
    let file = state.resource_dir.join("bin").join(CONSOLE_FILE);
    let available = file.is_file();
    let unlocked = if available {
        console_key(state).map(|key| {
            fs::read(&file).map(|bytes| console_ciphertext_valid_bytes(&bytes, &key)).unwrap_or(false)
        }).unwrap_or(false)
    } else { false };
    serde_json::json!({ "available": available, "unlocked": unlocked })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfig {
    game_folder: String,
    email: String,
    display_name: String,
    match_notifications: bool,
    match_notifications_configured: bool,
    has_password: bool,
    password_reentry_required: bool,
    has_anticrash: bool,
    installed: bool,
    build_root: String,
    platform: String,
    setup_complete: bool,
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> PublicConfig {
    let cfg = state.config.lock().unwrap().clone();
    let root = find_build_root(&cfg.game_folder, 2);
    PublicConfig {
        game_folder: cfg.game_folder.clone(),
        email: cfg.email.clone(),
        display_name: cfg.display_name.clone(),
        match_notifications: cfg.match_notifications,
        match_notifications_configured: cfg.match_notifications_configured,
        has_password: !cfg.password_enc.is_empty(),
        password_reentry_required: !cfg.password_enc.is_empty() && config::read_password(&cfg.password_enc, &cfg.password_mode).is_err(),
        has_anticrash: state.resource_dir.join("bin").join("anticrash.dll").is_file(),
        installed: root.is_some(),
        build_root: root.map(|p| p.display().to_string()).unwrap_or_default(),
        platform: if cfg!(target_os = "windows") { "win32".to_string() } else if cfg!(target_os = "macos") { "darwin".to_string() } else { std::env::consts::OS.to_string() },
        setup_complete: cfg.setup_complete,
    }
}

#[tauri::command]
pub fn complete_setup(state: State<AppState>) -> Result<bool, String> {
    let mut cfg = state.config.lock()
        .map_err(|_| "Launcher configuration is temporarily unavailable.".to_string())?;
    if cfg.display_name.trim().is_empty() || cfg.email.trim().is_empty() || cfg.password_enc.is_empty() {
        return Err("Enter your launcher username and game-account credentials first.".into());
    }
    if !cfg.match_notifications_configured {
        return Err("Choose whether to enable game match notifications first.".into());
    }
    // Confirm that the saved credential can be opened for this Windows user.
    config::read_password(&cfg.password_enc, &cfg.password_mode)?;
    let folder = cfg.game_folder.clone();
    if find_build_root(&folder, 2).is_none() {
        return Err("Select an existing game build or finish downloading Build 4.5 first.".into());
    }
    cfg.setup_complete = true;
    let saved = cfg.clone();
    drop(cfg);
    config::save(&state.app_data_dir, &saved)
        .map_err(|error| format!("Could not save completed setup: {error}"))?;
    write_log(&state, "first-run setup completed");
    Ok(true)
}

/* Folder selection and inspection */
#[tauri::command]
pub async fn choose_folder(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    if install_busy(&state) { return Ok(None); }
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| { let _ = tx.send(path.map(|p| p.to_string())); });
    let picked = rx.await.ok().flatten();
    if let Some(folder) = &picked {
        state.config.lock().unwrap().game_folder = folder.clone();
        save_cfg(&state)?;
        let mut phase = state.install_phase.lock().unwrap();
        if phase.as_str() == "done" || phase.as_str() == "error" { *phase = "idle".into(); }
        write_log(&state, &format!("install folder selected: {folder}"));
    }
    Ok(picked)
}

#[tauri::command]
pub async fn check_folder(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let folder = state.config.lock()
        .map_err(|_| "Launcher configuration is temporarily unavailable. Restart the launcher and try again.".to_string())?
        .game_folder.clone();

    let result = tokio::task::spawn_blocking(move || {
        if folder.is_empty() {
            return serde_json::json!({"ok":false,"installed":false,"message":"Choose a folder first."});
        }
        let mut entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                return serde_json::json!({
                    "ok":false,
                    "installed":false,
                    "message":format!("The selected folder cannot be accessed: {error}")
                });
            }
        };
        // Only inspect the first entry; counting a large folder is unnecessary.
        let has_entries = match entries.next() {
            Some(Ok(_)) => true,
            Some(Err(error)) => {
                return serde_json::json!({
                    "ok":false,
                    "installed":false,
                    "message":format!("The selected folder could not be read: {error}")
                });
            }
            None => false,
        };
        if let Some(build_root) = find_build_root(&folder, 2) {
            return serde_json::json!({
                "ok":true,
                "installed":true,
                "message":format!("Build found: {}", build_root.display())
            });
        }
        serde_json::json!({
            "ok":true,
            "installed":false,
            "message":if has_entries {
                "Folder accessible, but no complete game build was found."
            } else {
                "Folder is empty. Use Download to install the build here."
            }
        })
    }).await.map_err(|error| format!("Folder verification worker failed: {error}"))?;

    write_log(&state, &format!("folder verification: {}", result["message"].as_str().unwrap_or("completed")));
    Ok(result)
}

#[tauri::command]
pub fn open_folder(state: State<AppState>) -> bool {
    let folder = state.config.lock().unwrap().game_folder.clone();
    !folder.is_empty() && opener::open(folder).is_ok()
}

#[tauri::command]
pub fn folder_info(state: State<AppState>) -> serde_json::Value {
    let folder = state.config.lock().map(|cfg| cfg.game_folder.clone()).unwrap_or_default();
    let empty = !folder.is_empty() && fs::read_dir(&folder)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    serde_json::json!({"path":folder,"empty":empty})
}

/* Account and notification preferences */
#[tauri::command]
pub fn save_account(app: AppHandle, state: State<AppState>, display_name: Option<String>, email: Option<String>, password: Option<String>) -> serde_json::Value {
    let display_name = display_name.unwrap_or_default().trim().to_string();
    let email = email.unwrap_or_default().trim().to_string();
    let password = Zeroizing::new(password.unwrap_or_default());
    if display_name.len() > 24 || email.len() > 200 || password.len() > 200
        || display_name.chars().any(|c| c == '\r' || c == '\n') || email.chars().any(|c| c == '\r' || c == '\n') || password.chars().any(|c| c == '\r' || c == '\n') {
        return serde_json::json!({"ok":false,"message":"Invalid username or account credentials."});
    }
    {
        let mut cfg = state.config.lock().unwrap();
        if !display_name.is_empty() { cfg.display_name = display_name; }
        cfg.email = email.clone();
        if !password.is_empty() {
            match config::store_password(password.as_str()) {
                Ok((enc, mode)) => { cfg.password_enc = enc; cfg.password_mode = mode; }
                Err(message) => return serde_json::json!({"ok":false,"message":message}),
            }
        } else if email.is_empty() {
            cfg.password_enc.clear();
            cfg.password_mode.clear();
        }
    }
    if let Err(message) = save_cfg(&state) { return serde_json::json!({"ok":false,"message":message}); }
    write_log(&state, "account settings saved");
    let endpoints = state.endpoints.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { live::fetch_profile(&app2, &endpoints, &email).await; });
    serde_json::json!({"ok":true,"hasPassword":!state.config.lock().unwrap().password_enc.is_empty()})
}


#[tauri::command]
pub fn save_match_notifications(state: State<AppState>, enabled: bool) -> Result<serde_json::Value, String> {
    {
        let mut cfg = state.config.lock()
            .map_err(|_| "Launcher configuration is temporarily unavailable.".to_string())?;
        cfg.match_notifications = enabled;
        cfg.match_notifications_configured = true;
    }
    save_cfg(&state)?;
    write_log(&state, if enabled { "match notifications enabled" } else { "match notifications disabled" });
    Ok(serde_json::json!({"ok":true,"enabled":enabled,"configured":true}))
}

/* Internal pre-launch validation (not exposed as a user-facing debug screen). */

#[tauri::command]
pub async fn preflight(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = state.config.lock().unwrap().clone();
    let mut problems = Vec::<String>::new();
    let folder = Path::new(&cfg.game_folder);
    if cfg.game_folder.is_empty() || !folder.exists() {
        problems.push("The game folder cannot be found. Choose it again in Settings.".into());
    }
    let root = find_build_root(&cfg.game_folder, 2);
    if root.is_none() { problems.push("The game executable is missing or the installation is incomplete.".into()); }
    if let Some(root) = root {
        let paks = root.join("FortniteGame/Content/Paks");
        let count = fs::read_dir(&paks).map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".pak")).count()).unwrap_or(0);
        if count == 0 { problems.push("The game data (.pak files) is missing.".into()); }
    }
    for row in verify_bundled_files(&state.resource_dir) {
        if !row["ok"].as_bool().unwrap_or(false) {
            problems.push(format!("{}: {}", row["label"].as_str().unwrap_or("Launcher file"), row["detail"].as_str().unwrap_or("missing")));
        }
    }
    Ok(serde_json::json!({"ok":problems.is_empty(),"problems":problems,"repair":false}))
}

/* Live state, UI settings, and small helpers */
#[tauri::command]
pub fn get_state(state: State<AppState>) -> serde_json::Value {
    let phase = state.install_phase.lock().unwrap().clone();
    let pid = *state.game_pid.lock().unwrap();
    let message = state.game_message.lock().unwrap().clone();
    serde_json::json!({
        "install":{"phase":phase},
        "game":{"state":if pid.is_some(){"running"}else{"idle"},"message":message,"pid":pid}
    })
}

#[tauri::command]
pub fn get_live_snapshot(state: State<AppState>) -> serde_json::Value { live::live_snapshot(&state.live_cache) }


#[tauri::command]
pub fn set_lang(state: State<AppState>, lang: String) -> String {
    if ["en", "fr", "nl"].contains(&lang.as_str()) { *state.ui_lang.lock().unwrap() = lang; }
    state.ui_lang.lock().unwrap().clone()
}


#[tauri::command]
pub fn list_backgrounds(state: State<AppState>) -> Vec<String> {
    list_media(&state.resource_dir.join("assets").join("backgrounds"), "assets/backgrounds", &["jpg","jpeg","png","webp"], 100)
}

#[tauri::command]
pub fn list_live(state: State<AppState>) -> Vec<String> {
    list_media(&state.resource_dir.join("assets").join("live"), "assets/live", &["mp4","webm"], 1)
}

fn list_media(path: &Path, prefix: &str, extensions: &[&str], max: usize) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path).map(|rd| rd.flatten().filter_map(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        let ext = Path::new(&name).extension()?.to_string_lossy().to_ascii_lowercase();
        if extensions.contains(&ext.as_str()) { Some(format!("{prefix}/{name}")) } else { None }
    }).collect()).unwrap_or_default();
    names.sort();
    names.truncate(max);
    names
}


#[tauri::command]
pub async fn get_shop(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let api = state.endpoints.api_url.trim_end_matches('/').to_string();
    if !api.starts_with("https://") { return Ok(serde_json::json!({"ok":false,"message":"The shop address is not set (apiUrl in endpoints.json)."})); }
    let response = match reqwest::Client::new().get(format!("{api}/fortnite/api/storefront/v2/catalog")).timeout(std::time::Duration::from_secs(10)).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return Ok(serde_json::json!({"ok":false,"message":format!("The shop server returned HTTP {}.",r.status())})),
        Err(e) => return Ok(serde_json::json!({"ok":false,"message":format!("Could not load the shop: {e}")})),
    };
    let data: serde_json::Value = match response.json().await { Ok(v)=>v, Err(e)=>return Ok(serde_json::json!({"ok":false,"message":format!("Invalid shop response: {e}")})) };
    let mut sections = Vec::new();
    if let Some(stores) = data.get("storefronts").and_then(|v|v.as_array()) {
        for store in stores {
            let name = store.get("name").and_then(|v|v.as_str()).unwrap_or("");
            if name != "BRDailyStorefront" && name != "BRWeeklyStorefront" { continue; }
            let mut items = Vec::new();
            if let Some(entries) = store.get("catalogEntries").and_then(|v|v.as_array()) {
                for entry in entries {
                    let price = entry.get("prices").and_then(|v|v.as_array()).and_then(|v|v.first()).and_then(|v|v.get("finalPrice")).and_then(|v|v.as_i64()).unwrap_or(0);
                    let grants: Vec<String> = entry.get("itemGrants").and_then(|v|v.as_array()).map(|items|items.iter().filter_map(|g|g.get("templateId").and_then(|v|v.as_str())).take(10).map(str::to_string).collect()).unwrap_or_default();
                    if !grants.is_empty() { items.push(serde_json::json!({"price":price,"grants":grants})); }
                }
            }
            sections.push(serde_json::json!({"name":if name=="BRDailyStorefront"{"daily"}else{"featured"},"items":items}));
        }
    }
    let expiration = data.get("expiration").and_then(|v|v.as_str()).filter(|s|!s.starts_with("9999"));
    Ok(serde_json::json!({"ok":true,"sections":sections,"expiration":expiration}))
}

#[tauri::command]
pub async fn open_link(state: State<'_, AppState>, url: String) -> Result<bool, String> {
    let requested = match reqwest::Url::parse(&url) {
        Ok(parsed) if parsed.scheme() == "https" && parsed.host_str().is_some() => parsed,
        _ => return Ok(false),
    };

    // The Downloads page may still contain an older hard-coded archive URL.
    // Always resolve archive links from the same endpoint configuration used
    // by install_start(), so the browser link and the in-launcher download
    // cannot drift apart.
    let target = if requested.path().to_ascii_lowercase().ends_with(".7z") {
        match reqwest::Url::parse(&state.endpoints.build_archive_url) {
            Ok(configured) if configured.scheme() == "https" && configured.host_str().is_some()
                && configured.path().to_ascii_lowercase().ends_with(".7z") => configured,
            _ => return Ok(false),
        }
    } else {
        requested
    };

    Ok(opener::open(target.as_str()).is_ok())
}

#[tauri::command]
pub async fn open_official_site() -> bool { opener::open("https://projectlegacy.pages.dev/").is_ok() }

/* Download, verify, and safely extract the 7z build */
#[tauri::command]
pub async fn install_start(app: AppHandle, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if install_busy(&state) { return Ok(serde_json::json!({"ok":false,"message":"An installation is already running."})); }
    let folder = state.config.lock().unwrap().game_folder.clone();
    if folder.is_empty() { return Ok(serde_json::json!({"ok":false,"message":"Choose an installation folder first."})); }
    let url = state.endpoints.build_archive_url.clone();
    let expected_hash = state.endpoints.build_archive_sha256.trim().to_ascii_lowercase();
    if expected_hash.len() != 64 || !expected_hash.bytes().all(|b|b.is_ascii_hexdigit()) {
        return Ok(serde_json::json!({"ok":false,"message":"The build server configuration is incomplete: publish the real SHA-256 hash in resources/endpoints.json before enabling downloads."}));
    }
    if !url.to_ascii_lowercase().ends_with(".7z") { return Ok(serde_json::json!({"ok":false,"message":"The configured build URL must point to a genuine .7z archive."})); }

    let part_path = PathBuf::from(&folder).join(".launcher").join("download.part");
    if let Some(parent) = part_path.parent() { fs::create_dir_all(parent).map_err(|e|e.to_string())?; }
    let abort = AbortHandle::new();
    *state.abort.lock().unwrap() = Some(abort.clone());
    *state.install_phase.lock().unwrap() = "downloading".into();
    let _ = app.emit("install-progress", serde_json::json!({"phase":"downloading","percent":0}));
    write_log(&state, "build download started");

    let app2 = app.clone();
    let folder_path = PathBuf::from(folder);
    let phase_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let app_progress = app2.clone();
        let abort_for_download = abort.clone();
        let download_result = downloader::download(&url, &part_path, abort.clone(), move |progress| {
            let percent = if progress.total > 0 { (progress.received as f64 / progress.total as f64) * 100.0 } else { 0.0 };
            let _ = app_progress.emit("install-progress", serde_json::json!({"phase":"downloading","percent":percent,"received":progress.received,"total":progress.total,"speed":progress.speed,"eta":progress.eta}));
        }).await;
        let app_state = phase_handle.state::<AppState>();

        match download_result {
            Ok(_) => {
                if abort_for_download.reason() == AbortReason::Cancel {
                    let _ = fs::remove_file(&part_path);
                    *app_state.install_phase.lock().unwrap() = "idle".into();
                    let _ = app2.emit("install-progress", serde_json::json!({"phase":"idle"}));
                } else {
                    let actual_hash = file_sha256(&part_path);
                    match actual_hash {
                        Ok(actual) if actual == expected_hash => {
                            let marker_dir = folder_path.join(".launcher");
                            let marker = marker_dir.join("incomplete");
                            let marker_result = fs::create_dir_all(&marker_dir).and_then(|_|fs::write(&marker,b"Installation in progress. Do not launch the game."));
                            if let Err(e) = marker_result {
                                *app_state.install_phase.lock().unwrap() = "error".into();
                                let _ = app2.emit("install-progress", serde_json::json!({"phase":"error","message":format!("Could not create the install safety marker: {e}")}));
                            } else {
                                *app_state.install_phase.lock().unwrap() = "extracting".into();
                                let _ = app2.emit("install-progress", serde_json::json!({"phase":"extracting","percent":0}));
                                let archive_path = part_path.clone();
                                let target_path = folder_path.clone();
                                let cancel_token = abort.0.clone();
                                let app_progress = app2.clone();
                                let extracted = tokio::task::spawn_blocking(move || {
                                    extractor::extract(&archive_path,&target_path,cancel_token,move |progress| {
                                        let pct = if progress.total > 0 {(progress.done as f64/progress.total as f64)*100.0} else {0.0};
                                        let _ = app_progress.emit("install-progress",serde_json::json!({"phase":"extracting","percent":pct,"done":progress.done,"total":progress.total,"name":progress.name}));
                                    })
                                }).await;
                                match extracted {
                                    Ok(Ok(())) => {
                                        let _ = fs::remove_file(&marker);
                                        if find_build_root(&folder_path.to_string_lossy(),2).is_some() {
                                            let _ = fs::remove_file(&part_path);
                                            *app_state.install_phase.lock().unwrap() = "done".into();
                                            let _ = app2.emit("install-progress",serde_json::json!({"phase":"done","percent":100}));
                                            write_log(&app_state,"build installed and validated");
                                        } else {
                                            *app_state.install_phase.lock().unwrap() = "error".into();
                                            let _ = fs::write(&marker,b"Post-extraction validation failed.");
                                            let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":"Extraction finished, but the game executable was not found. The incomplete marker was retained."}));
                                        }
                                    }
                                    Ok(Err(err)) if err.0 == "cancelled" => {
                                        let _ = fs::remove_file(&part_path);
                                        *app_state.install_phase.lock().unwrap() = "idle".into();
                                        let _ = app2.emit("install-progress",serde_json::json!({"phase":"idle","message":"Installation cancelled. The partial game folder is marked incomplete."}));
                                    }
                                    Ok(Err(err)) => {
                                        *app_state.install_phase.lock().unwrap() = "error".into();
                                        let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":err.0}));
                                    }
                                    Err(err) => {
                                        *app_state.install_phase.lock().unwrap() = "error".into();
                                        let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":format!("Extraction worker failed: {err}")}));
                                    }
                                }
                            }
                        }
                        Ok(actual) => {
                            let _ = fs::remove_file(&part_path);
                            *app_state.install_phase.lock().unwrap() = "error".into();
                            let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":format!("Build SHA-256 verification failed. Expected {expected_hash}; received {actual}. The download was deleted.")}));
                        }
                        Err(message) => {
                            *app_state.install_phase.lock().unwrap() = "error".into();
                            let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":message}));
                        }
                    }
                }
            }
            Err(err) if err.aborted == AbortReason::Pause => {
                *app_state.install_phase.lock().unwrap() = "paused".into();
                let _ = app2.emit("install-progress",serde_json::json!({"phase":"paused","message":"Download paused. Resume to continue."}));
            }
            Err(err) if err.aborted == AbortReason::Cancel => {
                let _ = fs::remove_file(&part_path);
                *app_state.install_phase.lock().unwrap() = "idle".into();
                let _ = app2.emit("install-progress",serde_json::json!({"phase":"idle","message":"Download cancelled."}));
            }
            Err(err) => {
                *app_state.install_phase.lock().unwrap() = "error".into();
                let _ = app2.emit("install-progress",serde_json::json!({"phase":"error","message":err.message}));
            }
        }
        *app_state.abort.lock().unwrap() = None;
    });
    Ok(serde_json::json!({"ok":true}))
}

#[tauri::command]
pub fn install_pause(state: State<AppState>) -> bool {
    if *state.install_phase.lock().unwrap() != "downloading" { return false; }
    if let Some(abort) = state.abort.lock().unwrap().as_ref() { abort.pause(); true } else { false }
}

#[tauri::command]
pub fn install_cancel(state: State<AppState>) -> bool {
    let phase = state.install_phase.lock().unwrap().clone();
    if let Some(abort) = state.abort.lock().unwrap().as_ref() { abort.cancel(); return true; }
    if phase == "paused" {
        let folder = state.config.lock().unwrap().game_folder.clone();
        let partial = PathBuf::from(folder).join(".launcher").join("download.part");
        let _ = fs::remove_file(partial);
        *state.install_phase.lock().unwrap() = "idle".into();
        return true;
    }
    false
}

/* Game process lifecycle. No anti-cheat placeholder processes are spawned. */
#[tauri::command]
pub fn game_launch(app: AppHandle, state: State<AppState>) -> serde_json::Value {
    if state.game_pid.lock().unwrap().is_some() { return serde_json::json!({"ok":false,"message":"The game is already running."}); }
    if !cfg!(target_os="windows") { return serde_json::json!({"ok":false,"message":"The game can only be launched on Windows."}); }
    if install_busy(&state) { return serde_json::json!({"ok":false,"message":"Wait for the installation to finish."}); }
    let cfg = state.config.lock().unwrap().clone();
    let root = match find_build_root(&cfg.game_folder,2) {
        Some(path)=>path,
        None=>return serde_json::json!({"ok":false,"message":"Game build not found or incomplete. Install it or choose the correct folder in Settings."}),
    };
    if cfg.email.is_empty() || cfg.password_enc.is_empty() { return serde_json::json!({"ok":false,"needAccount":true,"message":"Enter your account email and password in Settings first."}); }
    let password = match config::read_password(&cfg.password_enc,&cfg.password_mode) {
        Ok(password)=>Zeroizing::new(password),
        Err(message)=>return serde_json::json!({"ok":false,"needAccount":true,"message":message}),
    };
    if password.is_empty() { return serde_json::json!({"ok":false,"needAccount":true,"message":"Enter your account password in Settings first."}); }
    let exe = root.join(crate::buildroot::GAME_EXE_REL.replace('/',std::path::MAIN_SEPARATOR_STR));
    if !exe.is_file() { return serde_json::json!({"ok":false,"message":"The game executable is missing."}); }
    let args = Zeroizing::new(build_launch_args(DEFAULT_ARGS,&cfg.email,password.as_str()));
    set_game_status(&app,&state,"launching","Starting the game...");
    // Drop the Command and its owned argument copies immediately after CreateProcess.
    // The legacy client still requires AUTH_PASSWORD on its command line; this OS-level
    // visibility remains until the client supports another authentication interface.
    let spawn_result = {
        let mut command = std::process::Command::new(&exe);
        command.args(args.iter().map(String::as_str)).current_dir(exe.parent().unwrap_or(&root));
        command.spawn()
    };
    match spawn_result {
        Ok(mut child) => {
            let pid = child.id();
            *state.game_pid.lock().unwrap() = Some(pid);
            set_game_status(&app,&state,"running","Game started.");
            schedule_redirector_injection(app.clone(), pid, cfg.inject_delay_ms);
            let monitor_app = app.clone();
            std::thread::spawn(move || {
                let result = child.wait();
                if let Some(monitored_state) = monitor_app.try_state::<AppState>() {
                    let mut current_pid = monitored_state.game_pid.lock().unwrap();
                    if *current_pid == Some(pid) {
                        *current_pid = None;
                        let message = match result {
                            Ok(status) if status.success()=>"The game closed.".to_string(),
                            Ok(status)=>format!("The game closed (exit code {}).",status.code().unwrap_or(-1)),
                            Err(error)=>format!("Could not monitor the game process: {error}"),
                        };
                        *monitored_state.game_message.lock().unwrap() = message.clone();
                        let _ = monitor_app.emit("game-status",serde_json::json!({"state":"idle","message":message}));
                        write_log(&monitored_state,&message);
                    }
                }
            });
            serde_json::json!({"ok":true,"pid":pid})
        }
        Err(error) => {
            set_game_status(&app,&state,"idle",format!("Could not start the game: {error}"));
            serde_json::json!({"ok":false,"message":format!("Could not start the game: {error}")})
        }
    }
}

#[tauri::command]
pub fn game_stop(app: AppHandle, state: State<AppState>) -> bool {
    let pid = *state.game_pid.lock().unwrap();
    if let Some(pid) = pid {
        #[cfg(target_os="windows")]
        {
            let result = std::process::Command::new("taskkill").args(["/PID",&pid.to_string(),"/T","/F"]).output();
            match result {
                Ok(output) if output.status.success()=>{ set_game_status(&app,&state,"stopping","Stopping the game..."); true },
                Ok(output)=>{ write_log(&state,&format!("taskkill returned {}",output.status)); false },
                Err(error)=>{ write_log(&state,&format!("taskkill failed: {error}")); false },
            }
        }
        #[cfg(not(target_os="windows"))]
        { let _ = app; false }
    } else { false }
}

/* Manual modules are loaded through the original, bundled Injector.exe.
 * Anti-cheat placeholder processes and security-product exclusions are separate
 * legacy behaviours and are not started by this launcher. */
#[tauri::command]
pub async fn inject_module(app: AppHandle, key: String) -> serde_json::Value {
    // Retrieve managed state from the owned AppHandle so this async command has no borrowed inputs.
    let state = app.state::<AppState>();
    let pid = match *state.game_pid.lock().unwrap() {
        Some(pid) => pid,
        None => return serde_json::json!({"ok":false,"message":"Start the game first."}),
    };
    let (file_name, label) = match key.as_str() {
        "console" => (CONSOLE_FILE, "Console"),
        "anticrash" => ("anticrash.dll", "Anti-crash"),
        _ => return serde_json::json!({"ok":false,"message":"Unknown module."}),
    };
    let module_path = state.resource_dir.join("bin").join(file_name);
    if !module_path.is_file() {
        return serde_json::json!({"ok":false,"message":format!("{file_name} is missing from the launcher's bin folder.")});
    }

    let mut temporary_dll: Option<PathBuf> = None;
    let target_path = if key == "console" {
        let key_bytes = match console_key(&state) {
            Some(value) => value,
            None => return serde_json::json!({"ok":false,"locked":true,"message":"Enter your access code first (Settings > Gameplay)."}),
        };
        let encrypted = match fs::read(&module_path) {
            Ok(bytes) => bytes,
            Err(error) => return serde_json::json!({"ok":false,"message":format!("Could not read {file_name}: {error}")}),
        };
        let clear = match decrypt_console_payload_bytes(&encrypted, &key_bytes) {
            Ok(bytes) => bytes,
            Err(_) => return serde_json::json!({"ok":false,"locked":true,"message":"Your access code is no longer valid. Enter it again."}),
        };
        let temp_name = format!("plc-{}-{}.dll", pid, chrono::Utc::now().timestamp_micros());
        let temp_path = std::env::temp_dir().join(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&temp_path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&clear).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&temp_path);
                    return serde_json::json!({"ok":false,"message":format!("Could not prepare console module: {error}")});
                }
                temporary_dll = Some(temp_path.clone());
                temp_path
            }
            Err(error) => return serde_json::json!({"ok":false,"message":format!("Could not prepare console module: {error}")}),
        }
    } else {
        module_path
    };

    let result = run_bundled_injector(&state, pid, &target_path).await;
    if let Some(path) = temporary_dll { schedule_temp_dll_cleanup(path); }
    match result {
        Ok(_) => {
            let message = if key == "console" { "Console injected. Press F3 in-game.".to_string() } else { format!("{label} injected.") };
            set_game_status(&app, &state, "running", message.clone());
            serde_json::json!({"ok":true,"message":message})
        }
        Err(error) => {
            let message = format!("{label} injection failed: {error}");
            write_log(&state, &message);
            set_game_status(&app, &state, "running", message.clone());
            serde_json::json!({"ok":false,"message":message})
        }
    }
}


#[tauri::command]
pub fn console_status(state: State<AppState>) -> serde_json::Value { console_status_for(&state) }

#[tauri::command]
pub fn console_unlock(state: State<AppState>, code: String) -> serde_json::Value {
    if let Some(until) = *state.console_unlock_locked_until.lock().unwrap() {
        let now = Instant::now();
        if until > now {
            return serde_json::json!({"ok":false,"message":format!("Too many attempts. Try again in {}s.", (until-now).as_secs().saturating_add(1))});
        }
    }
    let file = state.resource_dir.join("bin").join(CONSOLE_FILE);
    let bytes = match fs::read(&file) { Ok(bytes)=>bytes, Err(_)=>return serde_json::json!({"ok":false,"message":"The encrypted console module is missing from launcher resources."}) };
    if bytes.len()<48 || &bytes[..4]!=b"PLE1" { return serde_json::json!({"ok":false,"message":"The console module has an unsupported file format."}); }
    let normalized: String = code.to_ascii_uppercase().chars().filter(|c|c.is_ascii_alphanumeric()).collect();
    if normalized.len()<8 { return serde_json::json!({"ok":false,"message":"Enter a valid access code."}); }
    let params = match scrypt::Params::new(15, 8, 1, 32) { Ok(p)=>p,Err(_)=>return serde_json::json!({"ok":false,"message":"Secure key derivation is unavailable."}) };
    let mut key = [0u8;32];
    if scrypt::scrypt(normalized.as_bytes(),&bytes[4..20],&params,&mut key).is_err() || !console_ciphertext_valid_bytes(&bytes,&key) {
        let mut failures = state.console_unlock_failures.lock().unwrap();
        *failures = failures.saturating_add(1);
        if *failures >= 5 {
            let delay = Duration::from_secs(30 * u64::from(*failures - 4));
            *state.console_unlock_locked_until.lock().unwrap() = Some(Instant::now() + delay);
        }
        write_log(&state,"console access-code verification failed");
        return serde_json::json!({"ok":false,"message":"Wrong access code."});
    }
    *state.console_unlock_failures.lock().unwrap() = 0;
    *state.console_unlock_locked_until.lock().unwrap() = None;
    let key_hex = hex::encode(key);
    match config::store_password(&key_hex) {
        Ok((enc,mode))=>{
            let mut cfg = state.config.lock().unwrap();
            cfg.console_key_enc = enc;
            cfg.console_key_mode = mode;
            drop(cfg);
            if let Err(message)=save_cfg(&state) { return serde_json::json!({"ok":false,"message":message}); }
            write_log(&state,"console access code verified and key stored with Windows DPAPI");
            serde_json::json!({"ok":true,"message":"Access code verified and saved securely."})
        }
        Err(message)=>serde_json::json!({"ok":false,"message":message}),
    }
}

#[tauri::command]
pub fn console_lock(state: State<AppState>) -> serde_json::Value {
    let mut cfg = state.config.lock().unwrap();
    cfg.console_key_enc.clear();
    cfg.console_key_mode.clear();
    drop(cfg);
    match save_cfg(&state) { Ok(())=>serde_json::json!({"ok":true}),Err(message)=>serde_json::json!({"ok":false,"message":message}) }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

/// Checks the signed GitHub Releases manifest configured in tauri.conf.json.
#[tauri::command]
pub async fn check_launcher_update(app: AppHandle) -> Result<Option<LauncherUpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|error| format!("Could not configure launcher updater: {error}"))?;
    let update = updater
        .check()
        .await
        .map_err(|error| format!("Could not check for launcher updates: {error}"))?;

    Ok(update.map(|update| LauncherUpdateInfo {
        version: update.version,
        current_version: update.current_version,
        notes: update.body,
        published_at: update.date.map(|date| date.to_string()),
    }))
}

/// Re-checks the latest signed release, downloads it, verifies its signature,
/// and installs it. Progress is emitted to the WebView as a lightweight event.
#[tauri::command]
pub async fn install_launcher_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|error| format!("Could not configure launcher updater: {error}"))?;
    let update = updater
        .check()
        .await
        .map_err(|error| format!("Could not check for launcher updates: {error}"))?
        .ok_or_else(|| "No launcher update is currently available.".to_string())?;

    let target_version = update.version.clone();
    let progress_app = app.clone();
    let mut downloaded: u64 = 0;

    update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let percent = content_length
                    .filter(|length| *length > 0)
                    .map(|length| ((downloaded.saturating_mul(100) / length).min(100)) as u8);
                let _ = progress_app.emit(
                    "launcher-update-progress",
                    serde_json::json!({
                        "state": "downloading",
                        "downloaded": downloaded,
                        "contentLength": content_length,
                        "percent": percent
                    }),
                );
            },
            || {
                let _ = progress_app.emit(
                    "launcher-update-progress",
                    serde_json::json!({ "state": "installing" }),
                );
            },
        )
        .await
        .map_err(|error| format!("Launcher update installation failed: {error}"))?;

    let _ = app.emit(
        "launcher-update-progress",
        serde_json::json!({ "state": "installed", "version": target_version }),
    );
    // On Windows the updater's NSIS installer will close the app as part of
    // the installation flow. Explicit restart is still needed on other OSes.
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_runtime_hash_manifest_has_three_entries() {
        assert_eq!(EXPECTED_BIN_HASHES.len(),3);
        assert!(EXPECTED_BIN_HASHES.iter().all(|(_,h)|h.len()==64));
    }
    #[test]
    fn console_payload_requires_magic_header() {
        assert!(!console_ciphertext_valid_bytes(b"not a PLE file", &[0;32]));
    }
}


#[cfg(test)]
mod public_config_tests {
    use super::PublicConfig;

    #[test]
    fn public_config_uses_camel_case_for_frontend() {
        let value = serde_json::to_value(PublicConfig {
            game_folder: String::new(),
            email: String::new(),
            display_name: "LegacyPlayer".to_string(),
            match_notifications: true,
            match_notifications_configured: true,
            has_password: true,
            password_reentry_required: false,
            has_anticrash: false,
            installed: true,
            build_root: String::new(),
            platform: "win32".to_string(),
            setup_complete: true,
        }).unwrap();
        assert_eq!(value["setupComplete"], true);
        assert_eq!(value["displayName"], "LegacyPlayer");
        assert_eq!(value["matchNotifications"], true);
        assert_eq!(value["matchNotificationsConfigured"], true);
        assert!(value.get("setup_complete").is_none());
    }
}
