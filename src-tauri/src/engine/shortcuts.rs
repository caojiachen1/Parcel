//! Shortcut creation — desktop and Start Menu shortcuts on Windows.

use anyhow::Result;
use std::path::Path;

/// Create a desktop shortcut (.lnk) pointing to the installed executable.
#[cfg(target_os = "windows")]
pub fn create_desktop_shortcut(name: &str, exe_path: &Path) -> Result<()> {
    use mslnk::ShellLink;

    let desktop = dirs_next();
    let lnk_path = desktop.join(format!("{name}.lnk"));

    let mut sl = ShellLink::new(exe_path.to_string_lossy().as_ref())?;
    sl.set_name(Some(name.into()));
    sl.set_working_dir(Some(
        exe_path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .into(),
    ));
    sl.create_lnk(&lnk_path)?;

    log::info!("Created desktop shortcut: {}", lnk_path.display());
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn create_desktop_shortcut(_name: &str, _exe_path: &Path) -> Result<()> {
    log::warn!("Desktop shortcuts are only supported on Windows.");
    Ok(())
}

/// Create a Start Menu shortcut (.lnk) for the installed application.
#[cfg(target_os = "windows")]
pub fn create_start_menu_shortcut(name: &str, exe_path: &Path) -> Result<()> {
    use mslnk::ShellLink;

    let start_menu = start_menu_dir();
    std::fs::create_dir_all(&start_menu)?;
    let lnk_path = start_menu.join(format!("{name}.lnk"));

    let mut sl = ShellLink::new(exe_path.to_string_lossy().as_ref())?;
    sl.set_name(Some(name.into()));
    sl.set_working_dir(Some(
        exe_path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .into(),
    ));
    sl.create_lnk(&lnk_path)?;

    log::info!("Created Start Menu shortcut: {}", lnk_path.display());
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn create_start_menu_shortcut(_name: &str, _exe_path: &Path) -> Result<()> {
    log::warn!("Start Menu shortcuts are only supported on Windows.");
    Ok(())
}

/// Remove desktop shortcut.
pub fn remove_desktop_shortcut(name: &str) -> Result<()> {
    let desktop = dirs_next();
    let lnk_path = desktop.join(format!("{name}.lnk"));
    if lnk_path.exists() {
        std::fs::remove_file(&lnk_path)?;
        log::info!("Removed desktop shortcut: {}", lnk_path.display());
    }
    Ok(())
}

/// Remove Start Menu shortcut.
pub fn remove_start_menu_shortcut(name: &str) -> Result<()> {
    let start_menu = start_menu_dir();
    let lnk_path = start_menu.join(format!("{name}.lnk"));
    if lnk_path.exists() {
        std::fs::remove_file(&lnk_path)?;
        log::info!("Removed Start Menu shortcut: {}", lnk_path.display());
    }
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn dirs_next() -> std::path::PathBuf {
    std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join("Desktop"))
        .unwrap_or_else(|_| std::path::PathBuf::from(r"C:\Users\Default\Desktop"))
}

fn start_menu_dir() -> std::path::PathBuf {
    std::env::var("APPDATA")
        .map(|p| {
            std::path::PathBuf::from(p)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs")
        })
}
