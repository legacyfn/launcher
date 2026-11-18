use crate::commands::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    #[serde(default = "d_xmpp")]
    pub xmpp_server_url: String,
    #[serde(default = "d_build")]
    pub build_archive_url: String,
    #[serde(default = "d_news")]
    pub news_url: String,
    #[serde(default = "d_health_secs")]
    pub health_poll_seconds: u64,
    #[serde(default)]
    pub build_archive_sha256: String,
    #[serde(default)]
    pub discord_invite: String,
    #[serde(default)]
    pub profile_url: String,
    #[serde(default)]
    pub api_url: String,
}

fn d_xmpp() -> String { "https://projectlegacymain.streamxerosradio.duckdns.org".into() }
fn d_build() -> String { "https://projectlegacybuilds.streamxerosradio.duckdns.org/builds/4.5.7z".into() }
fn d_news() -> String { "https://projectlegacy.pages.dev/news.json".into() }
fn d_health_secs() -> u64 { 20 }

pub fn load_endpoints(resource_dir: &std::path::Path) -> Endpoints {
    let path = resource_dir.join("endpoints.json");
    let mut endpoints: Endpoints = std::fs::read_to_string(path)
        .ok().and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::from_str("{}").expect("default endpoints are valid"));

    // Only HTTPS endpoints are accepted for network requests.
    if !endpoints.xmpp_server_url.starts_with("https://") { endpoints.xmpp_server_url = d_xmpp(); }
    if !endpoints.build_archive_url.starts_with("https://") { endpoints.build_archive_url = d_build(); }
    if !endpoints.news_url.starts_with("https://") { endpoints.news_url = d_news(); }
    if !endpoints.api_url.is_empty() && !endpoints.api_url.starts_with("https://") { endpoints.api_url.clear(); }
    if !endpoints.profile_url.is_empty() && !endpoints.profile_url.starts_with("https://") { endpoints.profile_url.clear(); }
    endpoints
}

const USER_AGENT: &str = "ProjectLegacyLauncher/4.3";

#[derive(Default)]
pub struct LiveCache {
    pub latest: Mutex<HashMap<String, Value>>,
    pub matches: Mutex<HashMap<String, Value>>,
}

/// Emit an event and retain its latest value so the UI can recover events that
/// arrived before its JavaScript listeners were registered.
pub fn emit_cached(app: &AppHandle, event: &str, payload: Value) {
    if let Some(state) = app.try_state::<AppState>() {
        match event {
            "match-event" => {
                if let Some(mode) = payload.get("mode").and_then(Value::as_str) {
                    state.live_cache.matches.lock().unwrap().insert(mode.to_string(), payload.clone());
                } else if let Some(amount) = payload.get("amount") {
                    state.live_cache.latest.lock().unwrap().insert(
                        "clients-update".into(),
                        serde_json::json!({ "ok": true, "amount": amount, "clients": null }),
                    );
                }
            }
            _ => { state.live_cache.latest.lock().unwrap().insert(event.to_string(), payload.clone()); }
        }
    }
    let _ = app.emit(event, payload);
}

pub fn live_snapshot(cache: &LiveCache) -> Value {
    let latest = cache.latest.lock().unwrap().clone();
    let matches = cache.matches.lock().unwrap().clone();
    serde_json::json!({ "latest": latest, "matches": matches })
}

pub fn spawn_health_poll(app: AppHandle, endpoints: Endpoints) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/health", endpoints.xmpp_server_url.trim_end_matches('/'));
        let secs = endpoints.health_poll_seconds.max(10);
        loop {
            let ok = client.get(&url).header("User-Agent", USER_AGENT)
                .timeout(Duration::from_secs(6)).send().await
                .map(|r| r.status().is_success()).unwrap_or(false);
            emit_cached(&app, "health-status", serde_json::json!({ "ok": ok }));
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    });
}

fn sanitize_news(raw: Value) -> Option<Value> {
    let posts = raw.get("posts")?.as_array()?;
    let cleaned: Vec<Value> = posts.iter().filter_map(|post| {
        let id = post.get("id")?.as_str()?;
        let title = post.get("title")?.as_object()?;
        let summary = post.get("summary").and_then(Value::as_object);
        Some(serde_json::json!({
            "id": id,
            "date": post.get("date").and_then(Value::as_str).unwrap_or(""),
            "title": title,
            "summary": summary,
            "url": post.get("url").and_then(Value::as_str).filter(|u| u.starts_with("https://")).unwrap_or("")
        }))
    }).take(100).collect();
    Some(serde_json::json!({
        "updated": raw.get("updated").and_then(Value::as_str).unwrap_or(""),
        "posts": cleaned
    }))
}

pub fn spawn_news_poll(app: AppHandle, endpoints: Endpoints, app_data_dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let cache_path = app_data_dir.join("news-cache.json");
        if let Ok(text) = tokio::fs::read_to_string(&cache_path).await {
            if let Ok(raw) = serde_json::from_str::<Value>(&text) {
                if let Some(news) = sanitize_news(raw) {
                    emit_cached(&app, "news-update", news);
                }
            }
        }
        let client = reqwest::Client::new();
        loop {
            let result = client.get(&endpoints.news_url).header("User-Agent", USER_AGENT)
                .timeout(Duration::from_secs(8)).send().await;
            if let Ok(response) = result {
                if response.status().is_success() {
                    if let Ok(raw) = response.json::<Value>().await {
                        if let Some(news) = sanitize_news(raw) {
                            if let Ok(bytes) = serde_json::to_vec(&news) {
                                let _ = tokio::fs::create_dir_all(&app_data_dir).await;
                                let _ = tokio::fs::write(&cache_path, bytes).await;
                            }
                            emit_cached(&app, "news-update", news);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(12 * 60)).await;
        }
    });
}

#[derive(Serialize, Clone)]
struct DiscordCount { members: i64, online: i64 }

pub fn spawn_discord_poll(app: AppHandle, endpoints: Endpoints) {
    if endpoints.discord_invite.trim().is_empty() { return; }
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let invite = urlencoding::encode(&endpoints.discord_invite);
        let url = format!("https://discord.com/api/v9/invites/{invite}?with_counts=true");
        loop {
            if let Ok(response) = client.get(&url).header("User-Agent", USER_AGENT)
                .timeout(Duration::from_secs(8)).send().await {
                if let Ok(json) = response.json::<Value>().await {
                    if let Some(members) = json.get("approximate_member_count").and_then(Value::as_i64) {
                        let online = json.get("approximate_presence_count").and_then(Value::as_i64).unwrap_or(0);
                        emit_cached(&app, "discord-count", serde_json::to_value(DiscordCount { members, online }).unwrap_or(Value::Null));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
}

pub async fn fetch_profile(app: &AppHandle, endpoints: &Endpoints, email: &str) {
    if email.trim().is_empty() { return; }
    let client = reqwest::Client::new();
    let encoded = urlencoding::encode(email).into_owned();
    let mut urls = Vec::new();
    if endpoints.profile_url.starts_with("https://") {
        urls.push(endpoints.profile_url.replace("{email}", &encoded));
    }
    let api = endpoints.api_url.trim_end_matches('/');
    if api.starts_with("https://") {
        urls.push(format!("{api}/account/api/public/account/email/{encoded}"));
    }
    for url in urls {
        if let Ok(response) = client.get(&url).header("User-Agent", USER_AGENT)
            .timeout(Duration::from_secs(10)).send().await {
            if let Ok(json) = response.json::<Value>().await {
                let name = json.get("displayName").or_else(|| json.get("username")).or_else(|| json.get("name")).and_then(Value::as_str);
                if let Some(name) = name.filter(|n| !n.trim().is_empty() && n.len() <= 40) {
                    emit_cached(app, "profile-update", serde_json::json!({ "name": name.trim() }));
                    return;
                }
            }
        }
    }
}

pub fn spawn_clients_poll(app: AppHandle, endpoints: Endpoints) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/clients", endpoints.xmpp_server_url.trim_end_matches('/'));
        loop {
            let mut payload = serde_json::json!({ "ok": false, "amount": null, "clients": null });
            if let Ok(response) = client.get(&url).header("User-Agent", USER_AGENT)
                .timeout(Duration::from_secs(8)).send().await {
                if response.status().is_success() {
                    if let Ok(data) = response.json::<Value>().await {
                        let amount = data.get("amount").and_then(Value::as_u64);
                        let clients: Vec<String> = data.get("clients").and_then(Value::as_array).map(|items| {
                            items.iter().filter_map(Value::as_str).map(|s| s.trim().chars().take(40).collect::<String>())
                                .filter(|s| !s.is_empty()).take(100).collect()
                        }).unwrap_or_default();
                        if let Some(amount) = amount {
                            payload = serde_json::json!({ "ok": true, "amount": amount, "clients": clients });
                        }
                    }
                }
            }
            emit_cached(&app, "clients-update", payload);
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

/// Server-Sent-Events stream with automatic reconnect and last-event replay.
pub fn spawn_events_stream(app: AppHandle, endpoints: Endpoints) {
    tauri::async_runtime::spawn(async move {
        let url = format!("{}/events", endpoints.xmpp_server_url.trim_end_matches('/'));
        loop {
            if run_events_once(&app, &url).await.is_err() {
                emit_cached(&app, "network-status", serde_json::json!({ "events": false }));
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn run_events_once(app: &AppHandle, url: &str) -> Result<(), reqwest::Error> {
    use futures_util::StreamExt;
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build().unwrap_or_default();
    let response = client.get(url).header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache").send().await?;
    if !response.status().is_success() {
        emit_cached(app, "network-status", serde_json::json!({ "events": false }));
        return Ok(());
    }
    emit_cached(app, "network-status", serde_json::json!({ "events": true }));
    let mut buffer = String::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        if buffer.len() > 1024 * 1024 { buffer.clear(); }
        buffer = buffer.replace("\r\n", "\n");
        while let Some(index) = buffer.find("\n\n") {
            let block: String = buffer.drain(..index + 2).collect();
            let mut event_name = "message".to_string();
            let mut data_lines = Vec::new();
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") { event_name = rest.trim().to_string(); }
                if let Some(rest) = line.strip_prefix("data:") { data_lines.push(rest.trim().to_string()); }
            }
            if !data_lines.is_empty() && (event_name == "match" || event_name == "message") {
                if let Ok(value) = serde_json::from_str::<Value>(&data_lines.join("\n")) {
                    emit_cached(app, "match-event", value);
                }
            }
        }
    }
    emit_cached(app, "network-status", serde_json::json!({ "events": false }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Endpoints;
    #[test]
    fn endpoint_configuration_uses_camel_case_keys() {
        let endpoints: Endpoints = serde_json::from_str(r#"{
            "xmppServerUrl":"https://example.test",
            "buildArchiveUrl":"https://example.test/builds/current.7z",
            "newsUrl":"https://example.test/news.json",
            "healthPollSeconds":30,
            "buildArchiveSha256":"0123456789abcdef",
            "discordInvite":"invite-code",
            "profileUrl":"https://example.test/profile/{email}",
            "apiUrl":"https://example.test/api"
        }"#).unwrap();
        assert_eq!(endpoints.xmpp_server_url, "https://example.test");
        assert_eq!(endpoints.build_archive_url, "https://example.test/builds/current.7z");
        assert_eq!(endpoints.health_poll_seconds, 30);
        assert_eq!(endpoints.build_archive_sha256, "0123456789abcdef");
    }

    #[test]
    fn default_build_url_is_7z() { assert!(super::d_build().ends_with(".7z")); }
}
