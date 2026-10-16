//! Secrets kept for keys: session secrets, and mirrors with their sessions.
//!
//! A secret grants access to a key's data on a homeserver for as long as the
//! session lasts, so it is kept apart from the backed-up data: in the app data
//! directory, which never moves, rather than in the keys directory, which the
//! user may relocate to a synced or removable folder.

use std::io::Write;
use std::path::{Path, PathBuf};

use pubky::PublicKey;

use super::error::StorageError;

pub(crate) const SESSIONS_DIR_NAME: &str = "sessions";

/// Storage for one kind of secret, one file per key.
///
/// Located at: `~/.pubky-backup/<dir name>/<pubky>`
pub(super) struct SecretStorage {
    dir: PathBuf,
    /// What the secrets are, for error messages
    what: &'static str,
}

impl SecretStorage {
    pub(super) fn new(
        data_dir: &Path,
        dir_name: &str,
        what: &'static str,
    ) -> Result<Self, StorageError> {
        let dir = data_dir.join(dir_name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| StorageError::DirectoryCreation(format!("{}: {}", dir.display(), e)))?;
        Ok(Self { dir, what })
    }

    fn path(&self, pubky: &PublicKey) -> PathBuf {
        self.dir.join(pubky.z32())
    }

    /// Store a key's secret.
    ///
    /// The file is readable by its owner only (on Unix). Written atomically.
    pub(super) fn write(&self, pubky: &PublicKey, secret: &str) -> Result<(), StorageError> {
        let path = self.path(pubky);
        let temp_path = path.with_extension("tmp");
        write_owner_only(&temp_path, secret)
            .and_then(|_| std::fs::rename(&temp_path, &path))
            .map_err(|e| {
                let _ = std::fs::remove_file(&temp_path);
                StorageError::Internal(format!("Failed to write {}: {}", self.what, e))
            })
    }

    /// Read a key's stored secret, or `None` if it has none.
    pub(super) fn read(&self, pubky: &PublicKey) -> Result<Option<String>, StorageError> {
        match std::fs::read_to_string(self.path(pubky)) {
            Ok(secret) => Ok(Some(secret)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Internal(format!(
                "Failed to read {}: {}",
                self.what, e
            ))),
        }
    }

    /// Remove a key's stored secret and return it, or `None` if it has none.
    pub(super) fn take(&self, pubky: &PublicKey) -> Result<Option<String>, StorageError> {
        let secret = self.read(pubky)?;
        self.delete(pubky)?;
        Ok(secret)
    }

    /// Delete a key's stored secret. Does nothing if it has none.
    pub(super) fn delete(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        match std::fs::remove_file(self.path(pubky)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Internal(format!(
                "Failed to delete {}: {}",
                self.what, e
            ))),
        }
    }
}

/// Write a file that only its owner can read (mode 0600 on Unix).
fn write_owner_only(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn create_test_storage() -> (SecretStorage, PublicKey, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = SecretStorage::new(temp_dir.path(), "sessions", "session secret").unwrap();
        (storage, PublicKey::from_str(TEST_PUBKY).unwrap(), temp_dir)
    }

    #[test]
    fn test_secret_roundtrip() {
        let (storage, pubky, _temp_dir) = create_test_storage();

        assert_eq!(storage.read(&pubky).unwrap(), None);

        storage.write(&pubky, "pubky:secret").unwrap();
        assert_eq!(
            storage.read(&pubky).unwrap(),
            Some("pubky:secret".to_string())
        );

        // Writing again replaces the previous secret
        storage.write(&pubky, "pubky:newer").unwrap();
        assert_eq!(
            storage.read(&pubky).unwrap(),
            Some("pubky:newer".to_string())
        );
    }

    #[test]
    fn test_take_secret() {
        let (storage, pubky, _temp_dir) = create_test_storage();
        storage.write(&pubky, "pubky:secret").unwrap();

        assert_eq!(
            storage.take(&pubky).unwrap(),
            Some("pubky:secret".to_string())
        );
        assert_eq!(storage.read(&pubky).unwrap(), None);
        // Taking when there is no secret is not an error
        assert_eq!(storage.take(&pubky).unwrap(), None);
    }

    #[test]
    fn test_delete_secret() {
        let (storage, pubky, _temp_dir) = create_test_storage();

        storage.write(&pubky, "pubky:secret").unwrap();
        storage.delete(&pubky).unwrap();
        assert_eq!(storage.read(&pubky).unwrap(), None);

        // Deleting when there is no secret is not an error
        storage.delete(&pubky).unwrap();
    }

    #[test]
    fn test_secrets_of_different_kinds_are_kept_apart() {
        let temp_dir = TempDir::new().unwrap();
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let sessions = SecretStorage::new(temp_dir.path(), "sessions", "session").unwrap();
        let mirrors = SecretStorage::new(temp_dir.path(), "mirrors", "mirror").unwrap();

        sessions.write(&pubky, "session").unwrap();

        assert_eq!(mirrors.read(&pubky).unwrap(), None);
        assert!(temp_dir.path().join("sessions").join(pubky.z32()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_secret_is_readable_by_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (storage, pubky, _temp_dir) = create_test_storage();
        storage.write(&pubky, "pubky:secret").unwrap();

        let mode = std::fs::metadata(storage.path(&pubky))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
