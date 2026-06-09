//! `parcel build` — compile the installer executable.
//!
//! The build process:
//! 1. Creates a standalone Parcel installer project in `.parcel-build/`.
//! 2. Copies the Parcel source tree (parcel-core, installer runtime, frontend src/).
//! 3. Stages the target app's files as the embedded payload.
//! 4. Generates `payload-manifest.json` for compile-time embedding.
//! 5. Runs `cargo tauri build --no-bundle` inside `.parcel-build/`.
//! 6. Copies the resulting installer EXE to the user's `dist/` directory.

use crate::util;
use anyhow::{Context, Result};
use parcel_core::config::ParcelConfig;
use std::path::{Path, PathBuf};

/// Compile-time path to the Parcel workspace root (where the CLI was built).
const PARCEL_WORKSPACE: &str = env!("CARGO_MANIFEST_DIR");

/// Version marker for the installer template project.
/// When this changes, the template is rebuilt from scratch.
const TEMPLATE_VERSION: &str = "parcel-template-v2-vite";

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("parcel.json");

    if !config_path.exists() {
        anyhow::bail!(
            "parcel.json not found. Run `parcel init` first to create a configuration file."
        );
    }

    let mut config = ParcelConfig::load(&config_path)
        .context("Failed to parse parcel.json")?;

    // Resolve the actual target exe path.
    let resolved_exe = resolve_target_exe(&cwd, &config);
    if resolved_exe != config.paths.target_exe {
        println!(
            "Auto-detected target exe: {} (was: {})",
            resolved_exe.display(),
            config.paths.target_exe.display()
        );
        config.paths.target_exe = resolved_exe;
    }

    println!("Parcel Build");
    println!("  App:      {} v{}", config.app.name, config.app.version);
    println!("  Target:   {}", config.paths.target_exe.display());
    println!("  Output:   {}", config.paths.output_dir.display());
    println!();

    // ── Step 1: Validate inputs ─────────────────────────────────────
    validate_inputs(&cwd, &config)?;

    // ── Step 2: Collect files ───────────────────────────────────────
    let files = collect_target_files(&cwd, &config)?;
    println!("Collected {} file(s) to bundle.", files.len());

    // ── Step 3: Read EULA ───────────────────────────────────────────
    let eula_text = read_eula(&cwd, &config)?;

    // ── Step 4: Ensure installer template project exists ───────────
    let build_dir = cwd.join(".parcel-build");
    println!("Setting up installer project in .parcel-build/ …");
    ensure_installer_template(&build_dir, &cwd, &config)?;

    // ── Step 5: Stage payload for this specific build ──────────────
    println!("Staging payload files…");
    stage_payload(&build_dir, &cwd, &config, &files, &eula_text)?;

    // ── Step 5.5: Build uninstaller ────────────────────────────────
    println!("Building uninstaller…");
    build_uninstaller(&build_dir)?;

    // Copy uninstall.exe into the payload directory.
    let uninstaller_exe = build_dir
        .join("target")
        .join("release")
        .join("uninstall.exe");
    if uninstaller_exe.exists() {
        let payload_dir = build_dir.join("src-tauri").join("payload");
        let uninst_dest = payload_dir.join("uninstall.exe");
        std::fs::copy(&uninstaller_exe, &uninst_dest).with_context(|| {
            format!(
                "Failed to copy uninstaller from {} to {}",
                uninstaller_exe.display(),
                uninst_dest.display()
            )
        })?;
        println!("  Staged: uninstall.exe");

        // Also add uninstall.exe to the payload manifest.
        update_payload_manifest(&build_dir)?;
    } else {
        println!("  WARNING: uninstall.exe not found, skipping.");
    }

    // ── Step 6: Build installer via cargo tauri build ──────────────
    println!("Building installer (this may take a few minutes)…");
    run_tauri_build(&build_dir)?;

    // ── Step 7: Locate and copy output EXE ─────────────────────────
    let output_dir = cwd.join(&config.paths.output_dir);
    std::fs::create_dir_all(&output_dir)?;

    let installer_name = format!("{}_Setup.exe", config.app.name.replace(' ', "_"));
    let installer_exe = locate_installer_exe(&build_dir)?;

    let dest = output_dir.join(&installer_name);
    std::fs::copy(&installer_exe, &dest).with_context(|| {
        format!(
            "Failed to copy installer from {} to {}",
            installer_exe.display(),
            dest.display()
        )
    })?;

    println!();
    println!("Build complete!");
    println!("  Installer: {}", dest.display());
    println!(
        "  Size:      {} MB",
        std::fs::metadata(&dest)
            .map(|m| (m.len() as f64 / 1_048_576.0) as u64)
            .unwrap_or(0)
    );

    // ── Step 7: Add .parcel-build to .gitignore ─────────────────────
    ensure_gitignore(&cwd);

    Ok(())
}

// ── Installer project setup ─────────────────────────────────────────────

/// Ensure the installer template project exists in `build_dir`.
///
/// On first build (or when TEMPLATE_VERSION changes), this creates the full
/// installer project: copies the Parcel source tree (parcel-core, installer
/// runtime, frontend src/). On subsequent builds, it only updates tauri.conf.json
/// so the window title and product name stay current.
///
/// This avoids rebuilding the entire project from scratch every time,
/// making builds much faster and avoiding filesystem lock issues.
fn ensure_installer_template(
    build_dir: &Path,
    project_dir: &Path,
    config: &ParcelConfig,
) -> Result<()> {
    let version_file = build_dir.join(".parcel-template-version");
    let tauri_dest = build_dir.join("src-tauri");

    // Quick path: template already exists and version matches.
    if version_file.exists() && tauri_dest.join("src").join("lib.rs").exists() {
        let current = std::fs::read_to_string(&version_file).unwrap_or_default();
        if current.trim() == TEMPLATE_VERSION {
            // Template is current — just update tauri.conf.json for title/name.
            let tauri_conf = generate_tauri_conf(config);
            let _ = std::fs::write(tauri_dest.join("tauri.conf.json"), tauri_conf);
            println!("  Template up-to-date (version {}).", TEMPLATE_VERSION);
            return Ok(());
        }
        println!("  Template version changed (was '{}', now '{TEMPLATE_VERSION}'). Rebuilding...", current.trim());
    } else {
        println!("  Creating new installer template...");
    }

    // Resolve the Parcel workspace root at runtime.
    // CARGO_MANIFEST_DIR = Parcel/parcel-cli/, so one .parent() → Parcel/
    let workspace_root = PathBuf::from(PARCEL_WORKSPACE)
        .parent()
        .map(PathBuf::from)
        .context("Could not resolve Parcel workspace root")?;

    // Clean and recreate the build directory, but preserve target/ for faster rebuilds.
    if build_dir.exists() {
        let target_dir = build_dir.join("target");
        if target_dir.exists() {
            // Remove everything except target/.
            for entry in std::fs::read_dir(build_dir)?.flatten() {
                if entry.file_name() != "target" {
                    let path = entry.path();
                    // Ignore lock errors — files may be held by antivirus/sandbox.
                    let _ = if path.is_dir() {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                }
            }
        } else if std::fs::remove_dir_all(build_dir).is_err() {
            // Directory locked (sandbox/antivirus). We can still write into it
            // since it's empty — skip deletion and just proceed.
            eprintln!("  (old .parcel-build locked by another process, reusing)");
        }
    }
    std::fs::create_dir_all(build_dir)?;

    // ── Workspace Cargo.toml ────────────────────────────────────────
    std::fs::write(
        build_dir.join("Cargo.toml"),
        STANDALONE_WORKSPACE_TOML,
    )?;

    // ── parcel-core crate ───────────────────────────────────────────
    let core_src = workspace_root.join("parcel-core").join("src");
    let core_dest = build_dir.join("parcel-core");
    std::fs::create_dir_all(core_dest.join("src"))?;
    std::fs::write(core_dest.join("Cargo.toml"), STANDALONE_CORE_TOML)?;
    copy_dir_recursive(&core_src, &core_dest.join("src"))?;

    // ── parcel-uninstaller crate ────────────────────────────────────
    let uninst_src = workspace_root
        .join("parcel-uninstaller")
        .join("src");
    let uninst_dest = build_dir.join("parcel-uninstaller");
    std::fs::create_dir_all(uninst_dest.join("src"))?;
    std::fs::write(uninst_dest.join("Cargo.toml"), STANDALONE_UNINSTALLER_TOML)?;
    copy_dir_recursive(&uninst_src, &uninst_dest.join("src"))?;

    // ── src-tauri (installer runtime) ───────────────────────────────
    let tauri_src = workspace_root.join("src-tauri").join("src");
    let tauri_dest = build_dir.join("src-tauri");
    std::fs::create_dir_all(tauri_dest.join("src"))?;
    std::fs::write(
        tauri_dest.join("Cargo.toml"),
        STANDALONE_INSTALLER_TOML,
    )?;

    // Write a customised tauri.conf.json with the user's app name.
    let tauri_conf = generate_tauri_conf(config);
    std::fs::write(tauri_dest.join("tauri.conf.json"), tauri_conf)?;

    // Copy build.rs (payload generator) directly from the workspace.
    let build_rs_src = workspace_root.join("src-tauri").join("build.rs");
    std::fs::copy(&build_rs_src, tauri_dest.join("build.rs"))
        .context("Failed to copy installer build.rs")?;

    // Copy all installer source files.
    copy_dir_recursive(&tauri_src, &tauri_dest.join("src"))?;

    // Capabilities.
    let cap_src = workspace_root.join("src-tauri").join("capabilities");
    let cap_dest = tauri_dest.join("capabilities");
    if cap_src.exists() {
        copy_dir_recursive(&cap_src, &cap_dest)?;
    }

    // Icons — copy from the workspace first, then overlay from project if available.
    let workspace_icons = workspace_root.join("src-tauri").join("icons");
    let dest_icons = tauri_dest.join("icons");
    if workspace_icons.exists() {
        copy_dir_recursive(&workspace_icons, &dest_icons)?;
    }

    let project_icons = project_dir.join("src-tauri").join("icons");
    if project_icons.exists() {
        copy_dir_recursive(&project_icons, &dest_icons)?;
    }

    // Ensure icons directory exists with at least a placeholder,
    // so tauri build doesn't fail looking for icon files.
    std::fs::create_dir_all(&dest_icons)?;

    // ── Frontend (src/ + Vite) ─────────────────────────────────────
    let frontend_src = workspace_root.join("src");
    let frontend_dest = build_dir.join("src");
    // Always ensure frontend dest exists (needed for logo and other assets).
    std::fs::create_dir_all(&frontend_dest)?;
    if frontend_src.exists() {
        copy_dir_recursive(&frontend_src, &frontend_dest)?;
    }

    // Copy root-level Vite entry point and config.
    let index_html_src = workspace_root.join("index.html");
    if index_html_src.exists() {
        std::fs::copy(&index_html_src, build_dir.join("index.html"))
            .context("Failed to copy index.html")?;
    }
    let vite_config_src = workspace_root.join("vite.config.js");
    if vite_config_src.exists() {
        std::fs::copy(&vite_config_src, build_dir.join("vite.config.js"))
            .context("Failed to copy vite.config.js")?;
    }

    // Copy the user's logo into the frontend assets if available.
    let logo_src = project_dir.join(&config.appearance.logo);
    if logo_src.exists() {
        // Ensure assets directory exists.
        std::fs::create_dir_all(frontend_dest.join("assets"))?;
        let logo_dest = frontend_dest.join("assets").join("logo-placeholder.svg");
        // If the logo is a PNG, also create a proper file.
        let logo_png_dest = frontend_dest.join("assets").join("logo.png");
        if logo_src.extension().and_then(|e| e.to_str()) == Some("png") {
            std::fs::copy(&logo_src, &logo_png_dest)?;
        } else {
            std::fs::copy(&logo_src, &logo_dest)?;
        }
    }

    // ── Write template version marker ──────────────────────────────
    std::fs::write(&version_file, TEMPLATE_VERSION)?;

    // ── package.json (for @tauri-apps/cli) ──────────────────────────
    std::fs::write(
        build_dir.join("package.json"),
        PACKAGE_JSON,
    )?;

    Ok(())
}

/// Stage payload files and generate the manifest for this specific build.
///
/// This runs on EVERY build, updating the payload directory and manifest
/// in the existing template project so `cargo tauri build` picks up the
/// latest target app files.
fn stage_payload(
    build_dir: &Path,
    project_dir: &Path,
    config: &ParcelConfig,
    files: &[String],
    eula_text: &str,
) -> Result<()> {
    let tauri_dest = build_dir.join("src-tauri");
    let payload_dir = tauri_dest.join("payload");

    // Ensure parent directories exist.
    std::fs::create_dir_all(&tauri_dest)?;

    // Clean old payload and recreate.
    let _ = std::fs::remove_dir_all(&payload_dir);
    std::fs::create_dir_all(&payload_dir)?;

    let mut payload_files: Vec<String> = Vec::new();

    for relative in files {
        let src = project_dir.join(relative);
        if !src.exists() {
            println!("  Warning: payload file not found, skipping: {}", src.display());
            continue;
        }
        let filename = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(relative)
            .to_string();
        let dest = payload_dir.join(&filename);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest).with_context(|| {
            format!("Failed to copy {} to payload/", src.display())
        })?;
        payload_files.push(filename);
        println!("  Staged: {}", relative);
    }

    // Generate payload-manifest.json.
    let manifest = serde_json::json!({
        "parcel": config,
        "eula_text": eula_text,
        "payload_files": &payload_files,
        "parcel_version": env!("CARGO_PKG_VERSION"),
    });
    std::fs::write(
        tauri_dest.join("payload-manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!("  Generated payload-manifest.json");
    Ok(())
}

/// Generate a customised `tauri.conf.json` for the installer.
fn generate_tauri_conf(config: &ParcelConfig) -> String {
    let name = &config.app.name;
    let identifier = &config.app.identifier;
    format!(
        r#"{{
  "productName": "{name} Setup",
  "version": "{}",
  "identifier": "{identifier}",
  "build": {{
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../dist"
  }},
  "app": {{
    "withGlobalTauri": true,
    "windows": [
      {{
        "label": "main",
        "title": "{name} Setup",
        "width": 720,
        "height": 520,
        "minWidth": 640,
        "minHeight": 480,
        "center": true,
        "resizable": true,
        "decorations": true,
        "transparent": false
      }}
    ],
    "security": {{
      "csp": null
    }}
  }},
  "bundle": {{
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/icon.png", "icons/icon.ico"],
    "windows": {{
      "nsis": {{
        "installMode": "currentUser"
      }}
    }}
  }}
}}
"#,
        config.app.version
    )
}

// ── Uninstaller build ───────────────────────────────────────────────────

/// Compile the uninstaller binary (lightweight console app, no Tauri).
fn build_uninstaller(build_dir: &Path) -> Result<()> {
    let status = run_shell_command(
        "cargo",
        &["build", "-p", "parcel-uninstaller", "--release"],
        build_dir,
        &[],
    )
    .context("Failed to run `cargo build -p parcel-uninstaller`")?;

    if !status.success() {
        anyhow::bail!(
            "`cargo build -p parcel-uninstaller` failed with exit code: {:?}",
            status.code()
        );
    }

    Ok(())
}

/// Append `uninstall.exe` to the payload manifest after it has been built.
fn update_payload_manifest(build_dir: &Path) -> Result<()> {
    let manifest_path = build_dir.join("src-tauri").join("payload-manifest.json");
    if !manifest_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&manifest_path)?;
    let mut manifest: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(files) = manifest.get_mut("payload_files").and_then(|v| v.as_array_mut()) {
        let name = "uninstall.exe".to_string();
        if !files.iter().any(|f| f.as_str() == Some(&name)) {
            files.push(serde_json::Value::String(name));
        }
    }

    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)?,
    )?;

    Ok(())
}

// ── Tauri build ─────────────────────────────────────────────────────────

/// Invoke `cargo tauri build` to compile the installer.
fn run_tauri_build(build_dir: &Path) -> Result<()> {
    // Install npm dependencies if not already present.
    if !build_dir.join("node_modules").exists() {
        println!("  Installing @tauri-apps/cli…");
        let npm_status = run_shell_command("npm", &["install"], build_dir, &[])
            .context("Failed to run `npm install`. Is Node.js installed?")?;

        if !npm_status.success() {
            anyhow::bail!("`npm install` failed in .parcel-build/");
        }
    }

    // Build.
    let env_vars = [("CARGO_PROFILE_RELEASE_OPT_LEVEL", "s")];
    let status = run_shell_command("cargo", &["tauri", "build", "--no-bundle"], build_dir, &env_vars)
        .context(
            "Failed to run `cargo tauri build`. Is @tauri-apps/cli installed?",
        )?;

    if !status.success() {
        anyhow::bail!(
            "`cargo tauri build` failed with exit code: {:?}",
            status.code()
        );
    }

    Ok(())
}

/// Run a command through the system shell, which is needed on Windows
/// to resolve `.cmd` / `.bat` scripts (like `npm.cmd`).
fn run_shell_command(
    program: &str,
    args: &[&str],
    cwd: &Path,
    env_vars: &[(&str, &str)],
) -> std::io::Result<std::process::ExitStatus> {
    let mut cmd = if cfg!(target_os = "windows") {
        let cmd_line = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &cmd_line]);
        c
    } else {
        let mut c = std::process::Command::new(program);
        c.args(args);
        c
    };
    cmd.current_dir(cwd);
    for (key, val) in env_vars {
        cmd.env(key, val);
    }
    cmd.status()
}

/// Locate the compiled installer exe after `cargo tauri build`.
fn locate_installer_exe(build_dir: &Path) -> Result<PathBuf> {
    // Search in known locations.
    // When using a workspace, the target dir is at the workspace root.
    // When not using a workspace, it's inside src-tauri/.
    let candidates = [
        // Workspace-level target directory (most common with our setup).
        "target/release/parcel-installer.exe",
        // Inside src-tauri/ target directory.
        "src-tauri/target/release/parcel-installer.exe",
        // Alternative names.
        "target/release/installer.exe",
        "src-tauri/target/release/installer.exe",
    ];

    for candidate in &candidates {
        let path = build_dir.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    // Fallback: find any .exe in target/release/ that isn't a build tool.
    for release_dir_rel in &["target/release", "src-tauri/target/release"] {
        let release_dir = build_dir.join(release_dir_rel);
        if release_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&release_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("exe") {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        // Skip known build tools and intermediate artefacts.
                        if !name.starts_with("cargo")
                            && !name.starts_with("rustc")
                            && name != "parcel"
                            && name != "parcel-cli"
                            && !name.starts_with("build_script")
                        {
                            return Ok(path);
                        }
                    }
                }
            }
        }
    }

    let release_dir = build_dir.join("target").join("release");
    anyhow::bail!(
        "Could not locate the compiled installer exe.\n\
         Expected it in: {} or {}",
        release_dir.display(),
        build_dir.join("src-tauri").join("target").join("release").display()
    )
}

// ── Exe auto-detection ──────────────────────────────────────────────────

fn resolve_target_exe(cwd: &Path, config: &ParcelConfig) -> PathBuf {
    if cwd.join(&config.paths.target_exe).exists() {
        return config.paths.target_exe.clone();
    }

    let exe_name = detect_exe_name(cwd);
    if let Some(found) = util::find_target_exe(cwd, &exe_name) {
        return found;
    }

    config.paths.target_exe.clone()
}

fn detect_exe_name(cwd: &Path) -> String {
    let cargo_toml_path = cwd.join("src-tauri").join("Cargo.toml");
    let (bin_name, pkg_name) = util::read_cargo_names(&cargo_toml_path);

    if let Some(name) = bin_name {
        return name;
    }

    // Cargo uses the package name for the binary, not productName.
    pkg_name.unwrap_or_else(|| "app".into())
}

// ── Validation ──────────────────────────────────────────────────────────

fn validate_inputs(cwd: &Path, config: &ParcelConfig) -> Result<()> {
    // Ensure output directory exists (auto-create if missing).
    let output_dir = cwd.join(&config.paths.output_dir);
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;
        println!("  Created output directory: {}", output_dir.display());
    }

    let exe_path = cwd.join(&config.paths.target_exe);
    if !exe_path.exists() {
        let detected_name = detect_exe_name(cwd);
        // Try auto-detecting the exe
        if let Some(found) = util::find_target_exe(cwd, &detected_name) {
            println!(
                "  Note: configured exe '{}' not found, auto-detected: {}",
                config.paths.target_exe.display(),
                found.display()
            );
        } else {
            anyhow::bail!(
                "Target executable not found: {}\n\
                 Hint: run `cargo tauri build --no-bundle` in your project first.\n\
                 Expected exe name based on project config: \"{detected_name}.exe\"\n\
                 Searched locations:\n\
                 - {}\n\
                 - target/release/{detected_name}.exe",
                config.paths.target_exe.display(),
                PathBuf::from(format!("src-tauri/target/release/{detected_name}.exe")).display(),
            );
        }
    }

    if let Some(ref eula_path) = config.eula.file {
        let full = cwd.join(eula_path);
        if !full.exists() {
            println!(
                "  Warning: EULA file not found at {}. Using default license text.",
                eula_path.display()
            );
        }
    }

    let logo_path = cwd.join(&config.appearance.logo);
    if !logo_path.exists() {
        println!(
            "  Warning: Logo not found at {}. Using placeholder.",
            config.appearance.logo.display()
        );
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_eula(cwd: &Path, config: &ParcelConfig) -> Result<String> {
    if let Some(ref eula_path) = config.eula.file {
        let full = cwd.join(eula_path);
        if full.exists() {
            return std::fs::read_to_string(&full)
                .with_context(|| format!("Failed to read EULA file: {}", full.display()));
        }
        // File doesn't exist — fall through to default.
        println!(
            "  Warning: EULA file '{}' not found, using default text.",
            eula_path.display()
        );
    }
    Ok(String::from(
        "END USER LICENSE AGREEMENT\n\n\
         This software is provided \"as-is\" without warranty of any kind.\n\
         Use of this software is subject to the terms and conditions\n\
         provided by the publisher.\n\n\
         Please contact the publisher for the full license terms.\n",
    ))
}

fn collect_target_files(cwd: &Path, config: &ParcelConfig) -> Result<Vec<String>> {
    let mut files = Vec::new();

    files.push(config.paths.target_exe.to_string_lossy().to_string());

    for pattern in &config.paths.resources {
        let full_pattern = cwd.join(pattern).to_string_lossy().to_string();
        for entry in glob::glob(&full_pattern)? {
            let path = entry?;
            if let Ok(relative) = path.strip_prefix(cwd) {
                files.push(relative.to_string_lossy().to_string());
            }
        }
    }

    Ok(files)
}

/// Ensure `.parcel-build` is in the project's `.gitignore`.
fn ensure_gitignore(cwd: &Path) {
    let gitignore = cwd.join(".gitignore");
    let entry = ".parcel-build";
    if gitignore.exists() {
        if let Ok(content) = std::fs::read_to_string(&gitignore) {
            if content.lines().any(|l| l.trim() == entry) {
                return;
            }
        }
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .open(&gitignore)
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "\n# Parcel installer build directory\n{entry}")
            });
    } else {
        let _ = std::fs::write(
            &gitignore,
            format!("# Parcel installer build directory\n{entry}\n"),
        );
    }
}

/// Recursively copy a directory, skipping common build artefacts.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip build artefacts and caches.
        if matches!(
            name_str.as_ref(),
            "target" | "node_modules" | ".git" | "gen" | ".parcel-build"
        ) {
            continue;
        }

        let src_path = entry.path();
        let dest_path = dest.join(&name);

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

// ── Embedded templates ──────────────────────────────────────────────────

const STANDALONE_WORKSPACE_TOML: &str = r#"[workspace]
resolver = "2"
members = ["parcel-core", "parcel-uninstaller", "src-tauri"]

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Parcel"]
license = "MIT"

[workspace.dependencies]
parcel-core = { path = "parcel-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
"#;

const STANDALONE_CORE_TOML: &str = r#"[package]
name = "parcel-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "Core types and configuration for the Parcel installer framework"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
"#;

const STANDALONE_INSTALLER_TOML: &str = r#"[package]
name = "parcel-installer"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "Parcel installer runtime"

[build-dependencies]
tauri-build = { version = "2", features = [] }
serde_json = "1"

[dependencies]
parcel-core = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
log = "0.4"
env_logger = "0.11"

[target.'cfg(windows)'.dependencies]
winreg = "0.55"
mslnk = "0.1"
"#;

const STANDALONE_UNINSTALLER_TOML: &str = r#"[package]
name = "parcel-uninstaller"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "Parcel uninstaller — removes applications installed by Parcel"

[[bin]]
name = "uninstall"
path = "src/main.rs"

[dependencies]
parcel-core = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
log = "0.4"
env_logger = "0.11"

[target.'cfg(windows)'.dependencies]
winreg = "0.55"
"#;

const PACKAGE_JSON: &str = r#"{
  "name": "parcel-installer-build",
  "version": "1.0.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "@tauri-apps/plugin-shell": "^2"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "vite": "^6"
  }
}
"#;
