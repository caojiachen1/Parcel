//! File installation — copy payload files to the target directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Install a single file into the target directory.
///
/// In a full implementation the payload files are read from an embedded
/// archive (or from the overlay appended to the installer EXE). For now
/// this is a placeholder that creates an empty file at the destination.
///
/// Returns the full path of the installed file.
pub fn install_file(relative_path: &str, install_dir: &Path) -> Result<PathBuf> {
    let dest = install_dir.join(relative_path);

    // Ensure parent directories exist.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // In production, the file content would be read from the embedded
    // payload archive. Here we create a placeholder.
    //
    // TODO: Replace with actual payload extraction logic.
    //
    // For now, try to copy from a well-known source directory if it exists,
    // otherwise write an empty placeholder.
    let source = PathBuf::from(relative_path);
    if source.exists() {
        std::fs::copy(&source, &dest)
            .with_context(|| format!("Failed to copy {} -> {}", source.display(), dest.display()))?;
    } else {
        // Create placeholder file
        std::fs::write(&dest, b"")
            .with_context(|| format!("Failed to create placeholder: {}", dest.display()))?;
    }

    Ok(dest)
}

/// Remove all installed files for rollback.
///
/// Uses the shared safety module from `parcel-core` to validate that the
/// install directory is not a dangerous location before performing removal.
pub fn remove_installed_files(install_dir: &Path) -> Result<()> {
    if !install_dir.exists() {
        return Ok(());
    }

    // Run the authoritative safety validation from parcel-core.
    if let Err(e) = parcel_core::safety::validate_install_dir(install_dir) {
        log::error!(
            "SAFETY: Refusing to remove install directory: {e}"
        );
        return Err(e);
    }

    log::warn!("Rolling back: removing {}", install_dir.display());
    std::fs::remove_dir_all(install_dir).with_context(|| {
        format!(
            "Failed to remove install directory: {}",
            install_dir.display()
        )
    })?;

    Ok(())
}
