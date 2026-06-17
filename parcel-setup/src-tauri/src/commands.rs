//! Tauri commands for the Parcel Setup GUI.

use crate::AppState;
use parcel_core::config::ParcelConfig;
use serde::{Deserialize, Serialize};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{Emitter, State};

// ── Response types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Serialize)]
pub struct BuildProgress {
    pub line: String,
    pub stream: String, // "stdout" or "stderr"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub identifier: Option<String>,
    pub publisher: Option<String>,
    pub exe_name: Option<String>,
}

// ── Dialog commands ─────────────────────────────────────────────────────

#[tauri::command]
pub fn get_initial_path(state: State<'_, AppState>) -> Option<String> {
    state.project_path.lock().unwrap().clone()
}

#[tauri::command]
pub fn select_folder(
    app: tauri::AppHandle,
    title: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    let result = dialog.blocking_pick_folder();
    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
pub fn select_file(
    app: tauri::AppHandle,
    title: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if let Some(filters) = filters {
        for (name, exts) in filters {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(name, &ext_refs);
        }
    }
    let result = dialog.blocking_pick_file();
    Ok(result.map(|p| p.to_string()))
}

// ── Config commands ─────────────────────────────────────────────────────

#[tauri::command]
pub fn read_project_info(path: String) -> Result<ProjectInfo, String> {
    let project_dir = PathBuf::from(&path);
    let mut info = ProjectInfo {
        name: None,
        version: None,
        identifier: None,
        publisher: None,
        exe_name: None,
    };

    // Try reading tauri.conf.json
    let tauri_conf_path = project_dir.join("src-tauri").join("tauri.conf.json");
    if tauri_conf_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&tauri_conf_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(name) = json.get("productName").and_then(|v| v.as_str()) {
                    info.name = Some(name.to_string());
                }
                if let Some(ver) = json.get("version").and_then(|v| v.as_str()) {
                    info.version = Some(ver.to_string());
                }
                if let Some(id) = json.get("identifier").and_then(|v| v.as_str()) {
                    info.identifier = Some(id.to_string());
                }
            }
        }
    }

    // Try reading src-tauri/Cargo.toml for package info
    let cargo_toml_path = project_dir.join("src-tauri").join("Cargo.toml");
    if cargo_toml_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
            // Simple TOML parsing for package name and authors
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("name") && info.exe_name.is_none() {
                    if let Some(val) = line.split('=').nth(1) {
                        let val = val.trim().trim_matches('"');
                        info.exe_name = Some(val.to_string());
                        // Use package name as fallback for app name
                        if info.name.is_none() {
                            info.name = Some(val.to_string());
                        }
                    }
                }
                if line.starts_with("authors") && info.publisher.is_none() {
                    // authors = ["Name <email>"]
                    if let Some(bracket_content) = line.split('[').nth(1) {
                        if let Some(authors_str) = bracket_content.split(']').next() {
                            if let Some(first_author) = authors_str.split(',').next() {
                                let author = first_author.trim().trim_matches('"');
                                // Extract name part (before <email>)
                                let name = author.split('<').next().unwrap_or(author).trim();
                                if !name.is_empty() {
                                    info.publisher = Some(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Try reading package.json for additional info
    let package_json_path = project_dir.join("package.json");
    if package_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if info.name.is_none() {
                    if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
                        info.name = Some(name.to_string());
                    }
                }
                if info.version.is_none() {
                    if let Some(ver) = json.get("version").and_then(|v| v.as_str()) {
                        info.version = Some(ver.to_string());
                    }
                }
                if info.publisher.is_none() {
                    if let Some(author) = json.get("author").and_then(|v| v.as_str()) {
                        info.publisher = Some(author.to_string());
                    }
                }
            }
        }
    }

    Ok(info)
}

#[tauri::command]
pub fn load_config(
    path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config_path = PathBuf::from(&path).join("parcel.json");

    if !config_path.exists() {
        // No config file — generate default and save it
        let config = ParcelConfig::default();
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize default config: {e}"))?;
        std::fs::write(&config_path, &json)
            .map_err(|e| format!("Failed to write default config: {e}"))?;
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {e}", config_path.display()))?;

    *state.project_path.lock().unwrap() = Some(path);
    Ok(content)
}

#[tauri::command]
pub fn save_config(
    path: String,
    config_json: String,
) -> Result<(), String> {
    let config_path = PathBuf::from(&path).join("parcel.json");

    // Validate JSON by parsing it
    let _: serde_json::Value = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid JSON: {e}"))?;

    // Write with pretty formatting
    let value: serde_json::Value = serde_json::from_str(&config_json).unwrap();
    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to format JSON: {e}"))?;

    std::fs::write(&config_path, &pretty)
        .map_err(|e| format!("Failed to write {}: {e}", config_path.display()))?;

    Ok(())
}

// ── File browser ────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<DirEntry>, String> {
    let dir = PathBuf::from(&path);
    if !dir.exists() {
        return Err(format!("Directory not found: {}", path));
    }

    let mut entries: Vec<DirEntry> = Vec::new();

    // Add parent directory entry
    if let Some(parent) = dir.parent() {
        entries.push(DirEntry {
            name: "..".to_string(),
            path: parent.to_string_lossy().to_string(),
            is_dir: true,
        });
    }

    let read = std::fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read directory: {e}"))?;

    for entry in read.flatten() {
        let ft = entry.file_type().ok();
        let is_dir = ft.map(|t| t.is_dir()).unwrap_or(false);
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/dirs
        if name.starts_with('.') {
            continue;
        }

        entries.push(DirEntry {
            name,
            path: entry.path().to_string_lossy().to_string(),
            is_dir,
        });
    }

    // Sort: directories first, then alphabetical
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

// ── Build command ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn build_installer(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<BuildOutput, String> {
    let path = PathBuf::from(&project_path);
    if !path.join("parcel.json").exists() {
        return Err("No parcel.json found in project directory. Please save the configuration first.".into());
    }

    // Determine the workspace root (where this binary was built from)
    let workspace_root = std::env::current_exe()
        .ok()
        .and_then(|p| {
            // In dev: target/debug/parcel-setup.exe → go up 3 levels
            // In release: could be anywhere
            p.parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
        });

    // Try to find parcel CLI in workspace
    let parcel_cli = workspace_root
        .as_ref()
        .and_then(|root| {
            let candidate = root.join("target").join("release").join("parcel.exe");
            if candidate.exists() {
                Some(candidate)
            } else {
                let candidate = root.join("target").join("debug").join("parcel.exe");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            }
        });

    let app_handle = app.clone();
    let project_path_clone = project_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let emit = |line: &str, stream: &str| {
            let _ = app_handle.emit(
                "build-progress",
                BuildProgress {
                    line: line.to_string(),
                    stream: stream.to_string(),
                },
            );
        };

        let mut cmd = if let Some(cli_path) = parcel_cli {
            emit(
                &format!("Using parcel CLI: {}", cli_path.display()),
                "stdout",
            );
            let mut c = Command::new(cli_path);
            c.arg("build");
            c
        } else {
            emit("Using cargo run -p parcel-cli...", "stdout");
            let mut c = Command::new("cargo");
            c.args(["run", "-p", "parcel-cli", "--", "build"]);
            c
        };

        cmd.current_dir(&project_path_clone)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return BuildOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Failed to start build process: {e}"),
                };
            }
        };

        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().flatten() {
                stdout_buf.push_str(&line);
                stdout_buf.push('\n');
                emit(&line, "stdout");
            }
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().flatten() {
                stderr_buf.push_str(&line);
                stderr_buf.push('\n');
                emit(&line, "stderr");
            }
        }

        let status = match child.wait() {
            Ok(s) => s,
            Err(e) => {
                return BuildOutput {
                    success: false,
                    stdout: stdout_buf,
                    stderr: format!("{stderr_buf}\nProcess wait error: {e}"),
                };
            }
        };

        BuildOutput {
            success: status.success(),
            stdout: stdout_buf,
            stderr: stderr_buf,
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    Ok(result)
}

// ── File I/O commands ─────────────────────────────────────────────────

#[tauri::command]
pub fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {e}", path))
}

#[tauri::command]
pub fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write {}: {e}", path))
}
