//! Parcel configuration schema (`parcel.json`).
//!
//! Every field has sensible defaults so users only need to override what they
//! care about. The configuration is fully serialisable to/from JSON.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Root ────────────────────────────────────────────────────────────────

/// Top-level `parcel.json` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ParcelConfig {
    /// Application metadata.
    pub app: AppConfig,
    /// Input / output paths.
    pub paths: PathsConfig,
    /// Installation behaviour.
    pub install: InstallConfig,
    /// Visual appearance.
    pub appearance: AppearanceConfig,
    /// End-user licence agreement.
    pub eula: EulaConfig,
    /// Code-signing settings.
    pub signing: SigningConfig,
}

impl Default for ParcelConfig {
    fn default() -> Self {
        Self {
            app: AppConfig::default(),
            paths: PathsConfig::default(),
            install: InstallConfig::default(),
            appearance: AppearanceConfig::default(),
            eula: EulaConfig::default(),
            signing: SigningConfig::default(),
        }
    }
}

impl ParcelConfig {
    /// Load configuration from a JSON file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a JSON file (pretty-printed).
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

// ── App ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Display name shown in the installer UI.
    pub name: String,
    /// Semantic version string (e.g. "1.2.3").
    pub version: String,
    /// Unique reverse-domain identifier (e.g. "com.example.myapp").
    pub identifier: String,
    /// Publisher / company name.
    pub publisher: String,
    /// Publisher website URL.
    pub publisher_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "MyApp".into(),
            version: "0.1.0".into(),
            identifier: "com.example.myapp".into(),
            publisher: "My Company".into(),
            publisher_url: "https://example.com".into(),
        }
    }
}

// ── Paths ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    /// Path to the main executable (relative to project root).
    pub target_exe: PathBuf,
    /// Glob patterns for additional resource files to bundle.
    pub resources: Vec<String>,
    /// Path to the application icon (png or ico).
    pub icon: PathBuf,
    /// Output directory for the generated installer.
    pub output_dir: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            target_exe: PathBuf::from("src-tauri/target/release/app.exe"),
            resources: vec![],
            icon: PathBuf::from("icons/icon.png"),
            output_dir: PathBuf::from("dist"),
        }
    }
}

// ── Install ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallConfig {
    /// Default installation directory template.
    /// Supports placeholders: `{localappdata}`, `{programfiles}`, `{name}`.
    pub default_dir: String,
    /// Whether the user may change the install path.
    pub allow_custom_dir: bool,
    /// Whether the installer requires administrator privileges.
    pub require_admin: bool,
    /// Shortcut settings.
    pub shortcuts: ShortcutsConfig,
    /// File associations to register.
    pub file_associations: Vec<FileAssociation>,
    /// Auto-start settings.
    pub auto_start: AutoStartConfig,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            default_dir: r"{localappdata}\Programs\{name}".into(),
            allow_custom_dir: true,
            require_admin: false,
            shortcuts: ShortcutsConfig::default(),
            file_associations: vec![],
            auto_start: AutoStartConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutsConfig {
    /// Create a desktop shortcut by default.
    pub desktop_default: bool,
    /// Allow the user to opt out of the desktop shortcut.
    pub desktop_optional: bool,
    /// Create a Start Menu shortcut by default.
    pub start_menu_default: bool,
    /// Allow the user to opt out of the Start Menu shortcut.
    pub start_menu_optional: bool,
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            desktop_default: true,
            desktop_optional: true,
            start_menu_default: true,
            start_menu_optional: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FileAssociation {
    /// File extension without the dot (e.g. "myext").
    pub extension: String,
    /// Human-readable description.
    pub description: String,
    /// Path to the icon to use for this file type (optional).
    pub icon: Option<PathBuf>,
}

impl Default for FileAssociation {
    fn default() -> Self {
        Self {
            extension: String::new(),
            description: String::new(),
            icon: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoStartConfig {
    /// Show the auto-start option in the installer.
    pub enabled: bool,
    /// Whether auto-start is checked by default.
    pub default_value: bool,
}

impl Default for AutoStartConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_value: false,
        }
    }
}

// ── Appearance ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Base theme: "light" or "dark".
    pub theme: String,
    /// Colour palette.
    pub colors: ColorsConfig,
    /// Path to the application logo (png or svg).
    pub logo: PathBuf,
    /// Custom font family name (empty = system default).
    pub font_family: String,
    /// Optional background image for the welcome page.
    pub welcome_background: Option<PathBuf>,
    /// Optional background image for the finish page.
    pub finish_background: Option<PathBuf>,
    /// Window corner radius in pixels (0 = square).
    pub border_radius: u32,
    /// Animation style: "fade", "slide", "none".
    pub page_animation: String,
    /// All user-visible text strings.
    pub strings: StringsConfig,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            colors: ColorsConfig::default(),
            logo: PathBuf::from("parcel/assets/logo.png"),
            font_family: String::new(),
            welcome_background: None,
            finish_background: None,
            border_radius: 6,
            page_animation: "fade".into(),
            strings: StringsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    /// Primary brand colour (hex, e.g. "#3B82F6").
    pub primary: String,
    /// Accent colour for interactive elements.
    pub accent: String,
    /// Page background colour.
    pub background: String,
    /// Primary text colour.
    pub text: String,
    /// Secondary / muted text colour.
    pub text_secondary: String,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            primary: "#60CDFF".into(),
            accent: "#4DB8E8".into(),
            background: "#202020".into(),
            text: "#F5F5F5".into(),
            text_secondary: "#A0A0A0".into(),
        }
    }
}

// ── Strings ─────────────────────────────────────────────────────────────

/// All user-visible text in the installer.
/// Every string can be overridden via `parcel.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StringsConfig {
    // Welcome page
    pub welcome_title: String,
    pub welcome_subtitle: String,
    // EULA page
    pub eula_title: String,
    pub eula_scroll_hint: String,
    // Options page
    pub options_title: String,
    pub options_install_path: String,
    pub options_browse: String,
    pub options_desktop_shortcut: String,
    pub options_start_menu_shortcut: String,
    pub options_auto_start: String,
    // Progress page
    pub progress_title: String,
    pub progress_installing: String,
    pub progress_complete: String,
    // Finish page
    pub finish_title: String,
    pub finish_subtitle: String,
    pub finish_launch: String,
    // Buttons
    pub btn_next: String,
    pub btn_back: String,
    pub btn_install: String,
    pub btn_finish: String,
    pub btn_cancel: String,
    pub btn_agree: String,
    pub btn_disagree: String,
    // Dialogs
    pub cancel_confirm_title: String,
    pub cancel_confirm_message: String,
    pub overwrite_title: String,
    pub overwrite_message: String,
    pub error_title: String,
}

impl Default for StringsConfig {
    fn default() -> Self {
        Self {
            welcome_title: "Welcome to {name} Setup".into(),
            welcome_subtitle: "Version {version}".into(),
            eula_title: "License Agreement".into(),
            eula_scroll_hint: "Please scroll down to read the entire agreement.".into(),
            options_title: "Installation Options".into(),
            options_install_path: "Install Location".into(),
            options_browse: "Browse…".into(),
            options_desktop_shortcut: "Create desktop shortcut".into(),
            options_start_menu_shortcut: "Create Start Menu shortcut".into(),
            options_auto_start: "Launch at system startup".into(),
            progress_title: "Installing…".into(),
            progress_installing: "Copying files…".into(),
            progress_complete: "Installation complete!".into(),
            finish_title: "Installation Successful".into(),
            finish_subtitle: "{name} has been installed on your computer.".into(),
            finish_launch: "Launch {name}".into(),
            btn_next: "Next".into(),
            btn_back: "Back".into(),
            btn_install: "Install".into(),
            btn_finish: "Finish".into(),
            btn_cancel: "Cancel".into(),
            btn_agree: "I Agree".into(),
            btn_disagree: "I Disagree".into(),
            cancel_confirm_title: "Cancel Installation".into(),
            cancel_confirm_message: "Are you sure you want to cancel the installation?".into(),
            overwrite_title: "Existing Installation Detected".into(),
            overwrite_message: "A previous installation was found at {path}. Do you want to overwrite it?".into(),
            error_title: "Installation Error".into(),
        }
    }
}

// ── EULA ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EulaConfig {
    /// Path to the EULA file (plain text or HTML).
    pub file: Option<PathBuf>,
}

impl Default for EulaConfig {
    fn default() -> Self {
        Self { file: None }
    }
}

// ── Signing ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SigningConfig {
    /// Whether to sign the generated installer.
    pub enabled: bool,
    /// Path to the code-signing certificate (.pfx).
    pub certificate: Option<PathBuf>,
    /// Certificate password (prefer env vars instead).
    pub password: Option<String>,
}

impl Default for SigningConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            certificate: None,
            password: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrip() {
        let config = ParcelConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: ParcelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.app.name, "MyApp");
        assert_eq!(parsed.appearance.colors.primary, "#60CDFF");
    }

    #[test]
    fn partial_override() {
        let json = r#"{ "app": { "name": "TestApp" } }"#;
        // Partial deserialization should fail because serde(default)
        // is on the struct level, not individual fields of AppConfig.
        // But the top-level ParcelConfig uses serde(default).
        let config: ParcelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.app.name, "TestApp");
    }
}
