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
use std::path::{Path, PathBuf};

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

    log::info!("=== Parcel Uninstaller started ===");
    log::info!("App: {} v{}", manifest.app_name, manifest.app_version);
    log::info!("Install dir: {}", install_dir.display());
    log::info!("Silent mode: {}", silent);

    // ── Step 1: Remove shortcuts ────────────────────────────────────
    println!("Removing shortcuts…");

    if manifest.desktop_shortcut {
        if let Err(e) = remove_desktop_shortcut(&manifest.app_name) {
            log::warn!("Failed to remove desktop shortcut: {e}");
        }
    }

    if manifest.start_menu_shortcut {
        if let Err(e) = remove_start_menu_shortcut(&manifest.app_name) {
            log::warn!("Failed to remove Start Menu shortcut: {e}");
        }
    }

    // ── Step 2: Remove registry entries ─────────────────────────────
    println!("Cleaning registry…");

    // Uninstall key.
    if let Err(e) = remove_uninstall_registry(&manifest.app_identifier) {
        log::warn!("Failed to remove uninstall registry: {e}");
    }

    // Auto-start.
    if manifest.auto_start {
        if let Err(e) = remove_auto_start(&manifest.app_name) {
            log::warn!("Failed to remove auto-start entry: {e}");
        }
    }

    // File associations.
    for ext in &manifest.file_associations {
        if let Err(e) = remove_file_association(ext) {
            log::warn!("Failed to remove file association .{ext}: {e}");
        }
    }

    // ── Step 3: Remove installed files ──────────────────────────────
    println!("Removing files…");

    let mut removed_count = 0u32;
    let mut failed_count = 0u32;

    for relative in &manifest.installed_files {
        let full_path = install_dir.join(relative);
        if full_path.exists() {
            match std::fs::remove_file(&full_path) {
                Ok(_) => {
                    removed_count += 1;
                    log::info!("Removed: {}", full_path.display());
                }
                Err(e) => {
                    failed_count += 1;
                    log::warn!("Failed to remove {}: {e}", full_path.display());
                }
            }
        }
        // Also try to remove empty parent directories.
        cleanup_empty_parents(&full_path, install_dir);
    }

    // Remove the uninstall manifest itself.
    let manifest_path = install_dir.join("uninstall.json");
    if manifest_path.exists() {
        let _ = std::fs::remove_file(&manifest_path);
    }

    // Remove the install log.
    let log_path = install_dir.join("install.log");
    if log_path.exists() {
        let _ = std::fs::remove_file(&log_path);
    }

    log::info!(
        "File removal: {removed_count} removed, {failed_count} failed."
    );

    // ── Step 4: Remove installation directory ───────────────────────
    // Only if it's empty or contains only the uninstaller itself.
    println!("Cleaning up installation directory…");

    // Schedule self-deletion via a batch script.
    let self_delete_ok = schedule_self_delete(exe_path, install_dir);

    if !self_delete_ok {
        // Fallback: try direct removal of the directory.
        if install_dir.exists() {
            match remove_dir_if_safe(install_dir) {
                Ok(_) => log::info!("Removed install directory."),
                Err(e) => log::warn!("Could not remove install directory: {e}"),
            }
        }
    }

    // ── Done ────────────────────────────────────────────────────────
    if silent {
        log::info!("Uninstall complete (silent mode).");
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
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == stop_at || !d.starts_with(stop_at) {
            break;
        }
        if d.exists() {
            if let Ok(entries) = std::fs::read_dir(d) {
                if entries.count() == 0 {
                    let _ = std::fs::remove_dir(d);
                }
            }
        }
        dir = d.parent();
    }
}

/// Remove a directory only if it's in a recognised safe location.
fn remove_dir_if_safe(dir: &Path) -> Result<()> {
    let s = dir.to_string_lossy();
    let is_safe = s.contains("Programs")
        || s.contains("Program Files")
        || s.contains("AppData");

    if is_safe {
        std::fs::remove_dir_all(dir)?;
        Ok(())
    } else {
        anyhow::bail!(
            "Refusing to remove {} — not in a recognised safe directory.",
            dir.display()
        )
    }
}

/// Schedule self-deletion via a cmd.exe one-liner that runs after this
/// process exits.
///
/// The trick: `cmd /c ping 127.0.0.1 -n 2 > nul && del /f /q "<exe>" && rmdir "<dir>"`
///
/// Returns true if the self-delete was successfully scheduled.
fn schedule_self_delete(exe_path: &Path, install_dir: &Path) -> bool {
    let exe_str = exe_path.to_string_lossy();
    let dir_str = install_dir.to_string_lossy();
    let pid = std::process::id();

    // Use taskkill /F /PID to wait for our own process to exit, then delete.
    // This is more reliable than ping-based timing.
    let cmd = format!(
        r#"cmd /c "tasklist /fi "PID eq {pid}" > nul 2>&1 & ping 127.0.0.1 -n 3 > nul & del /f /q "{exe_str}" > nul 2>&1 & rmdir /s /q "{dir_str}" > nul 2>&1""#
    );

    log::info!("Scheduling self-delete: {cmd}");

    match std::process::Command::new("cmd")
        .args(["/c", "start", "/min", "", "cmd", "/c", &cmd])
        .spawn()
    {
        Ok(_) => {
            log::info!("Self-delete process spawned.");
            true
        }
        Err(e) => {
            log::warn!("Failed to spawn self-delete process: {e}");
            false
        }
    }
}

// ── Entry point ─────────────────────────────────────────────────────────

fn main() {
    // Initialise logging.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .format_timestamp_secs()
    .init();

    // Also write a file log next to the executable for diagnostics.
    if let Ok(exe_dir) = std::env::current_exe().and_then(|p| {
        p.parent()
            .map(|d| d.to_path_buf())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no parent"))
    }) {
        let log_path = exe_dir.join("uninstall.log");
        let header = format!(
            "=== Parcel Uninstaller {} ===\n",
            env!("CARGO_PKG_VERSION")
        );
        let _ = std::fs::write(&log_path, &header);
    }

    let silent = is_silent_mode();

    log::info!("=== Parcel Uninstaller {} ===", env!("CARGO_PKG_VERSION"));
    log::info!("Args: {:?}", std::env::args().collect::<Vec<_>>());

    // Load the uninstall manifest.
    let (manifest, exe_path) = match load_manifest() {
        Ok(m) => m,
        Err(e) => {
            log::error!("Failed to load manifest: {e}");
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
            "Are you sure you want to completely remove {} and all of its components?",
            manifest.app_name
        );
        let title = format!("Remove {}", manifest.app_name);
        if !confirm_dialog(&title, &msg) {
            log::info!("Uninstall cancelled by user.");
            println!("Uninstall cancelled.");
            return;
        }
    }

    // Run the uninstall sequence.
    if let Err(e) = run_uninstall(&manifest, &exe_path, silent) {
        log::error!("Uninstall failed: {e}");
        if !silent {
            info_dialog(
                "Uninstall Error",
                &format!("An error occurred during uninstallation:\n\n{e}"),
            );
        }
        std::process::exit(1);
    }

    log::info!("Uninstaller exiting normally.");
}
