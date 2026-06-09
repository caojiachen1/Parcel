//! VC++ redistributable detection and installation.

use anyhow::Result;

/// Check whether the Visual C++ redistributable (2015-2022) is installed.
#[cfg(target_os = "windows")]
pub fn is_installed() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Check for the 2015-2022 unified redistributable.
    let paths = [
        r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64",
        r"SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\X64",
    ];

    for path in &paths {
        if let Ok(key) = hklm.open_subkey(path) {
            if let Ok(version) = key.get_value::<u32, _>("Major") {
                if version >= 14 {
                    log::info!("VC++ redistributable detected (Major={version}).");
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(not(target_os = "windows"))]
pub fn is_installed() -> bool {
    true // Not applicable on non-Windows platforms.
}

/// Download and silently install the VC++ redistributable.
///
/// In a production implementation the installer binary would be embedded
/// in the payload. For now this is a placeholder.
#[cfg(target_os = "windows")]
pub fn install() -> Result<()> {
    log::info!("VC++ redistributable installation is a placeholder.");
    log::info!(
        "In production, the installer would embed vc_redist.x64.exe and run it silently."
    );
    // TODO: Embed vc_redist.x64.exe and run:
    //   Command::new("vc_redist.x64.exe")
    //       .args(["/install", "/quiet", "/norestart"])
    //       .status()
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn install() -> Result<()> {
    Ok(())
}
