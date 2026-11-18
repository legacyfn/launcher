use std::path::{Path, PathBuf};

pub const GAME_EXE_REL: &str = "FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe";

fn game_exe_rel_path() -> PathBuf {
    PathBuf::from("FortniteGame")
        .join("Binaries")
        .join("Win64")
        .join("FortniteClient-Win64-Shipping.exe")
}

/// A folder with an incomplete marker is never reported as a usable install.
pub fn has_incomplete_marker(dir: &Path) -> bool {
    dir.join(".launcher").join("incomplete").exists()
}

/// Looks in `dir` itself and up to `max_depth` levels of subfolders for the game executable.
pub fn find_build_root(dir: &str, max_depth: u32) -> Option<PathBuf> {
    if dir.is_empty() {
        return None;
    }
    let base = Path::new(dir);
    if !base.is_dir() || has_incomplete_marker(base) {
        return None;
    }
    // A dedicated game folder should be small. Bound the scan so choosing a
    // drive root or a very large directory cannot freeze the UI or walk the
    // entire disk. The build is normally at the selected root or one folder
    // below it, so this is ample for supported layouts.
    let mut budget = 4096usize;
    search(base, max_depth, &mut budget)
}

fn search(dir: &Path, depth_left: u32, budget: &mut usize) -> Option<PathBuf> {
    if dir.join(game_exe_rel_path()).is_file() && !has_incomplete_marker(dir) {
        return Some(dir.to_path_buf());
    }
    if depth_left == 0 || *budget == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries {
        if *budget == 0 { break; }
        *budget -= 1;
        let entry = match entry { Ok(value) => value, Err(_) => continue };
        let file_type = match entry.file_type() { Ok(value) => value, Err(_) => continue };
        // Do not follow symlinks/reparse points: they can form loops or send a
        // scan outside the user-selected installation directory.
        if !file_type.is_dir() || file_type.is_symlink() { continue; }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("System Volume Information") {
            continue;
        }
        if let Some(found) = search(&path, depth_left - 1, budget) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_path_is_stable() {
        assert!(GAME_EXE_REL.ends_with("FortniteClient-Win64-Shipping.exe"));
    }
}
