use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::args::DEFAULT_ARGS;

/// The config file intentionally remains compatible with the Electron launcher's
/// camelCase JSON. Legacy snake_case keys from early Tauri builds are also accepted.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    #[serde(alias = "game_folder")]
    pub game_folder: String,
    pub email: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
    pub match_notifications: bool,
    pub match_notifications_configured: bool,
    #[serde(alias = "password_enc")]
    pub password_enc: String,
    #[serde(alias = "password_mode")]
    pub password_mode: String,
    #[serde(alias = "extra_args")]
    pub extra_args: String,
    #[serde(alias = "use_stubs")]
    pub use_stubs: bool,
    pub compat: bool,
    pub setup_complete: bool,
    #[serde(alias = "inject_delay_ms")]
    pub inject_delay_ms: u64,
    #[serde(alias = "inject_delay_version")]
    pub inject_delay_version: u32,
    pub console_key_enc: String,
    pub console_key_mode: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            game_folder: String::new(),
            email: String::new(),
            display_name: String::new(),
            match_notifications: false,
            match_notifications_configured: false,
            password_enc: String::new(),
            password_mode: String::new(),
            extra_args: DEFAULT_ARGS.to_string(),
            // The legacy Electron option starts anti-cheat placeholder processes.
            // It is retained only for config compatibility; Tauri does not run them.
            use_stubs: false,
            compat: false,
            setup_complete: false,
            inject_delay_ms: 2500,
            inject_delay_version: 2,
            console_key_enc: String::new(),
            console_key_mode: String::new(),
        }
    }
}

pub fn config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("launcher-config.json")
}

/// Candidate Electron user-data locations on Windows. Electron normally uses
/// the application name; packaged builds can expose either the package name
/// or product name, while Tauri uses the application identifier. Check these
/// exact launcher-specific folders only (never scan the whole profile).
pub fn legacy_user_data_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let root = PathBuf::from(appdata);
        for name in ["project-legacy-launcher", "Project Legacy Launcher", "gg.projectlegacy.launcher"] {
            let candidate = root.join(name);
            if !paths.contains(&candidate) { paths.push(candidate); }
        }
    }
    paths
}

pub fn load(app_data_dir: &Path) -> Config {
    let mut candidates = vec![config_path(app_data_dir)];
    for dir in legacy_user_data_dirs() {
        let candidate = config_path(&dir);
        if !candidates.contains(&candidate) { candidates.push(candidate); }
    }

    let mut cfg = candidates.iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .find_map(|text| serde_json::from_str::<Config>(&text).ok())
        .unwrap_or_default();

    // Launch parameters are centrally managed by the launcher and cannot be
    // changed by end users. This also resets any custom values from old builds.
    cfg.extra_args = DEFAULT_ARGS.to_string();
    cfg.compat = false; // compatibility mode is fixed by the launcher, not user-configurable
    if cfg.inject_delay_version != 2 || !(500..=30_000).contains(&cfg.inject_delay_ms) {
        cfg.inject_delay_ms = 2500;
        cfg.inject_delay_version = 2;
    }
    cfg
}

pub fn save(app_data_dir: &Path, cfg: &Config) -> io::Result<()> {
    fs::create_dir_all(app_data_dir)?;
    let path = config_path(app_data_dir);
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&temp, bytes)?;
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(temp, path)
}

/// Store a password using Windows DPAPI (CurrentUser scope). The returned mode
/// is persisted beside the ciphertext so legacy Electron formats can be handled.
pub fn store_password(password: &str) -> Result<(String, String), String> {
    if password.is_empty() {
        return Ok((String::new(), String::new()));
    }
    #[cfg(windows)]
    {
        let protected = dpapi::protect(password.as_bytes())
            .ok_or_else(|| "Windows could not encrypt the password with DPAPI.".to_string())?;
        use base64::Engine;
        return Ok((
            base64::engine::general_purpose::STANDARD.encode(protected),
            "dpapi".to_string(),
        ));
    }
    #[cfg(not(windows))]
    {
        // This branch supports developer/test builds only. A Windows release uses DPAPI.
        use base64::Engine;
        Ok((
            base64::engine::general_purpose::STANDARD.encode(password.as_bytes()),
            "plain".to_string(),
        ))
    }
}

pub fn read_password(enc: &str, mode: &str) -> Result<String, String> {
    if enc.is_empty() {
        return Ok(String::new());
    }
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(enc)
        .map_err(|_| "The saved password cannot be decoded. Please enter it again.".to_string())?;

    match mode {
        "dpapi" => {
            #[cfg(windows)]
            {
                let clear = dpapi::unprotect(&bytes)
                    .ok_or_else(|| "Windows could not decrypt the saved password. Please enter it again.".to_string())?;
                String::from_utf8(clear)
                    .map_err(|_| "The saved password is invalid. Please enter it again.".to_string())
            }
            #[cfg(not(windows))]
            {
                Err("This password is encrypted for the Windows user account. Open the launcher on that Windows account or enter the password again.".to_string())
            }
        }
        // Electron safeStorage on Windows uses DPAPI and stores the encrypted
        // Buffer as Base64 in launcher-config.json. Decrypt it with the same
        // Windows user profile, then migrate it to this launcher's DPAPI format.
        "safe" => {
            #[cfg(windows)]
            {
                let clear = dpapi::unprotect(&bytes)
                    .ok_or_else(|| "The previous Electron password could not be migrated. Please enter it again.".to_string())?;
                String::from_utf8(clear)
                    .map_err(|_| "The previous Electron password is invalid. Please enter it again.".to_string())
            }
            #[cfg(not(windows))]
            {
                Err("Electron safeStorage credentials can only be migrated on the original Windows user account.".to_string())
            }
        }
        // Older Electron fallback and early Tauri files used Base64 only.
        "" | "plain" => String::from_utf8(bytes)
            .map_err(|_| "The saved password is invalid. Please enter it again.".to_string()),
        _ => Err("The saved password uses an unknown encryption format. Please enter it again.".to_string()),
    }
}

/// Migrate legacy Electron password formats to the Tauri DPAPI envelope.
/// Base64-only fallback and Windows safeStorage values are both supported.
pub fn migrate_legacy_password(cfg: &mut Config) -> bool {
    if cfg.password_enc.is_empty() || !(cfg.password_mode.is_empty() || cfg.password_mode == "plain" || cfg.password_mode == "safe") {
        return false;
    }
    let password = match read_password(&cfg.password_enc, &cfg.password_mode) {
        Ok(p) if !p.is_empty() => p,
        _ => return false,
    };
    match store_password(&password) {
        Ok((enc, mode)) => {
            cfg.password_enc = enc;
            cfg.password_mode = mode;
            true
        }
        Err(_) => false,
    }
}

/// Electron stored the verified console key in a separate `console-key.bin`
/// file encrypted by safeStorage. On Windows that payload is DPAPI-protected;
/// migrate it into launcher-config.json so Tauri no longer depends on the
/// Electron app-data directory. The original file is left untouched as backup.
pub fn migrate_legacy_console_key(cfg: &mut Config, app_data_dir: &Path) -> bool {
    if !cfg.console_key_enc.is_empty() { return false; }
    #[cfg(not(windows))]
    {
        let _ = app_data_dir;
        return false;
    }
    #[cfg(windows)]
    {
        let mut dirs = vec![app_data_dir.to_path_buf()];
        for dir in legacy_user_data_dirs() {
            if !dirs.contains(&dir) { dirs.push(dir); }
        }
        for dir in dirs {
            let encrypted = match fs::read(dir.join("console-key.bin")) { Ok(bytes) => bytes, Err(_) => continue };
            let clear = match dpapi::unprotect(&encrypted) { Some(bytes) => bytes, None => continue };
            let key_hex = match String::from_utf8(clear) { Ok(value) => value, Err(_) => continue };
            match hex::decode(&key_hex) { Ok(value) if value.len() == 32 => {}, _ => continue }
            if let Ok((enc, mode)) = store_password(&key_hex) {
                cfg.console_key_enc = enc;
                cfg.console_key_mode = mode;
                return true;
            }
        }
        false
    }
}

/// Copy only known, non-secret support files from a previous Electron user-data
/// directory. Config and the console key are migrated separately above.
pub fn migrate_legacy_support_files(app_data_dir: &Path) {
    let mut dirs = legacy_user_data_dirs();
    dirs.push(app_data_dir.to_path_buf());
    for filename in ["news-cache.json", "launcher.log"] {
        let destination = app_data_dir.join(filename);
        if destination.exists() { continue; }
        for dir in &dirs {
            let source = dir.join(filename);
            if source == destination || !source.is_file() { continue; }
            if let Some(parent) = destination.parent() { let _ = fs::create_dir_all(parent); }
            if fs::copy(&source, &destination).is_ok() { break; }
        }
    }
}

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::slice;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "Crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "Kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    fn copy_and_free(blob: &mut DataBlob) -> Option<Vec<u8>> {
        if blob.pb_data.is_null() || blob.cb_data == 0 {
            return None;
        }
        let bytes = unsafe { slice::from_raw_parts(blob.pb_data, blob.cb_data as usize).to_vec() };
        unsafe {
            let _ = LocalFree(blob.pb_data.cast());
        }
        blob.pb_data = null_mut();
        blob.cb_data = 0;
        Some(bytes)
    }

    pub fn protect(clear: &[u8]) -> Option<Vec<u8>> {
        if clear.is_empty() || clear.len() > u32::MAX as usize {
            return None;
        }
        let input = DataBlob { cb_data: clear.len() as u32, pb_data: clear.as_ptr() as *mut u8 };
        let mut output = DataBlob { cb_data: 0, pb_data: null_mut() };
        let ok = unsafe {
            CryptProtectData(
                &input,
                null(),
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 { return None; }
        copy_and_free(&mut output)
    }

    pub fn unprotect(cipher: &[u8]) -> Option<Vec<u8>> {
        if cipher.is_empty() || cipher.len() > u32::MAX as usize { return None; }
        let input = DataBlob { cb_data: cipher.len() as u32, pb_data: cipher.as_ptr() as *mut u8 };
        let mut output = DataBlob { cb_data: 0, pb_data: null_mut() };
        let mut description: *mut u16 = null_mut();
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                &mut description,
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if !description.is_null() { unsafe { let _ = LocalFree(description.cast()); } }
        if ok == 0 { return None; }
        copy_and_free(&mut output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_electron_camel_case_settings() {
        let c: Config = serde_json::from_str(r#"{
            "gameFolder":"C:/Games/Legacy",
            "email":"player@example.test",
            "passwordEnc":"YWJj",
            "passwordMode":"plain",
            "extraArgs":"-HTTP=WinINet -log",
            "useStubs":true,
            "compat":true,
            "injectDelayMs":2500,
            "injectDelayVersion":2
        }"#).unwrap();
        assert_eq!(c.game_folder, "C:/Games/Legacy");
        assert_eq!(c.password_mode, "plain");
        assert!(c.compat);
        assert_eq!(c.inject_delay_ms, 2500);
    }


    #[test]
    fn first_run_preferences_and_completion_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "project-legacy-config-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut config = Config::default();
        config.display_name = "LegacyPlayer".to_string();
        config.email = "player@example.test".to_string();
        config.match_notifications = true;
        config.match_notifications_configured = true;
        config.setup_complete = true;
        config.extra_args = "-custom-user-argument".to_string();
        save(&path, &config).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.display_name, "LegacyPlayer");
        assert!(loaded.match_notifications);
        assert!(loaded.match_notifications_configured);
        assert!(loaded.setup_complete);
        assert_eq!(loaded.extra_args, DEFAULT_ARGS);

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn invalid_electron_safe_storage_blob_is_not_read_as_plaintext() {
        let result = read_password("YWJj", "safe");
        assert!(result.is_err());
    }

    #[test]
    fn legacy_user_data_paths_are_launcher_scoped() {
        let paths = legacy_user_data_dirs();
        assert!(paths.iter().all(|p| p.to_string_lossy().to_lowercase().contains("project-legacy-launcher")
            || p.to_string_lossy().to_lowercase().contains("project legacy launcher")
            || p.to_string_lossy().to_lowercase().contains("gg.projectlegacy.launcher")));
    }

    #[test]
    fn base64_legacy_password_can_be_read() {
        use base64::Engine;
        let enc = base64::engine::general_purpose::STANDARD.encode("example-password");
        assert_eq!(read_password(&enc, "plain").unwrap(), "example-password");
    }
}
