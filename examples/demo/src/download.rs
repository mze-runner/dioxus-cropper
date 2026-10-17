//! Desktop download: the webview cannot save a file from a link, so the
//! result is written to the user's Downloads folder.

use std::path::PathBuf;

/// Writes `bytes` to `~/Downloads/{name}`, replacing any file of that name,
/// and returns the path written.
///
/// # Errors
///
/// Returns a message when no home directory is known or the write fails.
pub fn save(bytes: &[u8], name: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("no home directory")?;
    let path = PathBuf::from(home).join("Downloads").join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
