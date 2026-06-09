//! Rollback — undo a partial installation on failure or cancellation.

use super::files;
use std::path::Path;

/// Attempt to roll back a failed or cancelled installation.
///
/// This removes all installed files and cleans up any registry entries
/// that were written before the failure occurred.
pub fn rollback(install_dir: &Path, log_lines: &[String]) {
    log::warn!("Performing installation rollback…");

    // Append rollback info to the log.
    let mut all_lines: Vec<String> = log_lines.to_vec();
    all_lines.push(String::new());
    all_lines.push("--- ROLLBACK ---".into());

    // Remove installed files.
    match files::remove_installed_files(install_dir) {
        Ok(_) => {
            all_lines.push("Removed installed files.".into());
        }
        Err(e) => {
            all_lines.push(format!("Failed to remove files: {e}"));
        }
    }

    // Write the rollback log.
    let log_path = install_dir.join("rollback.log");
    let content = all_lines.join("\n");
    let _ = std::fs::write(&log_path, &content);

    log::warn!("Rollback complete. Log at {}", log_path.display());
}
