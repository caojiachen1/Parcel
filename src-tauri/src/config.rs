//! Payload configuration loader.
//!
//! Loading priority:
//! 1. Embedded payload data (compile-time `include_bytes!` from `build.rs`).
//! 2. `PARCEL_CONFIG` env var → preview / dev mode.
//! 3. Defaults (for `cargo tauri dev` without payload).

use crate::payload_data;
use parcel_core::config::ParcelConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The payload configuration embedded into / loaded by the installer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadConfig {
    /// Full parcel configuration.
    pub parcel: ParcelConfig,
    /// Plain-text EULA content.
    pub eula_text: String,
    /// List of file paths to install (relative paths inside the payload).
    pub payload_files: Vec<String>,
    /// Parcel version that generated this installer.
    pub parcel_version: String,
}

impl Default for PayloadConfig {
    fn default() -> Self {
        Self {
            parcel: ParcelConfig::default(),
            eula_text: String::from(
                "END USER LICENSE AGREEMENT\n\n\
                 This is a placeholder EULA for preview mode.\n\
                 Replace this text with your actual license terms in parcel/eula.txt.\n",
            ),
            payload_files: vec![],
            parcel_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Resolve the default installation directory by expanding placeholders.
pub fn resolve_default_dir(config: &PayloadConfig) -> PathBuf {
    let template = &config.parcel.install.default_dir;
    let name = &config.parcel.app.name;

    let localappdata = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| r"C:\Users\Default\AppData\Local".into());
    let programfiles = std::env::var("ProgramFiles")
        .unwrap_or_else(|_| r"C:\Program Files".into());

    let resolved = template
        .replace("{localappdata}", &localappdata)
        .replace("{programfiles}", &programfiles)
        .replace("{name}", name);

    PathBuf::from(resolved)
}

/// Load the payload configuration at startup.
pub fn load_payload() -> PayloadConfig {
    // 1) Try embedded payload (compile-time via build.rs).
    if !payload_data::FILES.is_empty() {
        match serde_json::from_str::<PayloadConfig>(payload_data::MANIFEST_JSON) {
            Ok(payload) => {
                log::info!(
                    "Loaded embedded payload ({} file(s)).",
                    payload.payload_files.len()
                );
                return payload;
            }
            Err(e) => {
                log::warn!("Failed to parse embedded manifest: {e}");
            }
        }
    }

    // 2) Preview mode: read from PARCEL_CONFIG env var.
    if let Ok(config_path) = std::env::var("PARCEL_CONFIG") {
        let path = std::path::Path::new(&config_path);
        if path.exists() {
            match ParcelConfig::load(path) {
                Ok(parcel) => {
                    let manifest_path = path
                        .parent()
                        .unwrap()
                        .join("src-tauri")
                        .join("payload-manifest.json");
                    if manifest_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(payload) =
                                serde_json::from_str::<PayloadConfig>(&content)
                            {
                                log::info!("Loaded payload from manifest (preview mode).");
                                return payload;
                            }
                        }
                    }
                    log::info!("Loaded config for preview (no manifest).");
                    return PayloadConfig {
                        parcel,
                        ..PayloadConfig::default()
                    };
                }
                Err(e) => {
                    log::warn!("Failed to load config from PARCEL_CONFIG: {e}");
                }
            }
        }
    }

    // 3) Fallback: defaults (for development / `cargo tauri dev`).
    log::info!("Using default payload configuration.");
    PayloadConfig::default()
}

/// Extract all embedded payload files to the given directory.
///
/// Returns the list of files that were written (relative paths).
pub fn extract_payload_files(install_dir: &std::path::Path) -> Vec<(String, std::io::Result<PathBuf>)> {
    payload_data::FILES
        .iter()
        .map(|(relative, data)| {
            let dest = install_dir.join(relative);
            let result = write_payload_file(&dest, data);
            (relative.to_string(), result)
        })
        .collect()
}

fn write_payload_file(dest: &std::path::Path, data: &[u8]) -> std::io::Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, data)?;
    Ok(dest.to_path_buf())
}
