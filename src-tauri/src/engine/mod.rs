//! Installation engine — core logic for performing the installation.

pub mod files;
pub mod registry;
pub mod rollback;
pub mod shortcuts;
pub mod silent;
pub mod vcredist;

use crate::commands::InstallOptions;
use crate::config::{self, PayloadConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

/// Mutable state tracking the progress of an installation.
pub struct InstallState {
    pub is_running: bool,
    pub is_cancelled: bool,
    pub is_complete: bool,
    pub progress: u8,
    pub current_file: String,
    pub status: String,
    pub installed_files: Vec<String>,
    pub installed_exe: Option<String>,
    pub error: Option<String>,
}

impl InstallState {
    pub fn new() -> Self {
        Self {
            is_running: false,
            is_cancelled: false,
            is_complete: false,
            progress: 0,
            current_file: String::new(),
            status: String::new(),
            installed_files: Vec::new(),
            installed_exe: None,
            error: None,
        }
    }
}

/// Run the full installation sequence.
pub fn run_installation(
    payload: PayloadConfig,
    options: InstallOptions,
    install_state: Arc<Mutex<InstallState>>,
) {
    CANCEL_FLAG.store(false, Ordering::SeqCst);

    let install_dir = std::path::PathBuf::from(&options.install_dir);
    let mut log_lines: Vec<String> = Vec::new();

    log::info!("Starting installation to {}", install_dir.display());
    log_lines.push(format!("Install directory: {}", install_dir.display()));

    // ── Whitelist: register install_dir BEFORE any file operations ──
    // This ensures that even if the install fails partway, the rollback
    // is permitted (the path is on the whitelist).
    match parcel_core::safety::whitelist_install_dir(&install_dir) {
        Ok(_) => {
            log::info!("Install directory added to whitelist.");
            log_lines.push("Install directory registered in whitelist.".into());
        }
        Err(e) => {
            log::warn!("Failed to whitelist install directory: {e}");
            log_lines.push(format!("Warning: could not register whitelist: {e}"));
            // Continue anyway — the install should not be blocked by a
            // whitelist I/O error (e.g. read-only APPDATA).
        }
    }

    // Helper to update progress state.
    let update = |progress: u8, status: &str, file: &str| {
        if let Ok(mut state) = install_state.lock() {
            state.progress = progress;
            state.status = status.to_string();
            state.current_file = file.to_string();
        }
    };

    // Helper to check cancellation.
    let is_cancelled = || -> bool {
        if CANCEL_FLAG.load(Ordering::SeqCst) {
            if let Ok(mut state) = install_state.lock() {
                state.is_running = false;
                state.status = "Cancelled".into();
                state.error = Some("Installation was cancelled by the user.".into());
            }
            return true;
        }
        false
    };

    // ── Step 1: Create installation directory ───────────────────────
    update(2, "Creating installation directory…", "");
    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        log::error!("Failed to create install directory: {e}");
        if let Ok(mut state) = install_state.lock() {
            state.is_running = false;
            state.error = Some(format!("Failed to create install directory: {e}"));
        }
        return;
    }
    log_lines.push(format!("Created directory: {}", install_dir.display()));

    if is_cancelled() { return; }

    // ── Step 2: Check for VC++ redistributable ─────────────────────
    update(5, "Checking system dependencies…", "");
    if !vcredist::is_installed() {
        log::info!("VC++ redistributable not found, attempting installation…");
        log_lines.push("Installing VC++ redistributable…".into());
        if let Err(e) = vcredist::install() {
            log::warn!("VC++ redistributable installation failed: {e}");
            log_lines.push(format!("Warning: VC++ redist failed: {e}"));
        }
    } else {
        log_lines.push("VC++ redistributable already installed.".into());
    }

    if is_cancelled() { return; }

    // ── Step 3: Extract embedded payload files ─────────────────────
    update(10, "Extracting files…", "");
    log_lines.push("Extracting files…".into());
    let total = payload.payload_files.len().max(1);
    log::info!("Starting extraction of {} file(s) to {}", payload.payload_files.len(), install_dir.display());
    for pf in &payload.payload_files {
        log::info!("  Will install: {}", pf);
    }

    // Use the embedded payload extraction from config.rs.
    let extracted = config::extract_payload_files(&install_dir);
    log::info!("extract_payload_files returned {} result(s)", extracted.len());

    for (i, (relative, result)) in extracted.iter().enumerate() {
        if is_cancelled() { return; }

        let progress = 10 + ((i as f64 / total as f64) * 70.0) as u8;
        update(progress, &format!("Installing {}…", relative), relative);

        match result {
            Ok(dest) => {
                let exists = dest.exists();
                let size = if exists {
                    std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                };
                log::info!("Installed ({progress}%): {} (exists={}, size={} bytes)", relative, exists, size);
                log_lines.push(format!("  -> {} ({} bytes)", dest.display(), size));
                if let Ok(mut state) = install_state.lock() {
                    state.installed_files.push(relative.clone());
                }
            }
            Err(e) => {
                log::error!("Failed to install {relative}: {e}");
                log_lines.push(format!("ERROR: Failed to install {relative}: {e}"));
                update(0, &format!("Error: {e}"), "");
                if let Ok(mut state) = install_state.lock() {
                    state.is_running = false;
                    state.error = Some(format!("Failed to install {relative}: {e}"));
                }
                rollback::rollback(&install_dir, &log_lines, &payload, &registry::RegistryChangeLog::new());
                return;
            }
        }
    }

    log::info!("File extraction complete.");

    if is_cancelled() { return; }

    // ── Step 3.5: Write uninstall manifest ─────────────────────────
    update(82, "Writing uninstall manifest…", "");
    let installed_file_list: Vec<String> = {
        let state = install_state.lock().unwrap();
        state.installed_files.clone()
    };

    let exe_filename_for_manifest = std::path::Path::new(&payload.parcel.paths.target_exe)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}.exe", payload.parcel.app.name.replace(' ', "_")));

    let uninstall_manifest = serde_json::json!({
        "app_name": payload.parcel.app.name,
        "app_version": payload.parcel.app.version,
        "app_identifier": payload.parcel.app.identifier,
        "publisher": payload.parcel.app.publisher,
        "install_dir": install_dir.to_string_lossy(),
        "target_exe": exe_filename_for_manifest,
        "installed_files": installed_file_list,
        "file_associations": payload.parcel.install.file_associations
            .iter()
            .map(|a| &a.extension)
            .collect::<Vec<_>>(),
        "auto_start": options.auto_start,
        "desktop_shortcut": options.desktop_shortcut,
        "start_menu_shortcut": options.start_menu_shortcut,
        "parcel_version": payload.parcel_version,
    });

    let manifest_path = install_dir.join("uninstall.json");
    match serde_json::to_string_pretty(&uninstall_manifest) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&manifest_path, &json) {
                log::warn!("Failed to write uninstall.json: {e}");
            } else {
                log::info!("Wrote uninstall manifest: {}", manifest_path.display());
            }
        }
        Err(e) => log::warn!("Failed to serialise uninstall manifest: {e}"),
    }

    if is_cancelled() { return; }

    // ── Step 4: Create shortcuts ───────────────────────────────────
    update(85, "Creating shortcuts…", "");
    // Use the actual filename from the target exe path, not derived from app name.
    let exe_filename = std::path::Path::new(&payload.parcel.paths.target_exe)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}.exe", payload.parcel.app.name.replace(' ', "_")));
    let exe_path = install_dir.join(&exe_filename);
    log::info!("Exe path for shortcuts/launch: {}", exe_path.display());
    log::info!("Exe exists? {}", exe_path.exists());

    if options.desktop_shortcut {
        log_lines.push("Creating desktop shortcut…".into());
        match shortcuts::create_desktop_shortcut(&payload.parcel.app.name, &exe_path) {
            Ok(_) => log::info!("Desktop shortcut created OK."),
            Err(e) => log::warn!("Failed to create desktop shortcut: {e}"),
        }
    } else {
        log::info!("Desktop shortcut not requested.");
    }

    if options.start_menu_shortcut {
        log_lines.push("Creating Start Menu shortcut…".into());
        match shortcuts::create_start_menu_shortcut(&payload.parcel.app.name, &exe_path) {
            Ok(_) => log::info!("Start Menu shortcut created OK."),
            Err(e) => log::warn!("Failed to create Start Menu shortcut: {e}"),
        }
    } else {
        log::info!("Start Menu shortcut not requested.");
    }

    if is_cancelled() { return; }

    // ── Step 5: Write registry entries ─────────────────────────────
    update(92, "Configuring system…", "");
    log_lines.push("Writing registry entries…".into());
    let mut reg_changes = registry::RegistryChangeLog::new();

    if let Err(e) = registry::write_uninstall_info(&payload.parcel, &install_dir, &mut reg_changes) {
        log::warn!("Failed to write uninstall info: {e}");
        log_lines.push(format!("Warning: failed to write uninstall registry: {e}"));
    }

    if options.auto_start {
        if let Err(e) = registry::write_auto_start(&payload.parcel.app.name, &exe_path, &mut reg_changes) {
            log::warn!("Failed to write auto-start entry: {e}");
            log_lines.push(format!("Warning: failed to write auto-start registry: {e}"));
        }
    }

    for assoc in &payload.parcel.install.file_associations {
        if let Err(e) = registry::write_file_association(assoc, &exe_path, &mut reg_changes) {
            log::warn!("Failed to register file association .{}: {e}", assoc.extension);
            log_lines.push(format!("Warning: failed to write file association .{}: {e}", assoc.extension));
        }
    }

    // Log a detailed summary of all registry modifications.
    let reg_summary = reg_changes.summary();
    log::info!("Registry change summary:\n{reg_summary}");
    log_lines.push(String::new());
    log_lines.push("=== Registry Changes ===".into());
    for line in reg_summary.lines() {
        log_lines.push(format!("  {line}"));
    }

    // Persist the change log alongside the install for the uninstaller.
    let reg_log_path = install_dir.join("registry_changes.json");
    match serde_json::to_string_pretty(&reg_changes) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&reg_log_path, &json) {
                log::warn!("Failed to write registry_changes.json: {e}");
            } else {
                log::info!("Wrote registry change log: {}", reg_log_path.display());
            }
        }
        Err(e) => log::warn!("Failed to serialise registry change log: {e}"),
    }

    // ── Step 6: Write installation log ─────────────────────────────
    update(98, "Finalizing…", "");
    let log_path = install_dir.join("install.log");
    let log_content = log_lines.join("\n");
    let _ = std::fs::write(&log_path, &log_content);

    // ── Done ────────────────────────────────────────────────────────
    log::info!("Installation completed successfully.");
    CANCEL_FLAG.store(false, Ordering::SeqCst);

    if let Ok(mut state) = install_state.lock() {
        state.progress = 100;
        state.status = "Installation complete!".into();
        state.current_file.clear();
        state.is_complete = true;
        state.is_running = false;
        state.installed_exe = Some(exe_path.to_string_lossy().to_string());
    }
}
