//! `parcel init` — scaffold configuration and resource directory.

use crate::util;
use anyhow::Result;
use parcel_core::config::ParcelConfig;
use std::path::{Path, PathBuf};

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("parcel.json");

    if config_path.exists() {
        println!("parcel.json already exists, skipping.");
        return Ok(());
    }

    let mut config = ParcelConfig::default();
    let mut detected = false;

    // ── Detect existing Tauri project ───────────────────────────────
    if let Some(info) = detect_tauri_project(&cwd) {
        detected = true;

        if let Some(ref name) = info.product_name {
            config.app.name = name.clone();
        }
        if let Some(ref version) = info.version {
            config.app.version = version.clone();
        }
        if let Some(ref identifier) = info.identifier {
            config.app.identifier = identifier.clone();
        }

        // Determine the exe name and set the correct target path.
        let exe_name = info.exe_name();
        // If the exe already exists, use the real path; otherwise use
        // the canonical src-tauri/target/release/ location.
        config.paths.target_exe = util::find_target_exe(&cwd, &exe_name)
            .unwrap_or_else(|| util::default_target_exe(&exe_name));

        // Auto-detect icon from declared paths.
        if let Some(ref icon_src) = info.best_icon(&cwd) {
            config.paths.icon = icon_src.clone();
        }

        println!("Detected existing Tauri project:");
        println!("  Name:       {}", config.app.name);
        println!("  Version:    {}", config.app.version);
        println!("  Target exe: {}", config.paths.target_exe.display());
    }

    // ── Create parcel/ resource directory ───────────────────────────
    let parcel_dir = cwd.join("parcel");
    std::fs::create_dir_all(parcel_dir.join("assets"))?;

    // Copy icon to parcel/assets/ (prefer icon.png, fall back to others).
    let logo_dest = parcel_dir.join("assets").join("logo.png");
    if !logo_dest.exists() {
        if let Some(icon_src) = find_best_icon(&cwd.join("src-tauri").join("icons")) {
            std::fs::copy(&icon_src, &logo_dest)?;
            println!(
                "  Logo:       copied {} -> parcel/assets/logo.png",
                icon_src.display()
            );
            config.appearance.logo = PathBuf::from("parcel/assets/logo.png");
        }
    }

    // ── Write parcel.json ───────────────────────────────────────────
    config.save(&config_path)?;

    if detected {
        println!("Pre-filled defaults from project config.");
    }
    println!("Created parcel.json");

    // ── Create EULA template ────────────────────────────────────────
    let eula_path = parcel_dir.join("eula.txt");
    if !eula_path.exists() {
        std::fs::write(
            &eula_path,
            "END USER LICENSE AGREEMENT\n\n\
             Replace this text with your own license terms.\n",
        )?;
        println!("Created parcel/eula.txt");
    }

    println!("\nParcel initialised successfully!");
    println!("  - Edit parcel.json to configure your installer.");
    println!("  - Place your logo and assets in parcel/assets/.");
    println!("  - Run `parcel build` to generate the installer.");

    Ok(())
}

// ── Tauri project detection ─────────────────────────────────────────────

struct TauriProjectInfo {
    product_name: Option<String>,
    version: Option<String>,
    identifier: Option<String>,
    bin_name: Option<String>,
    declared_icons: Vec<PathBuf>,
}

impl TauriProjectInfo {
    /// Determine the expected exe filename (without extension).
    fn exe_name(&self) -> String {
        if let Some(ref bin) = self.bin_name {
            return bin.clone();
        }
        if let Some(ref name) = self.product_name {
            return name.clone();
        }
        "app".into()
    }

    fn best_icon(&self, cwd: &Path) -> Option<PathBuf> {
        // 1) Check declared icons from tauri.conf.json — prefer .png
        for icon in &self.declared_icons {
            let full = cwd.join("src-tauri").join(icon);
            if full.exists() && is_png(&full) {
                // Return as a relative path (src-tauri/icons/...)
                return Some(PathBuf::from("src-tauri").join(icon));
            }
        }

        // 2) Scan the icons directory.
        let icons_dir = cwd.join("src-tauri").join("icons");
        find_best_icon(&icons_dir).map(|abs| {
            // Convert absolute path to relative (src-tauri/icons/...)
            abs.strip_prefix(cwd).map(PathBuf::from).unwrap_or(abs)
        })
    }
}

fn detect_tauri_project(cwd: &Path) -> Option<TauriProjectInfo> {
    let tauri_conf_path = cwd.join("src-tauri").join("tauri.conf.json");
    if !tauri_conf_path.exists() {
        return None;
    }

    let tauri_content = std::fs::read_to_string(&tauri_conf_path).ok()?;
    let tauri_json: serde_json::Value = serde_json::from_str(&tauri_content).ok()?;

    let product_name = tauri_json
        .get("productName")
        .and_then(|v| v.as_str())
        .map(String::from);

    let version = tauri_json
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    let identifier = tauri_json
        .get("identifier")
        .and_then(|v| v.as_str())
        .map(String::from);

    let declared_icons: Vec<PathBuf> = tauri_json
        .pointer("/bundle/icon")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();

    // Read Cargo.toml for binary name.
    let cargo_toml_path = cwd.join("src-tauri").join("Cargo.toml");
    let (bin_name, pkg_name) = util::read_cargo_names(&cargo_toml_path);

    // Cargo uses [[bin]] name if declared, otherwise the package name
    // (with hyphens preserved).  productName from tauri.conf.json often
    // has spaces and does NOT match the actual binary filename.
    let effective_bin = bin_name.or(pkg_name);

    Some(TauriProjectInfo {
        product_name,
        version,
        identifier,
        bin_name: effective_bin,
        declared_icons,
    })
}

// ── Icon detection ──────────────────────────────────────────────────────

fn find_best_icon(icons_dir: &Path) -> Option<PathBuf> {
    if !icons_dir.is_dir() {
        return None;
    }

    let entries: Vec<_> = match std::fs::read_dir(icons_dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return None,
    };

    // 1. icon.png
    let icon_png = icons_dir.join("icon.png");
    if icon_png.exists() {
        return Some(icon_png);
    }

    // 2. any other .png
    for entry in &entries {
        if is_png(&entry.path()) {
            return Some(entry.path());
        }
    }

    // 3. icon.svg
    let icon_svg = icons_dir.join("icon.svg");
    if icon_svg.exists() {
        return Some(icon_svg);
    }

    // 4. any .svg
    for entry in &entries {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("svg") {
            return Some(entry.path());
        }
    }

    // 5. icon.ico
    let icon_ico = icons_dir.join("icon.ico");
    if icon_ico.exists() {
        return Some(icon_ico);
    }

    // 6. jpg / jpeg
    for entry in &entries {
        match entry.path().extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") => return Some(entry.path()),
            _ => {}
        }
    }

    None
}

fn is_png(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_name_prefers_bin_name() {
        let info = TauriProjectInfo {
            product_name: Some("My Product".into()),
            version: None,
            identifier: None,
            bin_name: Some("my-app".into()),
            declared_icons: vec![],
        };
        assert_eq!(info.exe_name(), "my-app");
    }

    #[test]
    fn exe_name_falls_back_to_product_name() {
        let info = TauriProjectInfo {
            product_name: Some("My Product".into()),
            version: None,
            identifier: None,
            bin_name: None,
            declared_icons: vec![],
        };
        assert_eq!(info.exe_name(), "My Product");
    }
}
