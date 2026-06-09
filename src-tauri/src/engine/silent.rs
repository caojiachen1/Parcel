//! Silent (unattended) installation support.
//!
//! Supported command-line flags:
//!   /S                — fully silent, use all defaults.
//!   /DIR="path"       — override installation directory.
//!   /NO_SHORTCUT      — skip creating shortcuts.
//!   /NO_AUTOSTART     — skip auto-start registration.

use crate::commands::InstallOptions;
use crate::config;

/// Parsed silent-install arguments.
#[derive(Debug, Clone)]
pub struct SilentArgs {
    pub silent: bool,
    pub dir: Option<String>,
    pub no_shortcut: bool,
    pub no_autostart: bool,
}

/// Parse command-line arguments for silent installation mode.
///
/// Returns `None` if no silent-install flags are present.
pub fn parse_silent_args() -> Option<SilentArgs> {
    let args: Vec<String> = std::env::args().collect();

    let has_silent = args.iter().any(|a| a == "/S" || a == "/s");
    if !has_silent {
        return None;
    }

    let mut result = SilentArgs {
        silent: true,
        dir: None,
        no_shortcut: false,
        no_autostart: false,
    };

    for arg in &args {
        if arg.starts_with("/DIR=") || arg.starts_with("/dir=") {
            let value = arg.split_once('=').map(|(_, v)| v.trim_matches('"'));
            if let Some(dir) = value {
                result.dir = Some(dir.to_string());
            }
        }
        if arg == "/NO_SHORTCUT" || arg == "/no_shortcut" {
            result.no_shortcut = true;
        }
        if arg == "/NO_AUTOSTART" || arg == "/no_autostart" {
            result.no_autostart = true;
        }
    }

    Some(result)
}

/// Convert silent arguments into `InstallOptions`.
pub fn to_install_options(
    silent: &SilentArgs,
    payload: &config::PayloadConfig,
) -> InstallOptions {
    let default_dir = config::resolve_default_dir(payload)
        .to_string_lossy()
        .to_string();

    InstallOptions {
        install_dir: silent.dir.clone().unwrap_or(default_dir),
        desktop_shortcut: !silent.no_shortcut
            && payload.parcel.install.shortcuts.desktop_default,
        start_menu_shortcut: !silent.no_shortcut
            && payload.parcel.install.shortcuts.start_menu_default,
        auto_start: !silent.no_autostart && payload.parcel.install.auto_start.default_value,
    }
}
