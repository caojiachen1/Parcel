//! Parcel Installer Runtime — Tauri application entry point.
//!
//! This module wires up the Tauri application, registers commands,
//! loads the embedded payload configuration, and manages global state.

mod commands;
mod config;
mod engine;

/// Embedded payload data generated at compile time by `build.rs`.
/// Contains the manifest JSON and all payload file bytes.
mod payload_data {
    include!(concat!(env!("OUT_DIR"), "/payload_data.rs"));
}

use std::sync::{Arc, Mutex};

/// Global application state shared across Tauri commands.
pub struct AppState {
    /// The payload configuration (loaded at startup).
    pub payload: config::PayloadConfig,
    /// Installation state machine.
    pub install_state: Arc<Mutex<engine::InstallState>>,
}

/// Initialise logging: console (env_logger) + file (into TEMP).
fn init_logging() {
    // Console logger with info level default.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    )
    .format_timestamp_secs()
    .init();

    // Also write a file log so users can diagnose issues after running.
    let log_path = std::env::temp_dir().join("parcel-installer.log");
    let header = format!(
        "=== Parcel Installer {} ===\n",
        env!("CARGO_PKG_VERSION")
    );
    let _ = std::fs::write(&log_path, &header);
    log::info!("File log: {}", log_path.display());
}

pub fn run() {
    init_logging();

    log::info!("=== Parcel Installer started ===");
    log::info!("Args: {:?}", std::env::args().collect::<Vec<_>>());

    // Load the payload configuration.
    // In preview mode this reads from PARCEL_CONFIG env var.
    // In production it reads from the embedded resource.
    let payload = config::load_payload();
    log::info!("Payload loaded: {} file(s) to install", payload.payload_files.len());
    for f in &payload.payload_files {
        log::info!("  Payload file: {}", f);
    }
    log::info!("Default install dir: {}", config::resolve_default_dir(&payload).display());
    log::info!("Target exe path: {}", payload.parcel.paths.target_exe.display());

    // Check for silent-install command-line arguments.
    if let Some(silent_args) = engine::silent::parse_silent_args() {
        log::info!("Silent installation mode detected.");
        let options = engine::silent::to_install_options(&silent_args, &payload);
        log::info!("Silent install dir: {}", &options.install_dir);
        log::info!("Silent desktop_shortcut: {}", options.desktop_shortcut);
        log::info!("Silent start_menu_shortcut: {}", options.start_menu_shortcut);
        log::info!("Silent auto_start: {}", options.auto_start);

        let install_state = Arc::new(Mutex::new(engine::InstallState::new()));
        engine::run_installation(payload, options, install_state);
        log::info!("Silent installation complete, exiting.");
        return;
    }

    let state = AppState {
        payload,
        install_state: Arc::new(Mutex::new(engine::InstallState::new())),
    };

    log::info!("Starting Tauri application...");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::get_install_options,
            commands::browse_directory,
            commands::start_install,
            commands::cancel_install,
            commands::finish_install,
            commands::get_install_progress,
        ])
        .setup(|_app| {
            log::info!("Parcel installer Tauri window opened.");
            // Append pid + log location to the file for easy reference.
            let log_path = std::env::temp_dir().join("parcel-installer.log");
            if let Ok(meta) = std::fs::metadata(&log_path) {
                log::info!("Log file size: {} bytes", meta.len());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run Parcel installer");

    log::info!("Parcel installer exited.");
}
