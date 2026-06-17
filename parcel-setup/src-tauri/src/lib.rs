//! Parcel Setup — graphical configuration tool for Parcel installers.

mod commands;

use std::sync::Mutex;

pub struct AppState {
    pub project_path: Mutex<Option<String>>,
}

pub fn run() {
    // Parse command-line arguments or env var for project path.
    let args: Vec<String> = std::env::args().collect();
    let initial_path = args.get(1).cloned()
        .or_else(|| std::env::var("PARCEL_PROJECT_PATH").ok());

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            project_path: Mutex::new(initial_path.clone()),
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
            commands::get_initial_path,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
