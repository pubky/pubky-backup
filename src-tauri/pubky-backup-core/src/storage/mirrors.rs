//! Mirrors of keys: the second homeserver each is copied to, and the key's
//! session on it.
//!
//! The record holds a session secret, so it is kept with the other secrets in
//! the app data directory (see [`secrets`](super::secrets)), not with the
//! backed-up data. How far a mirror is synced is kept with the backup instead,
//! in the key's cursors.

use std::path::Path;

use pubky::PublicKey;
use serde::{Deserialize, Serialize};

use super::error::StorageError;
use super::secrets::SecretStorage;

pub(crate) const MIRRORS_DIR_NAME: &str = "mirrors";

/// The second homeserver a key's data is copied to, as stored for the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMirror {
    /// The mirror homeserver
    pub homeserver: PublicKey,
    /// Secret of the key's session on the mirror. `None` once the mirror
    /// stopped accepting it, until the key's owner approves a new one.
    pub session: Option<String>,
}

impl StoredMirror {
    /// This mirror without its session, as it is kept once the mirror
    /// stopped accepting the session.
    pub fn without_session(&self) -> Self {
        Self {
            homeserver: self.homeserver.clone(),
            session: None,
        }
    }
}

/// Storage for the mirrors of keys, one file per key.
///
/// Located at: `~/.pubky-backup/mirrors/<pubky>`
pub(super) struct MirrorStorage {
    secrets: SecretStorage,
}

impl MirrorStorage {
    pub(super) fn new(data_dir: &Path) -> Result<Self, StorageError> {
        Ok(Self {
            secrets: SecretStorage::new(data_dir, MIRRORS_DIR_NAME, "mirror")?,
        })
    }

    /// Store a key's mirror.
    ///
    /// The file is readable by its owner only (on Unix). Written atomically.
    pub(super) fn write(
        &self,
        pubky: &PublicKey,
        mirror: &StoredMirror,
    ) -> Result<(), StorageError> {
        let json =
            serde_json::to_string(mirror).map_err(|e| StorageError::Internal(e.to_string()))?;
        self.secrets.write(pubky, &json)
    }

    /// Read a key's mirror, or `None` if it has none.
    pub(super) fn read(&self, pubky: &PublicKey) -> Result<Option<StoredMirror>, StorageError> {
        self.secrets
            .read(pubky)?
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|e| StorageError::Internal(format!("Invalid mirror record: {}", e)))
            })
            .transpose()
    }

    /// Forget a key's mirror. Does nothing if it has none.
    pub(super) fn delete(&self, pubky: &PublicKey) -> Result<(), StorageError> {
        self.secrets.delete(pubky)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use std::str::FromStr;
    use tempfile::TempDir;

    const HOMESERVER: &str = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";

    fn create_test_storage() -> (MirrorStorage, PublicKey, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = MirrorStorage::new(temp_dir.path()).unwrap();
        (storage, PublicKey::from_str(TEST_PUBKY).unwrap(), temp_dir)
    }

    fn test_mirror(session: Option<&str>) -> StoredMirror {
        StoredMirror {
            homeserver: PublicKey::from_str(HOMESERVER).unwrap(),
            session: session.map(str::to_string),
        }
    }

    #[test]
    fn test_mirror_roundtrip() {
        let (storage, pubky, _temp_dir) = create_test_storage();
        assert_eq!(storage.read(&pubky).unwrap(), None);

        let mirror = test_mirror(Some("secret"));
        storage.write(&pubky, &mirror).unwrap();

        assert_eq!(storage.read(&pubky).unwrap(), Some(mirror));
    }

    #[test]
    fn test_delete_mirror() {
        let (storage, pubky, _temp_dir) = create_test_storage();
        storage.write(&pubky, &test_mirror(Some("secret"))).unwrap();

        storage.delete(&pubky).unwrap();

        assert_eq!(storage.read(&pubky).unwrap(), None);
        // Deleting when there is no mirror is not an error
        storage.delete(&pubky).unwrap();
    }

    #[test]
    fn test_without_session_keeps_the_homeserver() {
        let mirror = test_mirror(Some("secret"));

        assert_eq!(mirror.without_session(), test_mirror(None));
    }

    #[test]
    fn test_invalid_record_is_an_error() {
        let (storage, pubky, temp_dir) = create_test_storage();
        std::fs::write(
            temp_dir.path().join(MIRRORS_DIR_NAME).join(pubky.z32()),
            "not json",
        )
        .unwrap();

        assert!(storage.read(&pubky).is_err());
    }
}
