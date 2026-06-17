//! `parcel setup` — launch the Parcel Setup GUI.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Compile-time path to the Parcel workspace root.
const PARCEL_WORKSPACE: &str = env!("CARGO_MANIFEST_DIR");

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_string_lossy().to_string();

    println!("Launching Parcel Setup for: {}", cwd.display());

    // Resolve the workspace root (CARGO_MANIFEST_DIR = parcel-cli/).
    let manifest_dir = PathBuf::from(PARCEL_WORKSPACE);
    let workspace_root = manifest_dir
        .parent()
        .context("Could not resolve Parcel workspace root")?;

    let setup_dir = workspace_root.join("parcel-setup");

    // Check if parcel-setup has been built (look for the binary).
    // Two possible locations:
    //   1. workspace_root/target/...           (cargo build -p parcel-setup)
    //   2. parcel-setup/src-tauri/target/...   (cargo tauri build)
    let exe_name = format!("parcel-setup{}", std::env::consts::EXE_SUFFIX);
    let candidates = [
        // Workspace-level target (cargo build from workspace)
        workspace_root.join("target").join("release").join(&exe_name),
        workspace_root.join("target").join("debug").join(&exe_name),
        // Tauri-specific target directory
        setup_dir.join("src-tauri").join("target").join("release").join(&exe_name),
        setup_dir.join("src-tauri").join("target").join("debug").join(&exe_name),
    ];

    let setup_exe = candidates.iter().find(|p| p.exists());

    let setup_exe = match setup_exe {
        Some(path) => path.clone(),
        None => {
            // Not built yet — build it first, then launch with elevation.
            println!("Parcel Setup not built yet. Building…");
            launch_dev_mode(&setup_dir, &cwd_str)?;
            return Ok(());
        }
    };

    println!("Using: {}", setup_exe.display());

    // Launch the setup executable with the current directory as argument.
    launch_with_elevation(&setup_exe, &[&cwd_str])?;

    println!("Parcel Setup launched.");

    Ok(())
}

/// Convert a Rust string to a null-terminated wide string (UTF-16) for Win32 API.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Launch a process, automatically requesting UAC elevation on Windows if needed (error 740).
fn launch_with_elevation(exe: &std::path::Path, args: &[&str]) -> Result<()> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);

    match cmd.spawn() {
        Ok(child) => {
            std::mem::forget(child);
            Ok(())
        }
        #[cfg(target_os = "windows")]
        Err(e) if e.raw_os_error() == Some(740) || e.raw_os_error() == Some(5) => {
            // Error 740: "The requested operation requires elevation."
            // Error   5: "Access denied" — some Windows configurations
            //            also return this when elevation is needed.
            shell_execute_elevated(exe, args)
        }
        Err(e) => Err(e).context("Failed to launch Parcel Setup")?,
    }
}

/// Use Windows ShellExecuteW to launch a process with UAC elevation.
#[cfg(target_os = "windows")]
fn shell_execute_elevated(exe: &std::path::Path, args: &[&str]) -> Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe_wide = to_wide(&exe.to_string_lossy());

    // Build parameter string: arg1 arg2 ...
    // ShellExecuteW uses lpFile as the executable; lpParameters is JUST the
    // arguments to pass.  Do NOT include the exe path here.
    let param_string = {
        let mut s = String::new();
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push('"');
            s.push_str(a);
            s.push('"');
        }
        s
    };
    let param_wide = to_wide(&param_string);
    let verb_wide = to_wide("runas");

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb_wide.as_ptr(),
            exe_wide.as_ptr(),
            param_wide.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result as isize <= 32 {
        match result as i32 {
            5 | 1223 => anyhow::bail!("UAC elevation was cancelled by the user."),
            code => anyhow::bail!(
                "Failed to launch with elevation. ShellExecuteW returned error code: {}",
                code
            ),
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn shell_execute_elevated(_exe: &std::path::Path, _args: &[&str]) -> Result<()> {
    anyhow::bail!("Elevation is only supported on Windows.");
}

/// Build parcel-setup from source, then launch with elevation.
///
/// We avoid `cargo tauri dev` here because cargo/tauri internally launches the
/// binary via `CreateProcess`, which returns error 740 when elevation is needed
/// — and cargo has no UAC fallback.  The user would see:
/// ```
/// error: could not execute process `...parcel-setup.exe` (never executed)
/// Caused by: 请求的操作需要提升。 (os error 740)
/// ```
///
/// Instead: build with `cargo tauri build --no-bundle` (no elevation needed),
/// then hand the resulting binary to `launch_with_elevation` which correctly
/// handles 740/5 → UAC prompt.
fn launch_dev_mode(setup_dir: &std::path::Path, project_path: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let workspace_root = setup_dir
            .parent()
            .context("Could not resolve workspace root")?;

        println!("  Building parcel-setup (cargo tauri build --no-bundle)…");
        let status = std::process::Command::new("cargo")
            .args(["tauri", "build", "--no-bundle"])
            .current_dir(setup_dir.join("src-tauri"))
            .status()
            .context("Failed to build parcel-setup. Is Rust installed?")?;
        if !status.success() {
            anyhow::bail!("`cargo tauri build --no-bundle` failed");
        }

        // `cargo tauri build` compiles in release mode; the binary lives
        // in the workspace-level target dir (or the crate-local one).
        let exe_name = format!("parcel-setup{}", std::env::consts::EXE_SUFFIX);
        let candidates = [
            workspace_root.join("target").join("release").join(&exe_name),
            setup_dir
                .join("src-tauri")
                .join("target")
                .join("release")
                .join(&exe_name),
        ];

        let binary = candidates.iter().find(|p| p.exists());
        let binary = match binary {
            Some(p) => p.clone(),
            None => anyhow::bail!(
                "Build completed but expected binary not found in:\n  {}\n  {}",
                candidates[0].display(),
                candidates[1].display(),
            ),
        };

        println!("Built: {}", binary.display());
        launch_with_elevation(&binary, &[project_path])?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let workspace_root = setup_dir
            .parent()
            .context("Could not resolve workspace root")?;
        let status = std::process::Command::new("cargo")
            .args(["tauri", "build", "--no-bundle"])
            .current_dir(setup_dir.join("src-tauri"))
            .status()
            .context("Failed to build parcel-setup. Is Rust installed?")?;
        if !status.success() {
            anyhow::bail!("`cargo tauri build --no-bundle` failed");
        }
        Ok(())
    }
}
