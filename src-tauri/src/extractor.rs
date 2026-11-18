use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Serialize, Clone)]
pub struct ExtractProgress {
    pub done: u32,
    pub total: u32,
    pub name: String,
}

pub struct ExtractError(pub String);
impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

fn is_reserved_windows_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// Convert an archive entry name into a strictly relative path. Reject both
/// Unix and Windows path syntax, including ADS and reserved device names.
fn safe_relative_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.len() > 32_000 || name.contains('\0') {
        return None;
    }
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with('~') || normalized.contains(':') {
        return None;
    }
    let mut result = PathBuf::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.ends_with('.') || part.ends_with(' ') {
            return None;
        }
        if part.chars().any(|c| c.is_control() || "<>\"|?*".contains(c)) || is_reserved_windows_name(part) {
            return None;
        }
        result.push(part);
    }
    if result.components().any(|c| !matches!(c, Component::Normal(_))) { return None; }
    Some(result)
}

fn safe_output_path(root: &Path, rel: &Path) -> Result<PathBuf, ExtractError> {
    let root = fs::canonicalize(root).map_err(|e| ExtractError(format!("Cannot resolve install folder: {e}")))?;
    let output = root.join(rel);
    let parent = output.parent().ok_or_else(|| ExtractError("Invalid archive path.".into()))?;
    fs::create_dir_all(parent).map_err(|e| ExtractError(format!("Cannot create extraction folder: {e}")))?;
    let canonical_parent = fs::canonicalize(parent).map_err(|e| ExtractError(format!("Cannot resolve extraction folder: {e}")))?;
    if !canonical_parent.starts_with(&root) {
        return Err(ExtractError("The archive contains a path that escapes the selected folder.".into()));
    }
    if let Ok(meta) = fs::symlink_metadata(&output) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ExtractError(format!("Refusing to overwrite a non-regular file: {}", rel.display())));
        }
    }
    Ok(output)
}

fn free_bytes(dir: &Path) -> u64 { fs2::available_space(dir).unwrap_or(0) }

/// Extract a real 7z archive using maintained `sevenz-rust2` and a custom
/// extraction callback. The callback validates every path before writing;
/// the old sevenz-rust 0.6 library had a documented path-traversal issue.
pub fn extract(
    archive_path: &Path,
    target: &Path,
    cancel: Arc<AtomicU8>,
    mut on_progress: impl FnMut(ExtractProgress),
) -> Result<(), ExtractError> {
    const SIGNATURE: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
    let mut f = fs::File::open(archive_path).map_err(|e| ExtractError(format!("Could not open downloaded archive: {e}")))?;
    let mut signature = [0u8; 6];
    f.read_exact(&mut signature).map_err(|e| ExtractError(format!("Could not read archive signature: {e}")))?;
    if signature != SIGNATURE {
        return Err(ExtractError("Unsupported archive format. Publish a genuine .7z archive and make sure buildArchiveUrl points to it. Renaming a .rar file is not enough.".into()));
    }

    fs::create_dir_all(target).map_err(|e| ExtractError(format!("Could not create install folder: {e}")))?;
    let archive = sevenz_rust2::Archive::open(archive_path)
        .map_err(|e| ExtractError(format!("Could not read 7z archive headers: {e}. The archive may be corrupt or password-protected.")))?;
    let file_entries: Vec<_> = archive.files.iter().filter(|e| !e.is_directory && !e.is_anti_item).collect();
    if file_entries.is_empty() { return Err(ExtractError("The archive does not contain any files.".into())); }
    if file_entries.len() > 150_000 {
        return Err(ExtractError("The archive contains too many files to install safely.".into()));
    }
    let total_bytes = file_entries.iter().try_fold(0u64, |sum, entry| sum.checked_add(entry.size))
        .ok_or_else(|| ExtractError("The archive reports an invalid total size.".into()))?;
    const MAX_UNPACKED_BYTES: u64 = 300 * 1024 * 1024 * 1024;
    if total_bytes > MAX_UNPACKED_BYTES {
        return Err(ExtractError("The archive expands beyond the launcher's 300 GB safety limit.".into()));
    }
    let required = total_bytes.saturating_add(256 * 1024 * 1024);
    let available = free_bytes(target);
    if available < required {
        return Err(ExtractError(format!("Not enough free disk space. About {:.1} GB is needed; {:.1} GB is available.", required as f64 / 1024f64.powi(3), available as f64 / 1024f64.powi(3))));
    }
    for entry in &archive.files {
        if entry.is_anti_item { return Err(ExtractError("The archive contains unsupported delete entries.".into())); }
        if safe_relative_path(&entry.name).is_none() {
            return Err(ExtractError(format!("Refusing unsafe archive path: {}", entry.name)));
        }
    }

    let mut done = 0u32;
    let total = file_entries.len().min(u32::MAX as usize) as u32;
    on_progress(ExtractProgress { done: 0, total, name: "Preparing files...".into() });
    let root = fs::canonicalize(target).map_err(|e| ExtractError(e.to_string()))?;
    let mut callback_error: Option<String> = None;

    let result = sevenz_rust2::decompress_file_with_extract_fn(archive_path, target, |entry, reader, _library_output_path| {
        if cancel.load(Ordering::SeqCst) == 2 {
            callback_error = Some("cancelled".into());
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Installation cancelled").into());
        }
        if entry.is_anti_item {
            callback_error = Some("The archive contains unsupported delete entries.".into());
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Unsupported archive entry").into());
        }
        let rel = match safe_relative_path(&entry.name) {
            Some(p) => p,
            None => {
                callback_error = Some(format!("Refusing unsafe archive path: {}", entry.name));
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Unsafe archive path").into());
            }
        };
        if entry.is_directory {
            let out = root.join(&rel);
            fs::create_dir_all(&out)?;
            let canonical = fs::canonicalize(&out)?;
            if !canonical.starts_with(&root) {
                callback_error = Some("The archive contains a path that escapes the selected folder.".into());
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Archive path escapes destination").into());
            }
        } else {
            let out = safe_output_path(&root, &rel).map_err(|e| {
                callback_error = Some(e.0.clone());
                io::Error::new(io::ErrorKind::PermissionDenied, e.0)
            })?;
            let mut file = OpenOptions::new().create(true).truncate(true).write(true).open(&out)?;
            io::copy(reader, &mut file)?;
            file.flush()?;
            done = done.saturating_add(1);
            on_progress(ExtractProgress { done, total, name: entry.name.clone() });
        }
        Ok(true)
    });

    if let Some(err) = callback_error {
        return Err(ExtractError(if err == "cancelled" { "cancelled".into() } else { err }));
    }
    result.map_err(|e| ExtractError(format!("Extraction failed: {e}. The archive may be damaged; download it again.")))?;
    if cancel.load(Ordering::SeqCst) == 2 { return Err(ExtractError("cancelled".into())); }
    on_progress(ExtractProgress { done: total, total, name: String::new() });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe_relative_path;
    #[test]
    fn rejects_parent_and_absolute_archive_paths() {
        assert!(safe_relative_path("../outside.txt").is_none());
        assert!(safe_relative_path("C:\\Windows\\system.ini").is_none());
        assert!(safe_relative_path("/etc/passwd").is_none());
        assert!(safe_relative_path("folder\\..\\outside.txt").is_none());
    }
    #[test]
    fn rejects_windows_device_names_and_alternate_streams() {
        assert!(safe_relative_path("folder/CON.txt").is_none());
        assert!(safe_relative_path("payload.dll:stream").is_none());
    }
    #[test]
    fn accepts_normal_game_paths() {
        assert!(safe_relative_path("FortniteGame/Content/Paks/pakchunk0-WindowsClient.pak").is_some());
    }
}
