//! The repo the guards look at: its root and its files (tracked, plus new ones git doesn't ignore).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root (xtask's parent folder).
pub fn root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().map_or(here.clone(), Path::to_path_buf)
}

/// Every file git tracks or would track (not ignored), as `/`-separated paths relative to `root`.
pub fn files(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("can't run git ls-files (is git on PATH?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let mut list: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).replace('\\', "/"))
        // a tracked file deleted in the working tree is not there to check
        .filter(|p| root.join(p).is_file())
        .collect();
    list.sort();
    list.dedup();
    Ok(list)
}

/// Whether git ignores `rel` (a path that need not exist).
pub fn ignored(root: &Path, rel: &str) -> bool {
    Command::new("git")
        .args(["check-ignore", "-q", "--no-index", rel])
        .current_dir(root)
        .status()
        .is_ok_and(|s| s.success())
}

/// A file's text (invalid UTF-8 replaced), or None for a binary file (a NUL byte in its first
/// 8 KiB).
pub fn text(path: &Path) -> Result<Option<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}
