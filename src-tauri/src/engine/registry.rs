//! Registry operations — uninstall info, auto-start, file associations.

use anyhow::Result;
use parcel_core::config::{FileAssociation, ParcelConfig};
use std::path::Path;

/// Write the "Add/Remove Programs" uninstall entry to the Windows registry.
#[cfg(target_os = "windows")]
pub fn write_uninstall_info(config: &ParcelConfig, install_dir: &Path) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        config.app.identifier
    );
    let (key, _) = hkcu.create_subkey(&key_path)?;

    let uninstall_exe = install_dir.join("uninstall.exe");

    key.set_value("DisplayName", &config.app.name)?;
    key.set_value("DisplayVersion", &config.app.version)?;
    key.set_value("Publisher", &config.app.publisher)?;
    key.set_value(
        "InstallLocation",
        &install_dir.to_string_lossy().to_string(),
    )?;
    key.set_value(
        "UninstallString",
        &format!("\"{}\"", uninstall_exe.display()),
    )?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;

    log::info!("Wrote uninstall registry entry: {key_path}");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn write_uninstall_info(_config: &ParcelConfig, _install_dir: &Path) -> Result<()> {
    log::warn!("Registry operations are only supported on Windows.");
    Ok(())
}

/// Remove the uninstall registry entry.
#[cfg(target_os = "windows")]
pub fn remove_uninstall_info(identifier: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{identifier}"
    );
    let _ = hkcu.delete_subkey_all(&key_path);
    log::info!("Removed uninstall registry entry.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn remove_uninstall_info(_identifier: &str) -> Result<()> {
    Ok(())
}

/// Write a Run registry entry for auto-start at logon.
#[cfg(target_os = "windows")]
pub fn write_auto_start(app_name: &str, exe_path: &Path) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let (key, _) = hkcu.create_subkey(key_path)?;

    key.set_value(
        app_name,
        &format!("\"{}\"", exe_path.display()),
    )?;
    log::info!("Wrote auto-start registry entry for {app_name}.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn write_auto_start(_app_name: &str, _exe_path: &Path) -> Result<()> {
    Ok(())
}

/// Remove the auto-start registry entry.
#[cfg(target_os = "windows")]
pub fn remove_auto_start(app_name: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    if let Ok((key, _)) = hkcu.create_subkey(key_path) {
        let _ = key.delete_value(app_name);
    }
    log::info!("Removed auto-start registry entry for {app_name}.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn remove_auto_start(_app_name: &str) -> Result<()> {
    Ok(())
}

/// Register a file association in the registry.
#[cfg(target_os = "windows")]
pub fn write_file_association(assoc: &FileAssociation, exe_path: &Path) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcr = RegKey::predef(HKEY_CURRENT_USER);
    let ext_key = format!(r"Software\Classes\.{}", assoc.extension);
    let (key, _) = hkcr.create_subkey(&ext_key)?;

    let prog_id = format!("Parcel.{}", assoc.extension);
    key.set_value("", &prog_id)?;

    let prog_key = format!(r"Software\Classes\{}", prog_id);
    let (pk, _) = hkcr.create_subkey(&prog_key)?;
    pk.set_value("", &assoc.description)?;

    let cmd_key = format!(r"Software\Classes\{}\shell\open\command", prog_id);
    let (ck, _) = hkcr.create_subkey(&cmd_key)?;
    ck.set_value("", &format!("\"{}\" \"%1\"", exe_path.display()))?;

    log::info!("Registered file association .{}", assoc.extension);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn write_file_association(_assoc: &FileAssociation, _exe_path: &Path) -> Result<()> {
    Ok(())
}

/// Remove a file association from the registry.
#[cfg(target_os = "windows")]
pub fn remove_file_association(extension: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id = format!("Parcel.{extension}");

    // Remove command subkey.
    let cmd_key = format!(r"Software\Classes\{prog_id}\shell\open\command");
    let _ = hkcu.delete_subkey_all(&cmd_key);

    let open_key = format!(r"Software\Classes\{prog_id}\shell\open");
    let _ = hkcu.delete_subkey_all(&open_key);

    let shell_key = format!(r"Software\Classes\{prog_id}\shell");
    let _ = hkcu.delete_subkey_all(&shell_key);

    // Remove ProgID key.
    let prog_key = format!(r"Software\Classes\{prog_id}");
    let _ = hkcu.delete_subkey_all(&prog_key);

    // Remove extension key only if it points to our ProgID.
    let ext_key_path = format!(r"Software\Classes\.{extension}");
    if let Ok(ext_key) = hkcu.open_subkey(&ext_key_path) {
        if let Ok(val) = ext_key.get_value::<String, _>("") {
            if val == prog_id {
                let _ = hkcu.delete_subkey_all(&ext_key_path);
            }
        }
    }

    log::info!("Removed file association .{extension}.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn remove_file_association(_extension: &str) -> Result<()> {
    Ok(())
}
