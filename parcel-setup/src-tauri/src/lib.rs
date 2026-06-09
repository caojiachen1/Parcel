//! Parcel Setup — graphical configuration tool for Parcel installers.

mod commands;

use std::sync::Mutex;

pub struct AppState {
    pub project_path: Mutex<Option<String>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            project_path: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::select_folder,
            commands::select_file,
            commands::load_config,
            commands::save_config,
            commands::build_installer,
            commands::list_directory,
            commands::read_file,
            commands::write_file,
            commands::read_project_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
