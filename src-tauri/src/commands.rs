//! Tauri commands exposed to the frontend installer UI.

use crate::config;
use crate::engine;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

// ── Response types ──────────────────────────────────────────────────────

/// Configuration sent to the frontend on startup.
#[derive(Debug, Clone, Serialize)]
pub struct FrontendConfig {
    pub app_name: String,
    pub app_version: String,
    pub publisher: String,
    pub publisher_url: String,
    pub eula_text: String,
    pub appearance: AppearancePayload,
    pub strings: parcel_core::config::StringsConfig,
    pub install_options: InstallOptionsInfo,
    pub parcel_version: String,
    pub is_preview: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppearancePayload {
    pub theme: String,
    pub colors: parcel_core::config::ColorsConfig,
    pub border_radius: u32,
    pub page_animation: String,
    pub font_family: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallOptionsInfo {
    pub default_dir: String,
    pub allow_custom_dir: bool,
    pub desktop_shortcut_default: bool,
    pub desktop_shortcut_optional: bool,
    pub start_menu_shortcut_default: bool,
    pub start_menu_shortcut_optional: bool,
    pub auto_start_enabled: bool,
    pub auto_start_default: bool,
    pub file_associations: Vec<parcel_core::config::FileAssociation>,
}

/// User-chosen installation options sent from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct InstallOptions {
    pub install_dir: String,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    pub auto_start: bool,
}

/// Progress information sent to the frontend during installation.
#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub percent: u8,
    pub current_file: String,
    pub status: String,
    pub installed_files: Vec<String>,
    pub is_complete: bool,
    pub error: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────

/// Return the full configuration for the installer UI.
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<FrontendConfig, String> {
    let payload = &state.payload;
    let is_preview = std::env::var("PARCEL_PREVIEW").is_ok();

    Ok(FrontendConfig {
        app_name: payload.parcel.app.name.clone(),
        app_version: payload.parcel.app.version.clone(),
        publisher: payload.parcel.app.publisher.clone(),
        publisher_url: payload.parcel.app.publisher_url.clone(),
        eula_text: payload.eula_text.clone(),
        appearance: AppearancePayload {
            theme: payload.parcel.appearance.theme.clone(),
            colors: payload.parcel.appearance.colors.clone(),
            border_radius: payload.parcel.appearance.border_radius,
            page_animation: payload.parcel.appearance.page_animation.clone(),
            font_family: payload.parcel.appearance.font_family.clone(),
        },
        strings: payload.parcel.appearance.strings.clone(),
        install_options: InstallOptionsInfo {
            default_dir: config::resolve_default_dir(payload)
                .to_string_lossy()
                .to_string(),
            allow_custom_dir: payload.parcel.install.allow_custom_dir,
            desktop_shortcut_default: payload.parcel.install.shortcuts.desktop_default,
            desktop_shortcut_optional: payload.parcel.install.shortcuts.desktop_optional,
            start_menu_shortcut_default: payload.parcel.install.shortcuts.start_menu_default,
            start_menu_shortcut_optional: payload.parcel.install.shortcuts.start_menu_optional,
            auto_start_enabled: payload.parcel.install.auto_start.enabled,
            auto_start_default: payload.parcel.install.auto_start.default_value,
            file_associations: payload.parcel.install.file_associations.clone(),
        },
        parcel_version: payload.parcel_version.clone(),
        is_preview,
    })
}

/// Return the current install options defaults (for options page).
#[tauri::command]
pub fn get_install_options(state: State<'_, AppState>) -> Result<InstallOptionsInfo, String> {
    let payload = &state.payload;
    Ok(InstallOptionsInfo {
        default_dir: config::resolve_default_dir(payload)
            .to_string_lossy()
            .to_string(),
        allow_custom_dir: payload.parcel.install.allow_custom_dir,
        desktop_shortcut_default: payload.parcel.install.shortcuts.desktop_default,
        desktop_shortcut_optional: payload.parcel.install.shortcuts.desktop_optional,
        start_menu_shortcut_default: payload.parcel.install.shortcuts.start_menu_default,
        start_menu_shortcut_optional: payload.parcel.install.shortcuts.start_menu_optional,
        auto_start_enabled: payload.parcel.install.auto_start.enabled,
        auto_start_default: payload.parcel.install.auto_start.default_value,
        file_associations: payload.parcel.install.file_associations.clone(),
    })
}

/// Open a native directory browser dialog.
#[tauri::command]
pub async fn browse_directory(
    app: tauri::AppHandle,
    default_path: String,
) -> Result<String, String> {
    log::info!("browse_directory called, default_path={}", default_path);
    use tauri_plugin_dialog::DialogExt;

    let default = std::path::PathBuf::from(&default_path);
    log::info!("Opening folder picker...");

    let result = app.dialog()
        .file()
        .set_directory(&default)
        .blocking_pick_folder();

    match result {
        Some(path) => {
            let chosen = path.to_string();
            log::info!("User selected folder: {}", chosen);
            Ok(chosen)
        }
        None => {
            log::info!("User cancelled folder selection.");
            Err("No directory selected".into())
        }
    }
}

/// Start the installation process with user-chosen options.
#[tauri::command]
pub fn start_install(
    state: State<'_, AppState>,
    options: InstallOptions,
) -> Result<(), String> {
    log::info!("start_install called");
    log::info!("  install_dir: {}", &options.install_dir);
    log::info!("  desktop_shortcut: {}", options.desktop_shortcut);
    log::info!("  start_menu_shortcut: {}", options.start_menu_shortcut);
    log::info!("  auto_start: {}", options.auto_start);

    let mut install_state = state
        .install_state
        .lock()
        .map_err(|e| format!("Lock error: {e}"))?;

    if install_state.is_running {
        log::warn!("Installation already in progress, refusing.");
        return Err("Installation already in progress".into());
    }

    install_state.is_running = true;
    install_state.is_cancelled = false;
    install_state.progress = 0;
    install_state.current_file.clear();
    install_state.status = "Preparing…".into();
    install_state.installed_files.clear();
    install_state.error = None;

    let payload = state.payload.clone();
    let install_state_arc = state.install_state.clone();
    log::info!("Payload has {} file(s) to install", payload.payload_files.len());

    // Spawn the installation on a background thread.
    std::thread::spawn(move || {
        log::info!("Installation thread started.");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine::run_installation(payload, options, install_state_arc);
        }));
        if let Err(panic) = result {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            log::error!("Installation thread PANICKED: {msg}");
        }
        log::info!("Installation thread finished.");
    });

    Ok(())
}

/// Cancel an in-progress installation.
#[tauri::command]
pub fn cancel_install(state: State<'_, AppState>) -> Result<(), String> {
    let mut install_state = state
        .install_state
        .lock()
        .map_err(|e| format!("Lock error: {e}"))?;
    install_state.is_cancelled = true;
    Ok(())
}

/// Called when the user clicks "Finish" on the completion page.
#[tauri::command]
pub fn finish_install(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    launch_app: bool,
) -> Result<(), String> {
    log::info!("finish_install called, launch_app={}", launch_app);

    let install_state = state
        .install_state
        .lock()
        .map_err(|e| format!("Lock error: {e}"))?;

    log::info!("  is_complete: {}", install_state.is_complete);
    log::info!("  progress: {}%", install_state.progress);
    log::info!("  installed_exe: {:?}", install_state.installed_exe);
    log::info!("  error: {:?}", install_state.error);
    log::info!("  installed_files: {:?}", install_state.installed_files);

    if launch_app {
        if let Some(ref exe_path) = install_state.installed_exe {
            log::info!("Launching installed application: {}", exe_path);
            match std::process::Command::new(exe_path).spawn() {
                Ok(child) => {
                    log::info!("Launched OK, pid={}", child.id());
                }
                Err(e) => {
                    log::error!("Failed to launch {}: {}", exe_path, e);
                }
            }
        } else {
            log::warn!("launch_app=true but installed_exe is None, nothing to launch.");
        }
    }

    // Close the installer window from the backend.
    log::info!("Closing installer window...");
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.close() {
            log::error!("Failed to close window: {}", e);
        } else {
            log::info!("Window closed OK.");
        }
    } else {
        log::warn!("Could not find webview window 'main' to close.");
    }

    Ok(())
}

/// Poll the current installation progress.
#[tauri::command]
pub fn get_install_progress(state: State<'_, AppState>) -> Result<InstallProgress, String> {
    let install_state = state
        .install_state
        .lock()
        .map_err(|e| format!("Lock error: {e}"))?;

    Ok(InstallProgress {
        percent: install_state.progress,
        current_file: install_state.current_file.clone(),
        status: install_state.status.clone(),
        installed_files: install_state.installed_files.clone(),
        is_complete: install_state.is_complete,
        error: install_state.error.clone(),
    })
}
