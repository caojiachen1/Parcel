//! Shared utilities for the Parcel CLI.

use std::path::{Path, PathBuf};

/// Locate the target executable for a Tauri project.
///
/// `cargo tauri build` runs cargo inside `src-tauri/`, so the build
/// artefacts end up in `src-tauri/target/`.  However, if the user has a
/// workspace-level `Cargo.toml` that includes `src-tauri` as a member,
/// the artefacts may instead appear in the root `target/` directory.
///
/// This function checks all plausible locations and returns the first
/// match, or `None` if nothing is found.
pub fn find_target_exe(cwd: &Path, exe_name: &str) -> Option<PathBuf> {
    let candidates = [
        // Most common: Tauri's own target directory.
        format!("src-tauri/target/release/{exe_name}.exe"),
        // Workspace-level target directory.
        format!("target/release/{exe_name}.exe"),
        // Debug builds (useful during development).
        format!("src-tauri/target/debug/{exe_name}.exe"),
        format!("target/debug/{exe_name}.exe"),
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if cwd.join(&path).exists() {
            return Some(path);
        }
    }

    None
}

/// Build the expected `target_exe` path for use in generated config,
/// even when the file doesn't exist yet (e.g. during `parcel init`
/// before the user has compiled).
///
/// Prefers `src-tauri/target/release/` because that's where
/// `cargo tauri build --no-bundle` puts the artefact in a standard
/// Tauri project layout.
pub fn default_target_exe(exe_name: &str) -> PathBuf {
    PathBuf::from(format!("src-tauri/target/release/{exe_name}.exe"))
}

/// Read binary and package names from a `Cargo.toml` file.
///
/// Returns `(explicit_bin_name, package_name)`.
pub fn read_cargo_names(path: &Path) -> (Option<String>, Option<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let mut package_name: Option<String> = None;
    let mut bin_name: Option<String> = None;
    let mut in_package = false;
    let mut in_bin = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[package]" {
            in_package = true;
            in_bin = false;
            continue;
        } else if trimmed == "[[bin]]" {
            in_bin = true;
            in_package = false;
            continue;
        } else if trimmed.starts_with('[') {
            in_package = false;
            in_bin = false;
            continue;
        }

        if (in_package || in_bin) && trimmed.starts_with("name") {
            if let Some(val) = extract_toml_string(trimmed) {
                if in_package && package_name.is_none() {
                    package_name = Some(val.clone());
                }
                if in_bin && bin_name.is_none() {
                    bin_name = Some(val);
                }
            }
        }
    }

    (bin_name, package_name)
}

/// Extract the string value from a `key = "value"` TOML line.
pub fn extract_toml_string(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    let value = parts[1].trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Some(value[1..value.len() - 1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_toml_string_works() {
        assert_eq!(
            extract_toml_string(r#"name = "my-app""#),
            Some("my-app".into())
        );
        assert_eq!(extract_toml_string("name = 123"), None);
        assert_eq!(extract_toml_string("no-equals"), None);
    }

    #[test]
    fn default_target_exe_uses_src_tauri_prefix() {
        let path = default_target_exe("myapp");
        assert_eq!(
            path.to_string_lossy(),
            "src-tauri/target/release/myapp.exe"
        );
    }
}
