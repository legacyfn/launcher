use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use serde::Serialize;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Serialize, Clone)]
pub struct Progress {
    pub received: u64,
    pub total: u64,
    pub speed: f64,
    pub eta: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason { None, Pause, Cancel }

#[derive(Debug)]
pub struct DownloadError {
    pub message: String,
    pub fatal: bool,
    pub aborted: AbortReason,
}
impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for DownloadError {}

fn fatal(msg: impl Into<String>) -> DownloadError {
    DownloadError { message: msg.into(), fatal: true, aborted: AbortReason::None }
}
fn retryable(msg: impl Into<String>) -> DownloadError {
    DownloadError { message: msg.into(), fatal: false, aborted: AbortReason::None }
}

/// Cooperative download control. Pause keeps the partial file; cancel removes it
/// in the installer worker. State is 0=running, 1=pause requested, 2=cancel requested.
#[derive(Clone)]
pub struct AbortHandle(pub Arc<AtomicU8>);
impl AbortHandle {
    pub fn new() -> Self { Self(Arc::new(AtomicU8::new(0))) }
    pub fn pause(&self) { self.0.store(1, Ordering::SeqCst); }
    pub fn cancel(&self) { self.0.store(2, Ordering::SeqCst); }
    pub fn reason(&self) -> AbortReason {
        match self.0.load(Ordering::SeqCst) { 1 => AbortReason::Pause, 2 => AbortReason::Cancel, _ => AbortReason::None }
    }
}

const USER_AGENT: &str = "ProjectLegacyLauncher/4.3";
const MAX_RETRIES: u32 = 5;

pub async fn download(
    url: &str,
    part_path: &Path,
    abort: AbortHandle,
    mut on_progress: impl FnMut(Progress),
) -> Result<(u64, u64), DownloadError> {
    if !url.starts_with("https://") {
        return Err(fatal("Build downloads must use HTTPS."));
    }
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // Do not allow an HTTPS build URL to redirect the installer to HTTP.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() != "https" || attempt.previous().len() >= 6 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| fatal(e.to_string()))?;

    let mut tries = 0;
    loop {
        match abort.reason() {
            AbortReason::Pause => return Err(DownloadError { message: "Download paused.".into(), fatal: false, aborted: AbortReason::Pause }),
            AbortReason::Cancel => return Err(DownloadError { message: "Download cancelled.".into(), fatal: false, aborted: AbortReason::Cancel }),
            AbortReason::None => {}
        }
        match attempt(&client, url, part_path, &abort, &mut on_progress).await {
            Ok(v) => return Ok(v),
            Err(e) if e.aborted != AbortReason::None => return Err(e),
            Err(e) => {
                if e.fatal || tries >= MAX_RETRIES { return Err(e); }
                tries += 1;
                on_progress(Progress { received: std::fs::metadata(part_path).map(|m| m.len()).unwrap_or(0), total: 0, speed: 0.0, eta: None });
                tokio::time::sleep(Duration::from_millis(2500 * tries as u64)).await;
            }
        }
    }
}

async fn attempt(
    client: &reqwest::Client,
    url: &str,
    part_path: &Path,
    abort: &AbortHandle,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u64, u64), DownloadError> {
    let offset = std::fs::metadata(part_path).map(|m| m.len()).unwrap_or(0);
    let mut request = client.get(url).timeout(Duration::from_secs(30));
    if offset > 0 { request = request.header(RANGE, format!("bytes={offset}-")); }
    let res = request.send().await.map_err(|e| retryable(format!("Download request failed: {e}")))?;
    let status = res.status().as_u16();

    if status == 416 {
        if let Some(total) = res.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next()).and_then(|s| s.parse::<u64>().ok()) {
            if total == offset { return Ok((offset, offset)); }
        }
        let _ = std::fs::remove_file(part_path);
        return Err(retryable("The server rejected the resume request; the partial file was discarded."));
    }
    if status != 200 && status != 206 {
        let msg = format!("Build server returned HTTP {status}.");
        return if status >= 500 || status == 408 || status == 429 { Err(retryable(msg)) } else { Err(fatal(msg)) };
    }

    let (start, mut total) = if status == 206 {
        let cr = res.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let parsed_start = cr.strip_prefix("bytes ").and_then(|s| s.split('-').next()).and_then(|s| s.parse::<u64>().ok());
        let start = parsed_start.ok_or_else(|| fatal("The server sent an invalid Content-Range header."))?;
        if start != offset { return Err(fatal("The server resumed at the wrong byte offset. Delete the partial download and try again.")); }
        let total = cr.rsplit('/').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        (start, total)
    } else {
        (0, res.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0))
    };
    if total == 0 {
        total = res.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) + start;
    }

    if let Some(parent) = part_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| fatal(format!("Cannot create download folder: {e}")))?;
    }
    let mut file = if start > 0 {
        let mut f = std::fs::OpenOptions::new().write(true).open(part_path)
            .map_err(|e| fatal(format!("Cannot resume partial download: {e}")))?;
        f.seek(SeekFrom::Start(start)).map_err(|e| fatal(e.to_string()))?;
        f
    } else {
        std::fs::File::create(part_path).map_err(|e| fatal(format!("Cannot create download file: {e}")))?
    };

    let mut stream = res.bytes_stream();
    let mut written = 0u64;
    let mut last_tick = Instant::now();
    let mut last_bytes = 0u64;
    let mut speed = 0.0;

    while let Some(chunk) = stream.next().await {
        match abort.reason() {
            AbortReason::Pause => return Err(DownloadError { message: "Download paused.".into(), fatal: false, aborted: AbortReason::Pause }),
            AbortReason::Cancel => return Err(DownloadError { message: "Download cancelled.".into(), fatal: false, aborted: AbortReason::Cancel }),
            AbortReason::None => {}
        }
        let chunk = chunk.map_err(|e| retryable(format!("Download stream interrupted: {e}")))?;
        file.write_all(&chunk).map_err(|e| fatal(format!("Disk write failed: {e}")))?;
        written += chunk.len() as u64;

        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64();
        if dt >= 0.25 {
            let instantaneous = (written - last_bytes) as f64 / dt.max(0.001);
            speed = if speed > 0.0 { speed * 0.7 + instantaneous * 0.3 } else { instantaneous };
            last_bytes = written;
            last_tick = now;
            let received = start + written;
            on_progress(Progress {
                received, total, speed,
                eta: if total > received && speed > 1.0 { Some((total - received) as f64 / speed) } else { None },
            });
        }
    }
    file.flush().map_err(|e| fatal(format!("Could not flush download file: {e}")))?;

    let size = start + written;
    on_progress(Progress { received: size, total: if total > 0 { total } else { size }, speed, eta: None });
    if total > 0 && size != total {
        return Err(retryable(format!("Incomplete download ({size} of {total} bytes).")));
    }
    Ok((size, if total > 0 { total } else { size }))
}
