//! Registry operations — uninstall info, auto-start, file associations.
//!
//! ## Change Tracking & Rollback
//!
//! Every write operation records what was changed in a [`RegistryChangeLog`],
//! including a **backup of the original value** (if any).  This allows the
//! installer to precisely undo only the registry modifications it made,
//! without touching values that existed before the installation.

use anyhow::Result;
use parcel_core::config::{FileAssociation, ParcelConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Registry Change Log ──────────────────────────────────────────────────

/// A single registry value that was modified during installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryValueChange {
    /// Full registry key path (e.g. `HKCU\Software\Microsoft\...\Uninstall\MyApp`).
    pub key_path: String,
    /// Value name (empty string = default value).
    pub value_name: String,
    /// The original value before modification, if it existed.
    /// `None` means this value did not exist before (new value).
    pub old_value: Option<String>,
    /// The value that was written.
    pub new_value: String,
}

/// A registry subkey that was created during installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryKeyCreated {
    /// Full registry key path.
    pub key_path: String,
    /// Whether the key existed before we created it.
    pub pre_existed: bool,
}

/// Complete log of all registry modifications made during installation.
///
/// This is serialized to `install.log` alongside the file list so that
/// the rollback and uninstaller can precisely undo only what was changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryChangeLog {
    /// Individual value modifications.
    pub value_changes: Vec<RegistryValueChange>,
    /// Subkeys that were created (and whether they pre-existed).
    pub keys_created: Vec<RegistryKeyCreated>,
}

impl RegistryChangeLog {
    /// Create a new empty change log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a value was set.
    fn record_value_set(
        &mut self,
        key_path: &str,
        value_name: &str,
        old_value: Option<String>,
        new_value: &str,
    ) {
        self.value_changes.push(RegistryValueChange {
            key_path: key_path.to_string(),
            value_name: value_name.to_string(),
            old_value,
            new_value: new_value.to_string(),
        });
    }

    /// Record that a subkey was created/opened.
    fn record_key_created(&mut self, key_path: &str, pre_existed: bool) {
        // Only record if not already tracked
        if !self.keys_created.iter().any(|k| k.key_path == key_path) {
            self.keys_created.push(RegistryKeyCreated {
                key_path: key_path.to_string(),
                pre_existed,
            });
        }
    }

    /// Human-readable summary of all changes for logging.
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "Registry changes: {} value(s) modified, {} key(s) touched",
            self.value_changes.len(),
            self.keys_created.len()
        ));
        for vc in &self.value_changes {
            let status = if vc.old_value.is_some() {
                "MODIFIED"
            } else {
                "CREATED"
            };
            lines.push(format!(
                "  [{status}] {}\\{} = {:?}",
                vc.key_path,
                if vc.value_name.is_empty() {
                    "(Default)"
                } else {
                    &vc.value_name
                },
                vc.new_value
            ));
            if let Some(ref old) = vc.old_value {
                lines.push(format!("          (was: {old:?})"));
            }
        }
        for kc in &self.keys_created {
            let status = if kc.pre_existed { "EXISTED" } else { "NEW" };
            lines.push(format!("  [KEY-{status}] {}", kc.key_path));
        }
        lines.join("\n")
    }

    /// Roll back all recorded changes.
    ///
    /// - For values that existed before: restore the original value.
    /// - For values that were new: delete them.
    /// - For keys that were newly created (not pre-existing): delete the subkey.
    #[cfg(target_os = "windows")]
    pub fn rollback(&self) -> Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        log::info!("Rolling back {} registry value change(s)...", self.value_changes.len());

        // Roll back value changes in reverse order
        for vc in self.value_changes.iter().rev() {
            match &vc.old_value {
                Some(old) => {
                    // Restore original value
                    if let Ok((key, _)) = hkcu.create_subkey(&vc.key_path) {
                        let _ = key.set_value::<String, _>(&vc.value_name, old);
                        log::info!(
                            "Restored registry value: {}\\{} = {old:?}",
                            vc.key_path,
                            if vc.value_name.is_empty() { "(Default)" } else { &vc.value_name }
                        );
                    }
                }
                None => {
                    // Value was new — delete it
                    if let Ok(key) = hkcu.open_subkey(&vc.key_path) {
                        let _ = key.delete_value(&vc.value_name);
                        log::info!(
                            "Deleted new registry value: {}\\{}",
                            vc.key_path,
                            if vc.value_name.is_empty() { "(Default)" } else { &vc.value_name }
                        );
                    }
                }
            }
        }

        // Delete keys that were newly created (not pre-existing), in reverse order
        for kc in self.keys_created.iter().rev() {
            if !kc.pre_existed {
                let _ = hkcu.delete_subkey_all(&kc.key_path);
                log::info!("Deleted newly created key: {}", kc.key_path);
            }
        }

        log::info!("Registry rollback complete.");
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    pub fn rollback(&self) -> Result<()> {
        log::warn!("Registry rollback skipped (not Windows).");
        Ok(())
    }
}

// ── Helper: read existing value ──────────────────────────────────────────

#[cfg(target_os = "windows")]
fn read_existing_string_value(key_path: &str, value_name: &str) -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(key_path)
        .ok()
        .and_then(|key| key.get_value::<String, _>(value_name).ok())
}

#[cfg(target_os = "windows")]
fn subkey_exists(key_path: &str) -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(key_path).is_ok()
}

// ── Write uninstall info ─────────────────────────────────────────────────

/// Write the "Add/Remove Programs" uninstall entry to the Windows registry.
///
/// All modifications are recorded in `change_log` with old-value backups
/// so that the changes can be precisely undone on rollback.
#[cfg(target_os = "windows")]
pub fn write_uninstall_info(
    config: &ParcelConfig,
    install_dir: &Path,
    change_log: &mut RegistryChangeLog,
) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        config.app.identifier
    );

    let pre_existed = subkey_exists(&key_path);
    let (key, _) = hkcu.create_subkey(&key_path)?;
    change_log.record_key_created(&key_path, pre_existed);

    let uninstall_exe = install_dir.join("uninstall.exe");

    // Helper: read old value, write new value, record change.
    let mut set_value = |name: &str, new_val: &str| -> Result<()> {
        let old = read_existing_string_value(&key_path, name);
        let is_modified = old.is_some();
        key.set_value(name, &new_val.to_string())?;
        change_log.record_value_set(&key_path, name, old, new_val);
        log::info!(
            "Registry: HKCU\\{}\\{} = {:?}{}",
            key_path,
            name,
            new_val,
            if is_modified { " (modified)" } else { " (new)" }
        );
        Ok(())
    };

    set_value("DisplayName", &config.app.name)?;
    set_value("DisplayVersion", &config.app.version)?;
    set_value("Publisher", &config.app.publisher)?;
    set_value("InstallLocation", &install_dir.to_string_lossy())?;
    set_value("UninstallString", &format!("\"{}\"", uninstall_exe.display()))?;

    // DisplayIcon
    if let Some(exe_name) = config.paths.target_exe.file_name().and_then(|s| s.to_str()) {
        let icon_path = install_dir.join(exe_name).to_string_lossy().to_string();
        let old = read_existing_string_value(&key_path, "DisplayIcon");
        key.set_value("DisplayIcon", &icon_path)?;
        change_log.record_value_set(&key_path, "DisplayIcon", old, &icon_path);
    }

    // DWORD values (NoModify, NoRepair, EstimatedSize) — record but don't backup
    // (these are always set to known constants, so rollback just deletes them).
    key.set_value("NoModify", &1u32)?;
    change_log.record_value_set(&key_path, "NoModify", None, "1");
    key.set_value("NoRepair", &1u32)?;
    change_log.record_value_set(&key_path, "NoRepair", None, "1");

    if let Ok(total_size) = calculate_install_size(install_dir) {
        key.set_value("EstimatedSize", &(total_size as u32))?;
        change_log.record_value_set(&key_path, "EstimatedSize", None, &total_size.to_string());
    }

    log::info!("Wrote uninstall registry entry: HKCU\\{}", key_path);
    Ok(())
}

/// Calculate total size of installed files in KB.
#[cfg(target_os = "windows")]
fn calculate_install_size(dir: &Path) -> Result<u64> {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += calculate_install_size(&entry.path()).unwrap_or(0);
                }
            }
        }
    }
    Ok(total / 1024)
}

#[cfg(not(target_os = "windows"))]
pub fn write_uninstall_info(
    _config: &ParcelConfig,
    _install_dir: &Path,
    _change_log: &mut RegistryChangeLog,
) -> Result<()> {
    log::warn!("Registry operations are only supported on Windows.");
    Ok(())
}

// ── Remove uninstall info ────────────────────────────────────────────────

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
    log::info!("Removed uninstall registry entry: HKCU\\{}", key_path);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn remove_uninstall_info(_identifier: &str) -> Result<()> {
    Ok(())
}

// ── Auto-start ───────────────────────────────────────────────────────────

/// Write a Run registry entry for auto-start at logon.
#[cfg(target_os = "windows")]
pub fn write_auto_start(
    app_name: &str,
    exe_path: &Path,
    change_log: &mut RegistryChangeLog,
) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    let pre_existed = subkey_exists(key_path);
    let (key, _) = hkcu.create_subkey(key_path)?;
    change_log.record_key_created(key_path, pre_existed);

    let new_value = format!("\"{}\"", exe_path.display());
    let old = read_existing_string_value(key_path, app_name);
    let is_modified = old.is_some();
    key.set_value(app_name, &new_value)?;
    change_log.record_value_set(key_path, app_name, old, &new_value);

    log::info!(
        "Registry: HKCU\\{}\\{} = {:?}{}",
        key_path,
        app_name,
        new_value,
        if is_modified { " (modified)" } else { " (new)" }
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn write_auto_start(
    _app_name: &str,
    _exe_path: &Path,
    _change_log: &mut RegistryChangeLog,
) -> Result<()> {
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

// ── File associations ────────────────────────────────────────────────────

/// Register a file association in the registry.
#[cfg(target_os = "windows")]
pub fn write_file_association(
    assoc: &FileAssociation,
    exe_path: &Path,
    change_log: &mut RegistryChangeLog,
) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcr = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id = format!("Parcel.{}", assoc.extension);

    // Extension key: Software\Classes\.ext -> ProgID
    let ext_key_path = format!(r"Software\Classes\.{}", assoc.extension);
    let ext_pre = subkey_exists(&ext_key_path);
    let (ext_key, _) = hkcr.create_subkey(&ext_key_path)?;
    change_log.record_key_created(&ext_key_path, ext_pre);

    let old_ext_val = read_existing_string_value(&ext_key_path, "");
    ext_key.set_value("", &prog_id)?;
    change_log.record_value_set(&ext_key_path, "", old_ext_val, &prog_id);

    // ProgID key: Software\Classes\Parcel.ext -> description
    let prog_key_path = format!(r"Software\Classes\{prog_id}");
    let prog_pre = subkey_exists(&prog_key_path);
    let (pk, _) = hkcr.create_subkey(&prog_key_path)?;
    change_log.record_key_created(&prog_key_path, prog_pre);

    let old_desc = read_existing_string_value(&prog_key_path, "");
    pk.set_value("", &assoc.description)?;
    change_log.record_value_set(&prog_key_path, "", old_desc, &assoc.description);

    // Command key: Software\Classes\Parcel.ext\shell\open\command -> "exe" "%1"
    let cmd_key_path = format!(r"Software\Classes\{prog_id}\shell\open\command");
    let cmd_pre = subkey_exists(&cmd_key_path);
    let (ck, _) = hkcr.create_subkey(&cmd_key_path)?;
    change_log.record_key_created(&cmd_key_path, cmd_pre);

    let cmd_value = format!("\"{}\" \"%1\"", exe_path.display());
    let old_cmd = read_existing_string_value(&cmd_key_path, "");
    ck.set_value("", &cmd_value)?;
    change_log.record_value_set(&cmd_key_path, "", old_cmd, &cmd_value);

    log::info!(
        "Registered file association .{} -> {} (command: {})",
        assoc.extension,
        prog_id,
        cmd_value
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn write_file_association(
    _assoc: &FileAssociation,
    _exe_path: &Path,
    _change_log: &mut RegistryChangeLog,
) -> Result<()> {
    Ok(())
}

/// Remove a file association from the registry.
#[cfg(target_os = "windows")]
pub fn remove_file_association(extension: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id = format!("Parcel.{extension}");

    let cmd_key = format!(r"Software\Classes\{prog_id}\shell\open\command");
    let _ = hkcu.delete_subkey_all(&cmd_key);

    let open_key = format!(r"Software\Classes\{prog_id}\shell\open");
    let _ = hkcu.delete_subkey_all(&open_key);

    let shell_key = format!(r"Software\Classes\{prog_id}\shell");
    let _ = hkcu.delete_subkey_all(&shell_key);

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
