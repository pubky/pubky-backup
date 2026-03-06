//! Migration from old storage structure to new structure.
//!
//! This module handles automatic migration of data from the old flat storage
//! structure to the new hierarchical structure.
//!
//! # Old Structure
//!
//! ```text
//! ~/.pubky-backup/
//! ├── <pubky>/
//! │   ├── cursor
//! │   └── pub/...
//! ├── error.log
//! └── last_pubky
//! ```
//!
//! # New Structure
//!
//! ```text
//! ~/.pubky-backup/
//! ├── config/
//! │   └── last_pubky
//! ├── logs/
//! │   └── error.log
//! └── keys/
//!     └── <pubky>/
//!         ├── state/
//!         │   ├── cursor
//!         │   └── error.log
//!         ├── data/
//!         │   └── pub/...
//!         └── snapshots/
//! ```

use super::error::StorageError;
use super::{
    CONFIG_DIR_NAME, CURSOR_FILENAME, DATA_DIR_NAME, ERROR_LOG_FILENAME, KEYS_DIR_NAME,
    LAST_PUBKY_FILENAME, LOGS_DIR_NAME, STATE_DIR_NAME,
};
use log::{error, info};
use pubky::PublicKey;
use std::path::Path;
use std::str::FromStr;

/// Move a file from src to dst, using rename if possible, copy+delete as fallback.
/// Returns Ok(true) if moved successfully, Ok(false) if destination already exists.
/// Note: Source file cleanup is best-effort; may be left behind on permission errors.
fn move_file(src: &Path, dst: &Path) -> Result<bool, StorageError> {
    if dst.exists() {
        // Destination already exists, clean up source
        let _ = std::fs::remove_file(src);
        return Ok(false);
    }

    if std::fs::rename(src, dst).is_ok() {
        return Ok(true);
    }

    // Fallback to copy + delete for cross-filesystem moves
    std::fs::copy(src, dst).map_err(|e| {
        StorageError::Internal(format!(
            "Failed to copy {} to {}: {}",
            src.display(),
            dst.display(),
            e
        ))
    })?;
    let _ = std::fs::remove_file(src);
    Ok(true)
}

/// Recursively copy directory contents from src to dst.
/// Used for legacy directory structure migration.
fn recursive_copy_dir(src: &Path, dst: &Path) -> Result<(), StorageError> {
    std::fs::create_dir_all(dst)
        .map_err(|e| StorageError::DirectoryCreation(format!("{}: {}", dst.display(), e)))?;

    for entry in std::fs::read_dir(src).map_err(|e| {
        StorageError::Internal(format!("Failed to read directory {}: {}", src.display(), e))
    })? {
        let entry = entry.map_err(|e| {
            StorageError::Internal(format!("Failed to read directory entry: {}", e))
        })?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            recursive_copy_dir(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                StorageError::Internal(format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

/// Migrate from old storage structure to new structure.
///
/// Detects old-style directories and moves them to new locations.
/// Safe to call multiple times - detects if migration already done.
pub fn migrate_old_structure(data_dir: &Path) -> Result<(), StorageError> {
    if !data_dir.exists() {
        return Ok(());
    }

    // Find old-style pubky directories (valid pubky strings in root, excluding new structure dirs)
    let mut old_pubky_dirs: Vec<(String, std::path::PathBuf)> = Vec::new();
    let new_structure_dirs = [CONFIG_DIR_NAME, LOGS_DIR_NAME, KEYS_DIR_NAME];

    for entry in std::fs::read_dir(data_dir)
        .map_err(|e| StorageError::Internal(format!("Failed to read data directory: {}", e)))?
    {
        let entry = entry.map_err(|e| {
            StorageError::Internal(format!("Failed to read directory entry: {}", e))
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy().to_string();
            if new_structure_dirs.contains(&name_str.as_str()) {
                continue;
            }
            if PublicKey::from_str(&name_str).is_ok() {
                old_pubky_dirs.push((name_str, path));
            }
        }
    }

    // Check for old app-level files
    let old_last_pubky = data_dir.join(LAST_PUBKY_FILENAME);
    let old_error_log = data_dir.join(ERROR_LOG_FILENAME);
    let keys_dir = data_dir.join(KEYS_DIR_NAME);

    // Nothing to migrate
    if old_pubky_dirs.is_empty() && !old_last_pubky.exists() && !old_error_log.exists() {
        return Ok(());
    }

    info!(
        "Starting migration: {} keys, last_pubky={}, error.log={}",
        old_pubky_dirs.len(),
        old_last_pubky.exists(),
        old_error_log.exists()
    );

    if old_last_pubky.exists() {
        let config_dir = data_dir.join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&config_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", config_dir.display(), e))
        })?;

        let new_last_pubky = config_dir.join(LAST_PUBKY_FILENAME);
        if move_file(&old_last_pubky, &new_last_pubky)? {
            info!("Migrating last_pubky to config/last_pubky");
        } else {
            info!("Skipping last_pubky migration: destination already exists");
        }
    }

    if old_error_log.exists() {
        let logs_dir = data_dir.join(LOGS_DIR_NAME);
        std::fs::create_dir_all(&logs_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("{}: {}", logs_dir.display(), e))
        })?;

        let new_error_log = logs_dir.join(ERROR_LOG_FILENAME);
        if move_file(&old_error_log, &new_error_log)? {
            info!("Migrating error.log to logs/error.log");
        } else {
            info!("Skipping error.log migration: destination already exists");
        }
    }

    // Migrate each old pubky directory
    let mut migrated_count = 0;

    for (pubky_str, old_pubky_dir) in &old_pubky_dirs {
        let pubky = PublicKey::from_str(pubky_str).expect("Already validated");
        let z32_name = pubky.z32();
        info!("Migrating key: {} -> {}...", pubky_str, z32_name);

        let new_key_dir = keys_dir.join(&z32_name);
        let new_state_dir = new_key_dir.join(STATE_DIR_NAME);
        let new_data_dir = new_key_dir.join(DATA_DIR_NAME);

        // Create new directory structure
        if let Err(e) = std::fs::create_dir_all(&new_state_dir) {
            error!("Failed to create state directory for {}: {}", pubky_str, e);
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&new_data_dir) {
            error!("Failed to create data directory for {}: {}", pubky_str, e);
            continue;
        }

        let mut migration_success = true;

        // Migrate cursor file
        let old_cursor = old_pubky_dir.join(CURSOR_FILENAME);
        if old_cursor.exists() {
            let new_cursor = new_state_dir.join(CURSOR_FILENAME);
            if let Err(e) = move_file(&old_cursor, &new_cursor) {
                error!("Failed to migrate cursor for {}: {}", pubky_str, e);
                migration_success = false;
            }
        }

        // Migrate key-specific error.log
        let old_key_error_log = old_pubky_dir.join(ERROR_LOG_FILENAME);
        if old_key_error_log.exists() {
            let new_key_error_log = new_state_dir.join(ERROR_LOG_FILENAME);
            if let Err(e) = move_file(&old_key_error_log, &new_key_error_log) {
                error!("Failed to migrate error.log for {}: {}", pubky_str, e);
                migration_success = false;
            }
        }

        // Migrate pub/ directory contents
        let old_pub_dir = old_pubky_dir.join("pub");
        if old_pub_dir.exists() && old_pub_dir.is_dir() {
            let new_pub_dir = new_data_dir.join("pub");
            if !new_pub_dir.exists() {
                if let Err(e) = recursive_copy_dir(&old_pub_dir, &new_pub_dir) {
                    error!("Failed to migrate pub/ for {}: {}", pubky_str, e);
                    migration_success = false;
                } else {
                    // Remove old pub directory after successful copy
                    let _ = std::fs::remove_dir_all(&old_pub_dir);
                }
            } else {
                // Destination exists, just remove old directory
                let _ = std::fs::remove_dir_all(&old_pub_dir);
            }
        }

        // If all files migrated successfully, remove old pubky directory
        if migration_success {
            // Try to remove the old directory (it should be empty now)
            if let Err(e) = std::fs::remove_dir(old_pubky_dir) {
                // If it's not empty, try to remove all remaining contents
                if let Err(e2) = std::fs::remove_dir_all(old_pubky_dir) {
                    error!(
                        "Failed to remove old directory {}: {} (also tried remove_dir_all: {})",
                        pubky_str, e, e2
                    );
                }
            }
            migrated_count += 1;
        }
    }

    info!("Migration complete: {} keys migrated", migrated_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppStorage;
    use crate::TEST_PUBKY;
    use pubky::PubkyResource;
    use std::path::Path;
    use tempfile::TempDir;

    /// Helper to create old-style storage structure for testing
    fn create_old_structure(
        temp_dir: &Path,
        pubky_str: &str,
        cursor_value: Option<&str>,
        error_log: Option<&str>,
        pub_files: &[(&str, &[u8])],
    ) {
        let pubky_dir = temp_dir.join(pubky_str);
        std::fs::create_dir_all(&pubky_dir).unwrap();

        if let Some(cursor) = cursor_value {
            std::fs::write(pubky_dir.join(CURSOR_FILENAME), cursor).unwrap();
        }

        if let Some(log) = error_log {
            std::fs::write(pubky_dir.join(ERROR_LOG_FILENAME), log).unwrap();
        }

        if !pub_files.is_empty() {
            let pub_dir = pubky_dir.join("pub");
            for (path, content) in pub_files {
                let file_path = pub_dir.join(path);
                std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
                std::fs::write(file_path, content).unwrap();
            }
        }
    }

    #[test]
    fn test_migration_old_to_new_single_key() {
        let temp_dir = TempDir::new().unwrap();
        let pubky_str = TEST_PUBKY;

        // Setup old structure
        create_old_structure(
            temp_dir.path(),
            pubky_str,
            Some("test_cursor_123"),
            Some("error log content"),
            &[("profile.json", b"profile data")],
        );

        // Also add old app-level files
        std::fs::write(temp_dir.path().join(LAST_PUBKY_FILENAME), pubky_str).unwrap();
        std::fs::write(temp_dir.path().join(ERROR_LOG_FILENAME), "global error log").unwrap();

        // Run migration
        migrate_old_structure(temp_dir.path()).unwrap();

        // Verify app-level files migrated
        assert!(
            temp_dir
                .path()
                .join(CONFIG_DIR_NAME)
                .join(LAST_PUBKY_FILENAME)
                .exists(),
            "last_pubky should be in config/"
        );
        assert!(
            temp_dir
                .path()
                .join(LOGS_DIR_NAME)
                .join(ERROR_LOG_FILENAME)
                .exists(),
            "error.log should be in logs/"
        );
        assert!(
            !temp_dir.path().join(LAST_PUBKY_FILENAME).exists(),
            "old last_pubky should be removed"
        );
        assert!(
            !temp_dir.path().join(ERROR_LOG_FILENAME).exists(),
            "old error.log should be removed"
        );

        // Verify key migrated
        let new_key_dir = temp_dir.path().join(KEYS_DIR_NAME).join(pubky_str);
        assert!(
            new_key_dir
                .join(STATE_DIR_NAME)
                .join(CURSOR_FILENAME)
                .exists(),
            "cursor should be in state/"
        );
        assert!(
            new_key_dir
                .join(STATE_DIR_NAME)
                .join(ERROR_LOG_FILENAME)
                .exists(),
            "key error.log should be in state/"
        );
        assert!(
            new_key_dir
                .join(DATA_DIR_NAME)
                .join("pub")
                .join("profile.json")
                .exists(),
            "pub files should be in data/"
        );

        // Verify old structure removed
        assert!(
            !temp_dir.path().join(pubky_str).exists(),
            "old pubky directory should be removed"
        );

        // Verify data integrity
        let cursor_content =
            std::fs::read_to_string(new_key_dir.join(STATE_DIR_NAME).join(CURSOR_FILENAME))
                .unwrap();
        assert_eq!(cursor_content, "test_cursor_123");

        let profile_content = std::fs::read(
            new_key_dir
                .join(DATA_DIR_NAME)
                .join("pub")
                .join("profile.json"),
        )
        .unwrap();
        assert_eq!(profile_content, b"profile data");
    }

    #[test]
    fn test_migration_old_to_new_multiple_keys() {
        let temp_dir = TempDir::new().unwrap();
        let pubky1 = TEST_PUBKY;
        let pubky2 = "o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo";

        // Setup old structure for both keys
        create_old_structure(
            temp_dir.path(),
            pubky1,
            Some("cursor1"),
            None,
            &[("file1.json", b"data1")],
        );
        create_old_structure(
            temp_dir.path(),
            pubky2,
            Some("cursor2"),
            None,
            &[("file2.json", b"data2")],
        );

        // Run migration
        migrate_old_structure(temp_dir.path()).unwrap();

        // Verify both keys migrated
        let new_key1_dir = temp_dir.path().join(KEYS_DIR_NAME).join(pubky1);
        let new_key2_dir = temp_dir.path().join(KEYS_DIR_NAME).join(pubky2);

        assert!(new_key1_dir
            .join(STATE_DIR_NAME)
            .join(CURSOR_FILENAME)
            .exists());
        assert!(new_key1_dir
            .join(DATA_DIR_NAME)
            .join("pub")
            .join("file1.json")
            .exists());

        assert!(new_key2_dir
            .join(STATE_DIR_NAME)
            .join(CURSOR_FILENAME)
            .exists());
        assert!(new_key2_dir
            .join(DATA_DIR_NAME)
            .join("pub")
            .join("file2.json")
            .exists());

        // Verify old directories removed
        assert!(!temp_dir.path().join(pubky1).exists());
        assert!(!temp_dir.path().join(pubky2).exists());
    }

    #[tokio::test]
    async fn test_migration_already_migrated() {
        // Create storage with new structure
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Write some data using new structure
        let resource = PubkyResource::new(pubky.clone(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();
        storage.write_cursor(&pubky, 12345).await.unwrap();

        // Run migration again (should be idempotent)
        migrate_old_structure(temp_dir.path()).unwrap();

        // Verify nothing changed
        let cursor = storage.read_cursor(&pubky).await.unwrap();
        assert_eq!(cursor, Some(12345));

        let data = storage.read(&resource).await.unwrap();
        assert_eq!(data, b"test data");
    }

    #[test]
    fn test_migration_no_old_structure() {
        let temp_dir = TempDir::new().unwrap();

        // Run migration on empty directory
        migrate_old_structure(temp_dir.path()).unwrap();

        // Nothing should have been created
        assert!(!temp_dir.path().join(CONFIG_DIR_NAME).exists());
        assert!(!temp_dir.path().join(LOGS_DIR_NAME).exists());
        assert!(!temp_dir.path().join(KEYS_DIR_NAME).exists());
    }

    #[test]
    fn test_migration_handles_invalid_pubky_names() {
        let temp_dir = TempDir::new().unwrap();
        let valid_pubky = TEST_PUBKY;

        // Create valid pubky directory
        create_old_structure(
            temp_dir.path(),
            valid_pubky,
            Some("valid_cursor"),
            None,
            &[],
        );

        // Create invalid directory that looks like it could be a pubky
        let invalid_dir = temp_dir.path().join("not_a_valid_pubky_string");
        std::fs::create_dir_all(&invalid_dir).unwrap();
        std::fs::write(invalid_dir.join(CURSOR_FILENAME), "invalid_cursor").unwrap();

        // Run migration
        migrate_old_structure(temp_dir.path()).unwrap();

        // Verify valid pubky migrated
        let new_key_dir = temp_dir.path().join(KEYS_DIR_NAME).join(valid_pubky);
        assert!(new_key_dir
            .join(STATE_DIR_NAME)
            .join(CURSOR_FILENAME)
            .exists());
        assert!(!temp_dir.path().join(valid_pubky).exists());

        // Verify invalid directory was NOT migrated (left alone)
        assert!(
            invalid_dir.exists(),
            "Invalid directory should be left alone"
        );
    }

    #[test]
    fn test_list_keys_normalizes_to_z32() {
        let temp_dir = TempDir::new().unwrap();
        let z32_key = TEST_PUBKY;
        let prefixed_key = format!("pubky{}", z32_key);

        // Create keys directory with prefixed directory name
        let keys_dir = temp_dir.path().join(KEYS_DIR_NAME);
        let prefixed_dir = keys_dir.join(&prefixed_key);
        std::fs::create_dir_all(&prefixed_dir).unwrap();

        // Create KeysStorage and list keys
        let keys_storage = crate::storage::KeysStorage::new_with_path(&keys_dir).unwrap();
        let keys = keys_storage.list_keys().unwrap();

        // Should return the z32 format, not the prefixed format
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], z32_key);
        assert!(!keys[0].starts_with("pubky"));
    }
}
