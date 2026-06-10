//! Rollback — undo a partial installation on failure or cancellation.
//!
//! When a `RegistryChangeLog` is available, registry changes are undone
//! precisely: original values are restored, and only newly-created keys
//! are deleted.  This avoids the blunt "delete entire subkey" approach
//! that could remove values belonging to other software.

use super::{files, registry, shortcuts};
use crate::config::PayloadConfig;
use std::path::Path;

/// Attempt to roll back a failed or cancelled installation.
///
/// This removes all installed files, registry entries, and shortcuts
/// that were written before the failure occurred.
///
/// If `reg_changes` contains recorded modifications, registry rollback
/// is performed precisely (restoring old values instead of deleting keys).
pub fn rollback(
    install_dir: &Path,
    log_lines: &[String],
    payload: &PayloadConfig,
    reg_changes: &registry::RegistryChangeLog,
) {
    log::warn!("Performing installation rollback...");

    // Append rollback info to the log.
    let mut all_lines: Vec<String> = log_lines.to_vec();
    all_lines.push(String::new());
    all_lines.push("--- ROLLBACK ---".into());

    // Remove shortcuts.
    all_lines.push("Removing shortcuts...".into());
    if let Err(e) = shortcuts::remove_desktop_shortcut(&payload.parcel.app.name) {
        all_lines.push(format!("Failed to remove desktop shortcut: {e}"));
    }
    if let Err(e) = shortcuts::remove_start_menu_shortcut(&payload.parcel.app.name) {
        all_lines.push(format!("Failed to remove Start Menu shortcut: {e}"));
    }

    // ── Registry rollback ──────────────────────────────────────────
    // If we have a precise change log, use it for surgical rollback.
    // Otherwise fall back to the brute-force removal.
    if !reg_changes.value_changes.is_empty() || !reg_changes.keys_created.is_empty() {
        all_lines.push("Rolling back registry changes (precise)...".into());
        all_lines.push(reg_changes.summary());
        if let Err(e) = reg_changes.rollback() {
            all_lines.push(format!("Failed to rollback registry: {e}"));
            log::error!("Registry rollback failed: {e}");
        } else {
            all_lines.push("Registry rollback successful.".into());
        }
    } else {
        // No change log available — fall back to brute-force removal.
        all_lines.push("Cleaning registry (no change log, brute-force)...".into());
        if let Err(e) = registry::remove_uninstall_info(&payload.parcel.app.identifier) {
            all_lines.push(format!("Failed to remove uninstall registry: {e}"));
        }
        if let Err(e) = registry::remove_auto_start(&payload.parcel.app.name) {
            all_lines.push(format!("Failed to remove auto-start entry: {e}"));
        }
        for assoc in &payload.parcel.install.file_associations {
            if let Err(e) = registry::remove_file_association(&assoc.extension) {
                all_lines.push(format!("Failed to remove file association .{}: {e}", assoc.extension));
            }
        }
    }

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

/// Legacy rollback function for backward compatibility (file-only rollback).
#[allow(dead_code)]
pub fn rollback_files_only(install_dir: &Path, log_lines: &[String]) {
    log::warn!("Performing file-only installation rollback...");

    let mut all_lines: Vec<String> = log_lines.to_vec();
    all_lines.push(String::new());
    all_lines.push("--- ROLLBACK (files only) ---".into());

    match files::remove_installed_files(install_dir) {
        Ok(_) => {
            all_lines.push("Removed installed files.".into());
        }
        Err(e) => {
            all_lines.push(format!("Failed to remove files: {e}"));
        }
    }

    let log_path = install_dir.join("rollback.log");
    let content = all_lines.join("\n");
    let _ = std::fs::write(&log_path, &content);

    log::warn!("Rollback complete. Log at {}", log_path.display());
}
