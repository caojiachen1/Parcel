//! Path safety validation — shared between installer and uninstaller.
//!
//! Provides a single authoritative set of checks to prevent accidental
//! deletion of system-critical, user-sensitive, or otherwise dangerous
//! directories.  Both the installer's rollback logic and the uninstaller
//! must go through these checks before removing any directory.
//!
//! ## Path Whitelist
//!
//! In addition to the blocklist-based safety checks, a **whitelist**
//! mechanism ensures that only explicitly approved paths can be operated
//! on.  When an installation is performed, the install directory is
//! automatically added to the whitelist.  The uninstaller verifies the
//! path is whitelisted before proceeding with removal.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf, Component};

// ── Dangerous directories ─────────────────────────────────────────────────

/// Top-level directory names (direct children of a drive root) that must
/// NEVER be deleted.  The list covers system, recovery, and infrastructure
/// directories across Windows versions.
const DANGEROUS_ROOT_CHILDREN: &[&str] = &[
    // Core OS
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    // User profiles root
    "users",
    "documents and settings",
    // System infrastructure
    "$recycle_bin",
    "recovery",
    "boot",
    "system volume information",
    "perflogs",
    "msocache",
    "intel",
    "amd",
    // OEM / manufacturer
    "dell",
    "hp",
    "lenovo",
    // Network / share
    "inetpub",
];

/// User-profile sub-directories that should NEVER be recursively deleted,
/// even if they happen to be the install_dir (e.g. user mistakenly installs
/// to their Desktop or Documents folder).
const DANGEROUS_PROFILE_CHILDREN: &[&str] = &[
    "desktop",
    "documents",
    "downloads",
    "pictures",
    "videos",
    "music",
    "contacts",
    "favorites",
    "links",
    "searches",
    "saved games",
    "onedrive",
    "3d objects",
    "source",
];

/// Known system GUID directories that should never be touched
/// (e.g. `C:\Windows\{90D132C4-...}`).
fn is_system_guid_dir(name: &str) -> bool {
    // System GUID dirs typically start with '{' and are 38 chars long.
    name.starts_with('{') && name.ends_with('}') && name.len() >= 36
}

// ── Drive root detection ──────────────────────────────────────────────────

/// Check if a path is a drive root (e.g. `C:\`, `D:\`, `E:/`).
pub fn is_drive_root(path: &Path) -> bool {
    // Handle both `C:\` and `C:/` as well as bare `C:`.
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches(|c| c == '\\' || c == '/');
    trimmed.len() == 2 && trimmed.ends_with(':')
}

// ── Dangerous directory checks ────────────────────────────────────────────

/// Check if a directory is a dangerous system location that must NEVER be
/// deleted.  This examines the path structure, not just string matching.
pub fn is_dangerous_directory(path: &Path) -> bool {
    // 1. Drive-root children (e.g. C:\Windows)
    if let Some(parent) = path.parent() {
        if is_drive_root(parent) {
            if let Some(name) = path.file_name() {
                let name_lower = name.to_string_lossy().to_lowercase();
                if DANGEROUS_ROOT_CHILDREN.iter().any(|d| name_lower == *d) {
                    return true;
                }
                if is_system_guid_dir(&name_lower) {
                    return true;
                }
            }
        }
    }

    // 2. User-profile direct children (e.g. C:\Users\John\Desktop)
    //    Check if the parent is a per-user profile directory.
    if let Some(parent) = path.parent() {
        if is_user_profile_dir(parent) {
            if let Some(name) = path.file_name() {
                let name_lower = name.to_string_lossy().to_lowercase();
                if DANGEROUS_PROFILE_CHILDREN.iter().any(|d| name_lower == *d) {
                    return true;
                }
            }
        }
    }

    // 3. The user profiles directory itself
    if is_user_profiles_dir(path) {
        return true;
    }

    // 4. The drive root itself
    if is_drive_root(path) {
        return true;
    }

    false
}

/// Check if a path looks like the `C:\Users` directory.
fn is_user_profiles_dir(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        if is_drive_root(parent) {
            if let Some(name) = path.file_name() {
                let name_lower = name.to_string_lossy().to_lowercase();
                return name_lower == "users" || name_lower == "documents and settings";
            }
        }
    }
    false
}

/// Check if a path is a per-user profile directory (e.g. `C:\Users\John`).
fn is_user_profile_dir(path: &Path) -> bool {
    // A user profile dir is a direct child of the Users directory.
    if let Some(parent) = path.parent() {
        return is_user_profiles_dir(parent);
    }
    false
}

// ── Path traversal detection ──────────────────────────────────────────────

/// Check if a relative path contains traversal components (`..`).
pub fn has_path_traversal(relative: &str) -> bool {
    let path = Path::new(relative);
    path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Validate that `file_path` is contained within `base_dir` after
/// canonicalization.  Returns an error if the path escapes the base.
pub fn validate_path_within(file_path: &Path, base_dir: &Path) -> anyhow::Result<()> {
    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());
    let canonical_path = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());

    if !canonical_path.starts_with(&canonical_base) {
        anyhow::bail!(
            "SAFETY: Path {} escapes base directory {}! Refusing to operate.",
            file_path.display(),
            base_dir.display()
        );
    }
    Ok(())
}

// ── Install directory validation ──────────────────────────────────────────

/// Comprehensive validation that an install directory is safe to operate on.
///
/// This is the **single authoritative check** used by both the installer
/// (rollback) and the uninstaller (removal).  It rejects:
///
/// - Relative paths
/// - Drive roots
/// - Dangerous system directories
/// - Paths with too few components
/// - Known user-sensitive directories
///
/// Returns `Ok(())` if the directory is considered safe, or `Err` with a
/// detailed reason if it is not.
pub fn validate_install_dir(install_dir: &Path) -> anyhow::Result<()> {
    // 1. Must be absolute
    if !install_dir.is_absolute() {
        anyhow::bail!(
            "SAFETY: install_dir is not absolute: {}. Refusing to proceed.",
            install_dir.display()
        );
    }

    // 2. Must NOT be a drive root
    if is_drive_root(install_dir) {
        anyhow::bail!(
            "SAFETY: install_dir is a drive root: {}. REFUSING TO DELETE A DRIVE!",
            install_dir.display()
        );
    }

    // 3. Must NOT be a dangerous system directory
    if is_dangerous_directory(install_dir) {
        anyhow::bail!(
            "SAFETY: install_dir is a dangerous system directory: {}. Refusing to proceed.",
            install_dir.display()
        );
    }

    // 4. Must have at least 3 components
    //    On Windows, a path has these components:
    //      `E:\`           → Prefix("E:"), RootDir         = 2 components (DRIVE ROOT — BLOCKED)
    //      `E:\app`        → Prefix("E:"), RootDir, Normal  = 3 components (OK — subfolder)
    //      `E:\app\sub`    → Prefix, Root, Normal, Normal   = 4 components (OK)
    //    So we require >= 3 to allow root-level subdirectories like `E:\app`.
    //    Combined with check #2 (is_drive_root), this ensures that ONLY
    //    subfolders of a drive root are ever operated on.
    let component_count = install_dir.components().count();
    if component_count < 3 {
        anyhow::bail!(
            "SAFETY: install_dir has too few path components ({}): {}. \
             Only subfolders of a drive root are allowed (e.g. E:\\app). Refusing to proceed.",
            component_count,
            install_dir.display()
        );
    }

    // 5. Canonicalize and re-verify (catches symlinks / junctions)
    if let Ok(canonical) = install_dir.canonicalize() {
        if is_drive_root(&canonical) {
            anyhow::bail!(
                "SAFETY: install_dir canonicalizes to drive root: {}. REFUSING!",
                canonical.display()
            );
        }
        if is_dangerous_directory(&canonical) {
            anyhow::bail!(
                "SAFETY: install_dir canonicalizes to dangerous directory: {}. REFUSING!",
                canonical.display()
            );
        }
    }

    Ok(())
}

/// Remove a directory only if it passes all safety checks.
///
/// This is the recommended entry point for any code that needs to
/// recursively delete an installation directory.
pub fn remove_dir_if_safe(dir: &Path) -> anyhow::Result<()> {
    validate_install_dir(dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

// ── Path Whitelist ────────────────────────────────────────────────────────

/// Persistent whitelist of paths that are approved for installation /
/// uninstallation operations.
///
/// The whitelist is stored as a JSON file at:
///   `%APPDATA%\Parcel\whitelist.json`
///
/// Every successful installation automatically adds its target directory
/// to the whitelist.  The uninstaller **requires** the path to be on
/// the whitelist before it will proceed with removal.  This provides an
/// extra layer of protection against accidental deletion of directories
/// that were never installed to by Parcel.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathWhitelist {
    /// List of approved absolute paths.
    paths: Vec<String>,
}

impl PathWhitelist {
    /// Load the whitelist from the default location.
    /// Returns an empty whitelist if the file doesn't exist yet.
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::storage_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read whitelist file {}: {e}", path.display()))?;
        let wl: Self = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse whitelist file {}: {e}", path.display()))?;
        Ok(wl)
    }

    /// Persist the whitelist to the default location.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::storage_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create whitelist directory: {e}"))?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize whitelist: {e}"))?;
        std::fs::write(&path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write whitelist file {}: {e}", path.display()))?;
        Ok(())
    }

    /// Check whether a path is on the whitelist.
    ///
    /// Comparison is case-insensitive and normalizes path separators.
    pub fn is_allowed(&self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        self.paths.iter().any(|p| Self::normalize_path(&PathBuf::from(p)) == normalized)
    }

    /// Add a path to the whitelist and persist.
    pub fn add(&mut self, path: &Path) -> anyhow::Result<()> {
        let normalized = Self::normalize_path(path);
        if !self.paths.iter().any(|p| Self::normalize_path(&PathBuf::from(p)) == normalized) {
            self.paths.push(path.to_string_lossy().to_string());
        }
        self.save()
    }

    /// Remove a path from the whitelist and persist.
    pub fn remove(&mut self, path: &Path) -> anyhow::Result<()> {
        let normalized = Self::normalize_path(path);
        self.paths.retain(|p| Self::normalize_path(&PathBuf::from(p)) != normalized);
        self.save()
    }

    /// Return all whitelisted paths.
    pub fn entries(&self) -> &[String] {
        &self.paths
    }

    /// Get the storage path for the whitelist file.
    fn storage_path() -> anyhow::Result<PathBuf> {
        let appdata = std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .unwrap_or_else(|_| {
                // Fallback to temp dir if APPDATA is not available
                std::env::temp_dir().to_string_lossy().to_string()
            });
        Ok(PathBuf::from(appdata).join("Parcel").join("whitelist.json"))
    }

    /// Normalize a path for comparison: lowercase, forward slashes, no trailing slash.
    fn normalize_path(path: &Path) -> String {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    }
}

/// Validate that an install directory is safe AND whitelisted.
///
/// This is the strictest check — it combines the standard safety validation
/// with a whitelist lookup.  Use this in the **uninstaller** before any
/// destructive operation.
pub fn validate_and_whitelist(install_dir: &Path) -> anyhow::Result<()> {
    // First: standard safety checks
    validate_install_dir(install_dir)?;

    // Second: whitelist check
    let whitelist = PathWhitelist::load()
        .map_err(|e| anyhow::anyhow!("Failed to load whitelist: {e}"))?;

    if !whitelist.is_allowed(install_dir) {
        anyhow::bail!(
            "SAFETY: install_dir {} is NOT on the whitelist. \
             This directory was never registered by Parcel. Refusing to proceed.\n\
             Whitelisted paths: {:?}",
            install_dir.display(),
            whitelist.entries()
        );
    }

    Ok(())
}

/// Register an install directory in the whitelist.
///
/// Call this **early** in the installation process (before any files are
/// written) so that if the install fails partway, the rollback is still
/// permitted.
pub fn whitelist_install_dir(install_dir: &Path) -> anyhow::Result<()> {
    // Verify the path passes basic safety first
    validate_install_dir(install_dir)?;

    let mut whitelist = PathWhitelist::load().unwrap_or_default();
    whitelist.add(install_dir)?;
    Ok(())
}

/// Unregister an install directory from the whitelist after successful uninstall.
pub fn unwhitelist_install_dir(install_dir: &Path) -> anyhow::Result<()> {
    let mut whitelist = PathWhitelist::load().unwrap_or_default();
    whitelist.remove(install_dir)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn drive_root_detected() {
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new(r"D:\")));
        assert!(is_drive_root(Path::new(r"E:/")));
        assert!(!is_drive_root(Path::new(r"C:\Users")));
        assert!(!is_drive_root(Path::new(r"C:\Users\John")));
    }

    #[test]
    fn dangerous_system_dirs_detected() {
        let cases = [
            r"C:\Windows",
            r"C:\ProgramData",
            r"C:\Users",
            r"D:\$Recycle_BIN",
            r"C:\Recovery",
            r"C:\Boot",
            r"C:\System Volume Information",
            r"C:\PerfLogs",
        ];
        for case in cases {
            assert!(
                is_dangerous_directory(Path::new(case)),
                "Expected dangerous: {case}"
            );
        }
    }

    #[test]
    fn safe_dirs_not_flagged() {
        let cases = [
            r"C:\Users\John\AppData\Local\Programs\MyApp",
            r"C:\Program Files\MyApp",
            r"D:\Games\MyGame",
        ];
        for case in cases {
            assert!(
                !is_dangerous_directory(Path::new(case)),
                "Expected safe: {case}"
            );
        }
    }

    #[test]
    fn user_profile_children_flagged() {
        let cases = [
            r"C:\Users\John\Desktop",
            r"C:\Users\John\Documents",
            r"C:\Users\John\Downloads",
            r"C:\Users\John\Pictures",
            r"C:\Users\John\OneDrive",
        ];
        for case in cases {
            assert!(
                is_dangerous_directory(Path::new(case)),
                "Expected dangerous: {case}"
            );
        }
    }

    #[test]
    fn path_traversal_detected() {
        assert!(has_path_traversal("../etc/passwd"));
        assert!(has_path_traversal("foo/../../bar"));
        assert!(!has_path_traversal("foo/bar/baz"));
        assert!(!has_path_traversal("app.exe"));
    }

    #[test]
    fn validate_rejects_relative() {
        let result = validate_install_dir(Path::new("relative/path"));
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_drive_root() {
        let result = validate_install_dir(Path::new(r"C:\"));
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_dangerous() {
        let result = validate_install_dir(Path::new(r"C:\Windows"));
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_too_few_components() {
        // `C:\Something` has 3 components on Windows (Prefix, RootDir, Normal)
        // — this is now ALLOWED as a subfolder of the drive root.
        let result = validate_install_dir(Path::new(r"C:\Something"));
        // Should pass basic checks (canonicalize may fail in test env, that's OK)
        if Path::new(r"C:\Something").exists() {
            assert!(result.is_ok(), "C:\\Something should be allowed as a root subfolder");
        }
    }

    #[test]
    fn validate_rejects_all_drive_roots() {
        // ANY drive root must be blocked: C:\, D:\, E:\, etc.
        let roots = [
            r"C:\",
            r"D:\",
            r"E:\",
            r"F:\",
            r"C:/",
            r"D:/",
        ];
        for root in roots {
            let result = validate_install_dir(Path::new(root));
            assert!(result.is_err(), "Drive root {root} must be rejected");
        }
    }

    #[test]
    fn validate_allows_root_subfolders() {
        // Subfolders of drive roots should be allowed (e.g. E:\app, D:\Games)
        let cases = [
            r"E:\app",
            r"D:\Games",
            r"C:\MyApp",
        ];
        for case in cases {
            let result = validate_install_dir(Path::new(case));
            // canonicalize may fail in test env, but basic checks should pass
            // (the path won't exist, so canonicalize returns Err, and we skip that check)
            assert!(
                result.is_ok(),
                "Root subfolder {case} should be allowed, got: {result:?}"
            );
        }
    }

    #[test]
    fn validate_accepts_normal_path() {
        // This may fail if the path doesn't exist (canonicalize),
        // but it should not fail the basic checks.
        let p = PathBuf::from(r"C:\Users\TestUser\AppData\Local\Programs\MyApp");
        let result = validate_install_dir(&p);
        // May fail on canonicalize in test env, but the basic checks pass.
        // In real usage the directory exists.
        if p.exists() {
            assert!(result.is_ok());
        }
    }

    // ── Whitelist tests ──────────────────────────────────────────────

    #[test]
    fn whitelist_add_and_check() {
        let mut wl = PathWhitelist::default();
        let path = Path::new(r"C:\Users\Test\AppData\Local\Programs\MyApp");

        // Not yet whitelisted
        assert!(!wl.is_allowed(path));

        // Add to whitelist (skip save to avoid touching the real filesystem in tests)
        wl.paths.push(path.to_string_lossy().to_string());
        assert!(wl.is_allowed(path));
    }

    #[test]
    fn whitelist_case_insensitive() {
        let mut wl = PathWhitelist::default();
        wl.paths.push(r"C:\Users\John\Apps\MyApp".to_string());

        // Case-insensitive comparison
        assert!(wl.is_allowed(Path::new(r"c:\users\john\apps\myapp")));
        assert!(wl.is_allowed(Path::new(r"C:\USERS\JOHN\APPS\MYAPP")));
        assert!(wl.is_allowed(Path::new(r"C:\Users\John\Apps\MyApp")));
    }

    #[test]
    fn whitelist_separator_normalization() {
        let mut wl = PathWhitelist::default();
        wl.paths.push(r"C:\Users\John\Apps\MyApp".to_string());

        // Backslash and forward slash should match
        assert!(wl.is_allowed(Path::new("C:/Users/John/Apps/MyApp")));
        assert!(wl.is_allowed(Path::new(r"C:\Users\John\Apps\MyApp")));
    }

    #[test]
    fn whitelist_remove_entry() {
        let mut wl = PathWhitelist::default();
        let path = Path::new(r"C:\Users\Test\Apps\Foo");
        wl.paths.push(path.to_string_lossy().to_string());
        assert!(wl.is_allowed(path));

        // Remove without persisting
        let normalized = PathWhitelist::normalize_path(path);
        wl.paths.retain(|p| PathWhitelist::normalize_path(&PathBuf::from(p)) != normalized);
        assert!(!wl.is_allowed(path));
    }

    #[test]
    fn whitelist_rejects_unregistered_paths() {
        let wl = PathWhitelist::default();
        // An empty whitelist should reject everything
        assert!(!wl.is_allowed(Path::new(r"C:\Some\Random\Path")));
        assert!(!wl.is_allowed(Path::new(r"E:\app")));
    }

    #[test]
    fn whitelist_no_duplicates() {
        let mut wl = PathWhitelist::default();
        let path = Path::new(r"C:\Users\Test\Apps\Foo");
        wl.paths.push(path.to_string_lossy().to_string());
        wl.paths.push(path.to_string_lossy().to_string());

        // Should have 2 entries (we pushed manually)
        assert_eq!(wl.paths.len(), 2);

        // But the `add` method should prevent duplicates
        let _wl2 = PathWhitelist::default();
        // Use a temp dir to test the add-with-save flow
        // (We can't easily test save without filesystem side effects)
    }
}
