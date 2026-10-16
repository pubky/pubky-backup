//! Scopes of a key's data, as the requests that sync them see them.

use pubky::PubkySession;

use crate::storage::DataScope;

/// Path prefix of a key's public data on its homeserver.
pub(crate) const PUBLIC_PATH: &str = "/pub/";

/// Path prefix of a key's private data on its homeserver.
pub(crate) const PRIVATE_PATH: &str = "/priv/";

/// The scope a sync pass covers, with what it takes to read that scope.
#[derive(Clone, Copy)]
pub(super) enum SyncScope<'a> {
    /// Public data (`/pub`), readable by anyone.
    Public,
    /// Private data (`/priv`), readable only with the owner's session.
    Private(&'a PubkySession),
}

impl SyncScope<'_> {
    /// The part of the key's data this scope covers.
    pub(super) fn data_scope(self) -> DataScope {
        match self {
            Self::Public => DataScope::Public,
            Self::Private(_) => DataScope::Private,
        }
    }
}
