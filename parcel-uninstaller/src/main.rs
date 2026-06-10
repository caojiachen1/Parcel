// Prevents additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Parcel Uninstaller — removes applications installed by Parcel.
//!
//! This is a lightweight application that reads an `uninstall.json`
//! manifest from its own directory and performs a full cleanup:
//!
//! - Desktop and Start Menu shortcuts
//! - Windows registry entries (uninstall info, auto-start, file associations)
//! - Installed application files
//! - The installation directory itself
//! - The uninstaller binary (self-deletion)
//!
//! ## Usage
//!
//! Interactive mode (shows a confirmation dialog):
//! ```text
//! uninstall.exe
//! ```
//!
//! Silent mode (no prompts, uses all defaults):
//! ```text
//! uninstall.exe /S
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ── Global file logger ──────────────────────────────────────────────────

/// Global log file handle so we can write detailed diagnostics to disk.
static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Write a line to both the console (via log crate) and the log file.
macro_rules! debug_log {
    (info, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        log::info!("{}", msg);
        write_log_file("INFO", &msg);
    };
    (warn, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        log::warn!("{}", msg);
        write_log_file("WARN", &msg);
    };
    (error, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        log::error!("{}", msg);
        write_log_file("ERROR", &msg);
    };
    (debug, $($arg:tt)*) => {
        let msg = format!($($arg)*);
        log::debug!("{}", msg);
        write_log_file("DEBUG", &msg);
    };
}

fn write_log_file(level: &str, msg: &str) {
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut f) = *guard {
            let _ = writeln!(f, "[{level}] {msg}");
            let _ = f.flush();
        }
    }
}

// ── Uninstall manifest ──────────────────────────────────────────────────

/// Manifest written to the install directory by the Parcel installer.
/// Contains everything the uninstaller needs to know to perform a clean removal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallManifest {
    /// Application display name.
    pub app_name: String,
    /// Application version at the time of installation.
    pub app_version: String,
    /// Unique identifier (reverse-domain), used as the registry key name.
    pub app_identifier: String,
    /// Publisher / company name.
    pub publisher: String,
    /// Absolute path to the installation directory.
    pub install_dir: PathBuf,
    /// Filename of the main application executable (e.g. "MyApp.exe").
    pub target_exe: String,
    /// List of all file paths (relative to install_dir) that were installed.
    pub installed_files: Vec<String>,
    /// File extensions that were registered during installation.
    pub file_associations: Vec<String>,
    /// Whether an auto-start registry entry was created.
    pub auto_start: bool,
    /// Whether a desktop shortcut was created.
    pub desktop_shortcut: bool,
    /// Whether a Start Menu shortcut was created.
    pub start_menu_shortcut: bool,
    /// Version of Parcel that performed the installation.
    pub parcel_version: String,
}

// ── Silent mode ─────────────────────────────────────────────────────────

fn is_silent_mode() -> bool {
    std::env::args().any(|a| a == "/S" || a == "/s" || a == "--silent")
}

// ── Windows MessageBox helpers ──────────────────────────────────────────

#[cfg(target_os = "windows")]
fn confirm_dialog(title: &str, message: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            hwnd: isize,
            lp_text: *const u16,
            lp_caption: *const u16,
            u_type: u32,
        ) -> i32;
    }

    const MB_YESNO: u32 = 0x0000_0004;
    const MB_ICONQUESTION: u32 = 0x0000_0020;
    const MB_TOPMOST: u32 = 0x0004_0000;
    const IDYES: i32 = 6;

    let wide_text: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_caption: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MessageBoxW(
            0,
            wide_text.as_ptr(),
            wide_caption.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST,
        )
    };

    result == IDYES
}

#[cfg(target_os = "windows")]
fn info_dialog(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            hwnd: isize,
            lp_text: *const u16,
            lp_caption: *const u16,
            u_type: u32,
        ) -> i32;
    }

    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONINFORMATION: u32 = 0x0000_0040;
    const MB_TOPMOST: u32 = 0x0004_0000;

    let wide_text: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_caption: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            0,
            wide_text.as_ptr(),
            wide_caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_TOPMOST,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn confirm_dialog(title: &str, message: &str) -> bool {
    println!("\n{title}\n{message}");
    print!("Continue? [y/N] ");
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

#[cfg(not(target_os = "windows"))]
fn info_dialog(title: &str, message: &str) {
    println!("\n{title}\n{message}");
}

// ── Manifest loading ────────────────────────────────────────────────────

/// Locate and load the uninstall manifest.
///
/// The manifest (`uninstall.json`) is expected to be in the same directory
/// as the uninstaller executable.
fn load_manifest() -> Result<(UninstallManifest, PathBuf)> {
    let exe_path = std::env::current_exe()
        .context("Failed to determine own executable path")?;

    let exe_dir = exe_path
        .parent()
        .context("Executable has no parent directory")?;

    let manifest_path = exe_dir.join("uninstall.json");

    if !manifest_path.exists() {
        anyhow::bail!(
            "uninstall.json not found at {}\n\
             The uninstall manifest is missing. The installation may be corrupt.",
            manifest_path.display()
        );
    }

    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let manifest: UninstallManifest = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    Ok((manifest, exe_path))
}

// ── Uninstall sequence ──────────────────────────────────────────────────

fn run_uninstall(manifest: &UninstallManifest, exe_path: &Path, silent: bool) -> Result<()> {
    let install_dir = &manifest.install_dir;

    debug_log!(info, "=== Parcel Uninstaller started ===");
    debug_log!(info, "App: {} v{}", manifest.app_name, manifest.app_version);
    debug_log!(info, "App identifier: {}", manifest.app_identifier);
    debug_log!(info, "Install dir (raw): {}", install_dir.display());
    debug_log!(info, "Install dir (canonical): {:?}", install_dir.canonicalize());
    debug_log!(info, "Target exe: {}", manifest.target_exe);
    debug_log!(info, "Installed files count: {}", manifest.installed_files.len());
    debug_log!(info, "File associations: {:?}", manifest.file_associations);
    debug_log!(info, "Auto start: {}", manifest.auto_start);
    debug_log!(info, "Desktop shortcut: {}", manifest.desktop_shortcut);
    debug_log!(info, "Start menu shortcut: {}", manifest.start_menu_shortcut);
    debug_log!(info, "Silent mode: {}", silent);
    debug_log!(info, "Exe path: {}", exe_path.display());

    // ── CRITICAL: Validate install_dir BEFORE any deletion ──
    validate_install_dir(install_dir)?;

    // Log every file that will be deleted
    debug_log!(info, "--- Files to be deleted ---");
    for (i, relative) in manifest.installed_files.iter().enumerate() {
        let full = install_dir.join(relative);
        debug_log!(info, "  [{i}] relative={relative:?} -> full={}", full.display());
    }
    debug_log!(info, "--- End of file list ---");

    // ── Step 1: Remove shortcuts ────────────────────────────────────
    println!("Removing shortcuts…");
    debug_log!(info, "Step 1: Removing shortcuts");

    if manifest.desktop_shortcut {
        match remove_desktop_shortcut(&manifest.app_name) {
            Ok(_) => debug_log!(info, "Desktop shortcut removed successfully."),
            Err(e) => debug_log!(warn, "Failed to remove desktop shortcut: {e}"),
        }
    } else {
        debug_log!(info, "No desktop shortcut to remove.");
    }

    if manifest.start_menu_shortcut {
        match remove_start_menu_shortcut(&manifest.app_name) {
            Ok(_) => debug_log!(info, "Start Menu shortcut removed successfully."),
            Err(e) => debug_log!(warn, "Failed to remove Start Menu shortcut: {e}"),
        }
    } else {
        debug_log!(info, "No Start Menu shortcut to remove.");
    }

    // ── Step 2: Remove registry entries ─────────────────────────────
    println!("Cleaning registry…");
    debug_log!(info, "Step 2: Cleaning registry");

    // Uninstall key.
    match remove_uninstall_registry(&manifest.app_identifier) {
        Ok(_) => debug_log!(info, "Uninstall registry key removed."),
        Err(e) => debug_log!(warn, "Failed to remove uninstall registry: {e}"),
    }

    // Auto-start.
    if manifest.auto_start {
        match remove_auto_start(&manifest.app_name) {
            Ok(_) => debug_log!(info, "Auto-start entry removed."),
            Err(e) => debug_log!(warn, "Failed to remove auto-start entry: {e}"),
        }
    } else {
        debug_log!(info, "No auto-start entry to remove.");
    }

    // File associations.
    for ext in &manifest.file_associations {
        match remove_file_association(ext) {
            Ok(_) => debug_log!(info, "File association .{ext} removed."),
            Err(e) => debug_log!(warn, "Failed to remove file association .{ext}: {e}"),
        }
    }

    // ── Step 3: Remove installed files ──────────────────────────────
    println!("Removing files…");
    debug_log!(info, "Step 3: Removing installed files");

    let mut removed_count = 0u32;
    let mut failed_count = 0u32;
    let mut skipped_count = 0u32;

    for relative in &manifest.installed_files {
        let full_path = install_dir.join(relative);
        debug_log!(debug, "Processing file: relative={relative:?}, full={}", full_path.display());

        // SAFETY: Verify the file is within install_dir
        if let Err(e) = validate_path_within(&full_path, install_dir) {
            debug_log!(error, "SKIPPING file outside install_dir: {e}");
            skipped_count += 1;
            continue;
        }

        // SAFETY: Reject files with suspicious relative paths (containing ..)
        if relative.contains("..") {
            debug_log!(error, "SKIPPING suspicious path with '..': {relative:?}");
            skipped_count += 1;
            continue;
        }

        if full_path.exists() {
            debug_log!(debug, "File exists, attempting removal: {}", full_path.display());
            match std::fs::remove_file(&full_path) {
                Ok(_) => {
                    removed_count += 1;
                    debug_log!(info, "Removed file: {}", full_path.display());
                }
                Err(e) => {
                    failed_count += 1;
                    debug_log!(warn, "Failed to remove {}: {e}", full_path.display());
                }
            }
        } else {
            debug_log!(debug, "File does not exist, skipping: {}", full_path.display());
        }
        // Also try to remove empty parent directories.
        cleanup_empty_parents(&full_path, install_dir);
    }

    // Remove the uninstall manifest itself.
    let manifest_path = install_dir.join("uninstall.json");
    debug_log!(info, "Removing manifest: {}", manifest_path.display());
    if manifest_path.exists() {
        match std::fs::remove_file(&manifest_path) {
            Ok(_) => debug_log!(info, "Removed uninstall.json"),
            Err(e) => debug_log!(warn, "Failed to remove uninstall.json: {e}"),
        }
    }

    // Remove the install log.
    let log_path = install_dir.join("install.log");
    debug_log!(info, "Removing install log: {}", log_path.display());
    if log_path.exists() {
        match std::fs::remove_file(&log_path) {
            Ok(_) => debug_log!(info, "Removed install.log"),
            Err(e) => debug_log!(warn, "Failed to remove install.log: {e}"),
        }
    }

    debug_log!(
        info,
        "File removal summary: {removed_count} removed, {failed_count} failed, {skipped_count} skipped (safety)."
    );

    // ── Step 4: Remove installation directory ───────────────────────
    // Only if it's empty or contains only the uninstaller itself.
    println!("Cleaning up installation directory…");
    debug_log!(info, "Step 4: Cleaning up installation directory");
    debug_log!(info, "Install dir exists: {}", install_dir.exists());

    // List remaining files in install_dir before self-delete
    if install_dir.exists() {
        match std::fs::read_dir(install_dir) {
            Ok(entries) => {
                let remaining: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                debug_log!(info, "Remaining entries in install_dir: {}", remaining.len());
                for entry in &remaining {
                    debug_log!(info, "  remaining: {}", entry.path().display());
                }
            }
            Err(e) => debug_log!(warn, "Cannot list install_dir contents: {e}"),
        }
    }

    // Schedule self-deletion via a batch script.
    debug_log!(info, "Scheduling self-delete...");
    let self_delete_ok = schedule_self_delete(exe_path, install_dir);

    if !self_delete_ok {
        debug_log!(warn, "Self-delete scheduling failed, trying fallback directory removal.");
        // Fallback: try direct removal of the directory.
        if install_dir.exists() {
            match remove_dir_if_safe(install_dir) {
                Ok(_) => debug_log!(info, "Removed install directory via fallback."),
                Err(e) => debug_log!(warn, "Could not remove install directory: {e}"),
            }
        }
    }

    // ── Done ────────────────────────────────────────────────────────
    if silent {
        debug_log!(info, "Uninstall complete (silent mode).");
    } else {
        let msg = format!(
            "{} has been successfully removed from your computer.",
            manifest.app_name
        );
        info_dialog("Uninstall Complete", &msg);
    }

    Ok(())
}

// ── Shortcut removal ────────────────────────────────────────────────────

fn remove_desktop_shortcut(name: &str) -> Result<()> {
    let desktop = std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Desktop"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Users\Default\Desktop"));

    let lnk = desktop.join(format!("{name}.lnk"));
    if lnk.exists() {
        std::fs::remove_file(&lnk)?;
        log::info!("Removed desktop shortcut: {}", lnk.display());
    }
    Ok(())
}

fn remove_start_menu_shortcut(name: &str) -> Result<()> {
    let start_menu = std::env::var("APPDATA")
        .map(|p| {
            PathBuf::from(p)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs")
        });

    let lnk = start_menu.join(format!("{name}.lnk"));
    if lnk.exists() {
        std::fs::remove_file(&lnk)?;
        log::info!("Removed Start Menu shortcut: {}", lnk.display());
    }
    Ok(())
}

// ── Registry removal ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn remove_uninstall_registry(identifier: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{identifier}"
    );
    let _ = hkcu.delete_subkey_all(&key_path);
    log::info!("Removed uninstall registry entry.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn remove_uninstall_registry(_identifier: &str) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_auto_start(app_name: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    if let Ok((key, _)) = hkcu.create_subkey(key_path) {
        let _ = key.delete_value(app_name);
    }
    log::info!("Removed auto-start entry for {app_name}.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn remove_auto_start(_app_name: &str) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_file_association(extension: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Remove the command subkey first.
    let prog_id = format!("Parcel.{extension}");
    let cmd_key = format!(r"Software\Classes\{prog_id}\shell\open\command");
    let _ = hkcu.delete_subkey_all(&cmd_key);

    // Remove the shell\open parent.
    let open_key = format!(r"Software\Classes\{prog_id}\shell\open");
    let _ = hkcu.delete_subkey_all(&open_key);

    // Remove the shell parent.
    let shell_key = format!(r"Software\Classes\{prog_id}\shell");
    let _ = hkcu.delete_subkey_all(&shell_key);

    // Remove the ProgID key.
    let prog_key = format!(r"Software\Classes\{prog_id}");
    let _ = hkcu.delete_subkey_all(&prog_key);

    // Remove the extension key (only if it points to our ProgID).
    let ext_key_path = format!(r"Software\Classes\.{extension}");
    if let Ok(ext_key) = hkcu.open_subkey(&ext_key_path) {
        if let Ok(val) = ext_key.get_value::<String, _>("") {
            if val == prog_id {
                let _ = hkcu.delete_subkey_all(&ext_key_path);
            }
        }
    }

    log::info!("Removed file association .{extension}.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn remove_file_association(_extension: &str) -> Result<()> {
    Ok(())
}

// ── File / directory helpers ────────────────────────────────────────────

/// Walk up from `file` to `stop_at`, removing each directory if it's empty.
fn cleanup_empty_parents(file: &Path, stop_at: &Path) {
    debug_log!(debug, "cleanup_empty_parents: from {} stopping at {}", file.display(), stop_at.display());
    let mut dir = file.parent();
    let mut depth = 0u32;
    while let Some(d) = dir {
        depth += 1;
        debug_log!(debug, "  checking parent [depth={depth}]: {}", d.display());

        if d == stop_at {
            debug_log!(debug, "  reached stop_at, breaking.");
            break;
        }
        if !d.starts_with(stop_at) {
            debug_log!(warn, "  SAFETY: parent {} is outside stop_at {}, breaking.", d.display(), stop_at.display());
            break;
        }
        if d.exists() {
            match std::fs::read_dir(d) {
                Ok(entries) => {
                    let count = entries.count();
                    if count == 0 {
                        debug_log!(info, "  removing empty dir: {}", d.display());
                        match std::fs::remove_dir(d) {
                            Ok(_) => debug_log!(info, "  removed empty dir: {}", d.display()),
                            Err(e) => debug_log!(warn, "  failed to remove empty dir {}: {e}", d.display()),
                        }
                    } else {
                        debug_log!(debug, "  dir not empty ({count} entries), stopping cleanup.");
                        break;
                    }
                }
                Err(e) => {
                    debug_log!(warn, "  cannot read dir {}: {e}", d.display());
                    break;
                }
            }
        } else {
            debug_log!(debug, "  dir does not exist, skipping.");
        }
        dir = d.parent();
    }
}

/// Validate that a path is safely contained within `install_dir`.
/// Returns an error if the path escapes the install directory via `..` or similar tricks.
fn validate_path_within(path: &Path, install_dir: &Path) -> Result<()> {
    let canonical_install = install_dir
        .canonicalize()
        .unwrap_or_else(|_| install_dir.to_path_buf());
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    if !canonical_path.starts_with(&canonical_install) {
        anyhow::bail!(
            "SAFETY: Path {} escapes install directory {}! Refusing to delete.",
            path.display(),
            install_dir.display()
        );
    }
    Ok(())
}

/// Check if a directory path is a drive root (e.g. `C:\`, `E:\`, `D:\`).
fn is_drive_root(path: &Path) -> bool {
    let s = path.to_string_lossy();
    // Match patterns like "C:\", "E:\", "C:/", "E:/"
    let trimmed = s.trim_end_matches(|c| c == '\\' || c == '/');
    // A drive root is exactly 2 chars like "C:" after trimming trailing separators
    trimmed.len() == 2 && trimmed.ends_with(':')
}

/// Check if a directory is a dangerous system location that must NEVER be deleted.
fn is_dangerous_directory(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    let dangerous = [
        "windows",
        "programdata",
        "users",
        "documents and settings",
        "$recycle.bin",
        "recovery",
        "boot",
    ];
    // Check direct children of drive roots
    if let Some(parent) = path.parent() {
        if is_drive_root(parent) {
            if let Some(name) = path.file_name() {
                let name_lower = name.to_string_lossy().to_lowercase();
                if dangerous.iter().any(|d| name_lower == *d) {
                    return true;
                }
            }
        }
    }
    false
}

/// Validate that the install_dir is safe to operate on.
/// This is the FIRST check before any deletion happens.
fn validate_install_dir(install_dir: &Path) -> Result<()> {
    debug_log!(info, "Validating install directory: {}", install_dir.display());

    // 1. Must be absolute
    if !install_dir.is_absolute() {
        anyhow::bail!(
            "SAFETY: install_dir is not absolute: {}. Refusing to proceed.",
            install_dir.display()
        );
    }

    // 2. Must NOT be a drive root
    if is_drive_root(install_dir) {
        anyhow::bail!(
            "SAFETY: install_dir is a drive root: {}. REFUSING TO DELETE A DRIVE!",
            install_dir.display()
        );
    }

    // 3. Must NOT be a dangerous system directory
    if is_dangerous_directory(install_dir) {
        anyhow::bail!(
            "SAFETY: install_dir is a dangerous system directory: {}. Refusing to proceed.",
            install_dir.display()
        );
    }

    // 4. Must have at least 3 components (e.g. C:\something\app, not just C:\something)
    let component_count = install_dir.components().count();
    if component_count < 3 {
        anyhow::bail!(
            "SAFETY: install_dir has too few path components ({}): {}. \
             This looks like a top-level directory. Refusing to proceed.",
            component_count,
            install_dir.display()
        );
    }

    debug_log!(info, "Install directory validation PASSED ({} components)", component_count);
    Ok(())
}

/// Remove a directory only if it passes strict safety checks.
fn remove_dir_if_safe(dir: &Path) -> Result<()> {
    debug_log!(info, "Checking if directory is safe to remove: {}", dir.display());

    // First: run the standard install_dir validation
    validate_install_dir(dir)?;

    // Second: canonicalize and verify the REAL path
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    debug_log!(info, "Canonical path: {}", canonical.display());

    if is_drive_root(&canonical) {
        anyhow::bail!(
            "SAFETY: Canonical path resolves to drive root: {}. REFUSING!",
            canonical.display()
        );
    }

    debug_log!(info, "Safety check PASSED for remove_dir_all: {}", dir.display());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Write a batch file that will self-delete after this process exits.
///
/// We write the batch script to a temp file to avoid ALL issues with
/// nested quoting in `cmd /c "..."` when paths contain spaces.
///
/// Returns true if the self-delete was successfully scheduled.
fn schedule_self_delete(exe_path: &Path, install_dir: &Path) -> bool {
    debug_log!(info, "schedule_self_delete called:");
    debug_log!(info, "  exe_path = {}", exe_path.display());
    debug_log!(info, "  install_dir = {}", install_dir.display());

    // SAFETY: validate install_dir before scheduling rmdir
    if let Err(e) = validate_install_dir(install_dir) {
        debug_log!(error, "SAFETY: Refusing to schedule rmdir for install_dir: {e}");
        return false;
    }

    if is_drive_root(install_dir) {
        debug_log!(error, "SAFETY: install_dir is a DRIVE ROOT! Refusing rmdir!");
        return false;
    }

    let pid = std::process::id();

    // Write a batch file to a temp location to avoid nested quoting hell.
    let batch_dir = std::env::temp_dir();
    let batch_path = batch_dir.join(format!("parcel_uninstall_{pid}.bat"));

    let exe_str = exe_path.to_string_lossy();
    let dir_str = install_dir.to_string_lossy();

    // Use `tasklist /fi "PID eq N"` to wait for our process, then delete.
    // All paths are quoted individually in the batch file.
    // Build batch content line by line to avoid any indentation issues.
    // Each line uses CRLF (\r\n) as required by cmd.exe batch files.
    let lines: Vec<String> = vec![
        "@echo off".into(),
        format!("echo Parcel Uninstaller self-delete script (PID={pid})"),
        "echo Waiting for uninstaller process to exit...".into(),
        ":wait_loop".into(),
        format!("tasklist /fi \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul 2>&1"),
        "if not errorlevel 1 (".into(),
        "timeout /t 1 /nobreak >nul".into(),
        "goto wait_loop".into(),
        ")".into(),
        "echo Process exited. Deleting files...".into(),
        format!("if exist \"{exe_str}\" ("),
        format!("del /f /q \"{exe_str}\""),
        format!("echo Deleted: {exe_str}"),
        ") else (".into(),
        format!("echo Exe not found: {exe_str}"),
        ")".into(),
        format!("if exist \"{dir_str}\" ("),
        format!("rmdir /s /q \"{dir_str}\""),
        format!("echo Deleted directory: {dir_str}"),
        ") else (".into(),
        format!("echo Directory not found: {dir_str}"),
        ")".into(),
        "echo Cleanup complete.".into(),
        "del /f /q \"%~f0\" >nul 2>&1".into(),
    ];
    let batch_content = lines.join("\r\n") + "\r\n";

    debug_log!(info, "Batch file path: {}", batch_path.display());
    debug_log!(info, "Batch file content:\n{batch_content}");

    match std::fs::write(&batch_path, &batch_content) {
        Ok(_) => debug_log!(info, "Batch file written successfully."),
        Err(e) => {
            debug_log!(error, "Failed to write batch file: {e}");
            return false;
        }
    }

    // Launch the batch file in a minimized window.
    // `start` is needed for non-blocking launch so the main process can exit
    // before the batch script deletes the exe and install directory.
    // `start` recognizes .bat files and invokes cmd.exe automatically,
    // so no need for an extra `cmd /c` wrapper.
    let batch_str = batch_path.to_string_lossy().to_string();
    debug_log!(info, "Spawning: cmd /c start /min \"\" \"{batch_str}\"");
    match std::process::Command::new("cmd")
        .args(["/c", "start", "/min", "", &batch_str])
        .spawn()
    {
        Ok(child) => {
            debug_log!(info, "Self-delete batch process spawned (pid={}).", child.id());
            true
        }
        Err(e) => {
            debug_log!(error, "Failed to spawn self-delete batch process: {e}");
            // Clean up the batch file if we couldn't launch it
            let _ = std::fs::remove_file(&batch_path);
            false
        }
    }
}

// ── Entry point ─────────────────────────────────────────────────────────

fn main() {
    // Initialise logging to console.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("debug"),
    )
    .format_timestamp_secs()
    .init();

    // Open a persistent log file next to the executable for diagnostics.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    if let Some(ref dir) = exe_dir {
        let log_path = dir.join("uninstall.log");
        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
        {
            Ok(f) => {
                if let Ok(mut guard) = LOG_FILE.lock() {
                    *guard = Some(f);
                }
                // Write header immediately
                write_log_file(
                    "INFO",
                    &format!("=== Parcel Uninstaller {} ===", env!("CARGO_PKG_VERSION")),
                );
            }
            Err(e) => {
                log::warn!("Could not open log file {}: {e}", log_path.display());
            }
        }
    }

    let silent = is_silent_mode();

    debug_log!(info, "=== Parcel Uninstaller {} ===", env!("CARGO_PKG_VERSION"));
    debug_log!(info, "Args: {:?}", std::env::args().collect::<Vec<_>>());
    debug_log!(info, "Current directory: {:?}", std::env::current_dir());
    debug_log!(info, "Silent mode: {silent}");

    // Load the uninstall manifest.
    let (manifest, exe_path) = match load_manifest() {
        Ok(m) => {
            debug_log!(info, "Manifest loaded successfully.");
            debug_log!(info, "  app_name: {}", m.app_name);
            debug_log!(info, "  app_version: {}", m.app_version);
            debug_log!(info, "  app_identifier: {}", m.app_identifier);
            debug_log!(info, "  publisher: {}", m.publisher);
            debug_log!(info, "  install_dir: {}", m.install_dir.display());
            debug_log!(info, "  target_exe: {}", m.target_exe);
            debug_log!(info, "  installed_files ({}) : {:?}", m.installed_files.len(), m.installed_files);
            debug_log!(info, "  file_associations: {:?}", m.file_associations);
            debug_log!(info, "  auto_start: {}", m.auto_start);
            debug_log!(info, "  desktop_shortcut: {}", m.desktop_shortcut);
            debug_log!(info, "  start_menu_shortcut: {}", m.start_menu_shortcut);
            debug_log!(info, "  parcel_version: {}", m.parcel_version);
            m
        }
        Err(e) => {
            debug_log!(error, "Failed to load manifest: {e}");
            if !silent {
                info_dialog(
                    "Uninstall Error",
                    &format!("Failed to load uninstall manifest:\n\n{e}"),
                );
            }
            std::process::exit(1);
        }
    };

    // Confirm with user (interactive mode).
    if !silent {
        let msg = format!(
            "Are you sure you want to completely remove {} and all of its components?\n\n\
             Installation directory: {}",
            manifest.app_name,
            manifest.install_dir.display()
        );
        let title = format!("Remove {}", manifest.app_name);
        if !confirm_dialog(&title, &msg) {
            debug_log!(info, "Uninstall cancelled by user.");
            println!("Uninstall cancelled.");
            return;
        }
    }

    // Run the uninstall sequence.
    if let Err(e) = run_uninstall(&manifest, &exe_path, silent) {
        debug_log!(error, "Uninstall failed: {e}");
        debug_log!(error, "Error chain: {e:?}");
        if !silent {
            info_dialog(
                "Uninstall Error",
                &format!("An error occurred during uninstallation:\n\n{e}"),
            );
        }
        std::process::exit(1);
    }

    debug_log!(info, "Uninstaller exiting normally.");

    // Flush and close log file before exit.
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut f) = *guard {
            let _ = writeln!(f, "=== Uninstaller exiting ===");
            let _ = f.flush();
        }
    }
}
