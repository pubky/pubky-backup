//! Legacy data detection and cleanup.
//!
//! Previous versions used different storage layouts. Rather than maintaining
//! complex migration code for each historical format, we detect any old layout
//! and remove it so the app starts fresh. Backed-up data is always re-fetched
//! from homeservers, so nothing is permanently lost.
//!
//! # Detection
//!
//! Old formats are detected by the presence of a `config/` subdirectory in the
//! data root. This directory existed in an intermediate storage layout and is
//! never created by the current version.

use super::error::StorageError;
use log::info;
use std::path::Path;

/// Detect and remove old storage layouts so the app starts fresh.
///
/// If the legacy `config/` subdirectory exists in the data root, the entire
/// data directory is wiped. The current storage code will recreate everything
/// it needs on first use.
///
/// This is safe because all backed-up data can be re-fetched from homeservers.
pub fn remove_legacy_data(data_dir: &Path) -> Result<(), StorageError> {
    let old_config_dir = data_dir.join("config");

    if !old_config_dir.exists() {
        return Ok(());
    }

    info!(
        "Detected legacy storage layout in {}, removing to start fresh",
        data_dir.display()
    );

    // Remove everything in the data directory
    for entry in std::fs::read_dir(data_dir)
        .map_err(|e| StorageError::Internal(format!("Failed to read data directory: {}", e)))?
    {
        let entry = entry.map_err(|e| {
            StorageError::Internal(format!("Failed to read directory entry: {}", e))
        })?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| {
                StorageError::Internal(format!("Failed to remove {}: {}", path.display(), e))
            })?;
        } else {
            std::fs::remove_file(&path).map_err(|e| {
                StorageError::Internal(format!("Failed to remove {}: {}", path.display(), e))
            })?;
        }
    }

    info!("Legacy data removed, app will start fresh");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_no_legacy_data_is_noop() {
        let temp_dir = TempDir::new().unwrap();

        // Create current-format files
        std::fs::write(temp_dir.path().join("config.json"), "{}").unwrap();
        std::fs::create_dir_all(temp_dir.path().join("keys")).unwrap();

        remove_legacy_data(temp_dir.path()).unwrap();

        // Everything should still be there
        assert!(temp_dir.path().join("config.json").exists());
        assert!(temp_dir.path().join("keys").exists());
    }

    #[test]
    fn test_legacy_config_dir_triggers_wipe() {
        let temp_dir = TempDir::new().unwrap();

        // Create old config/ subdirectory (legacy marker)
        let old_config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&old_config_dir).unwrap();
        std::fs::write(old_config_dir.join("config.json"), "{}").unwrap();

        // Create some other files that would exist in old layout
        std::fs::write(temp_dir.path().join("last_pubky"), "some_key").unwrap();
        std::fs::create_dir_all(temp_dir.path().join("keys").join("some_key")).unwrap();

        remove_legacy_data(temp_dir.path()).unwrap();

        // Everything should be gone
        assert!(!old_config_dir.exists());
        assert!(!temp_dir.path().join("last_pubky").exists());
        assert!(!temp_dir.path().join("keys").exists());

        // Data dir itself should still exist
        assert!(temp_dir.path().exists());
    }

    #[test]
    fn test_empty_data_dir_is_noop() {
        let temp_dir = TempDir::new().unwrap();
        remove_legacy_data(temp_dir.path()).unwrap();
        // Should not error on empty dir
    }

    #[test]
    fn test_nonexistent_data_dir_is_noop() {
        let path = Path::new("/tmp/nonexistent_pubky_backup_test_dir");
        assert!(!path.exists());
        // Should not error
        remove_legacy_data(path).unwrap();
    }
}
