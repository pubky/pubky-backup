//! Session persistence for the backup manager.
//!
//! This module handles persisting and restoring session state across application
//! restarts. Currently this includes tracking the "last used pubky" so the UI
//! can restore the user's previous selection.
//!
//! # Responsibilities
//!
//! - Writing/reading the last used pubky to/from storage
//! - Future: Additional session state persistence

use pubky::PublicKey;

use super::error::OrchestratorError;
use crate::storage::AppStorage;

/// Write the last used pubky to storage for session persistence.
///
/// This allows the app to remember which pubky was last used.
///
/// # Arguments
///
/// * `storage` - The app storage instance
/// * `pubky` - The pubky to save as "last used"
///
/// # Errors
///
/// Returns `OrchestratorError` if the write operation fails.
pub async fn write_last_pubky(
    storage: &AppStorage,
    pubky: &PublicKey,
) -> Result<(), OrchestratorError> {
    storage.write_last_pubky(pubky).await?;
    Ok(())
}

/// Read the last used pubky from storage.
///
/// # Arguments
///
/// * `storage` - The app storage instance
///
/// # Returns
///
/// The last used pubky, or `None` if no last pubky was saved.
///
/// # Errors
///
/// Returns `OrchestratorError` if the read operation fails.
pub async fn read_last_pubky(storage: &AppStorage) -> Result<Option<PublicKey>, OrchestratorError> {
    Ok(storage.read_last_pubky().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use std::str::FromStr;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_last_pubky_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();

        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();

        // Initially no last pubky
        let result = read_last_pubky(&storage).await.unwrap();
        assert!(result.is_none());

        // Write last pubky
        write_last_pubky(&storage, &pubky).await.unwrap();

        // Read back
        let result = read_last_pubky(&storage).await.unwrap();
        assert_eq!(result, Some(pubky));
    }
}
