//! Scopes of a key's data that are synced separately.

use pubky::PubkySession;

/// Path prefix of a key's private data on its homeserver.
pub(crate) const PRIVATE_PATH: &str = "/priv/";

/// Which part of a key's data a sync pass covers.
///
/// Each scope has its own event stream and cursor, so losing access to private
/// data never holds back the backup of public data.
#[derive(Clone, Copy)]
pub(super) enum SyncScope<'a> {
    /// Public data (`/pub`), readable by anyone.
    Public,
    /// Private data (`/priv`), readable only with the owner's session.
    Private(&'a PubkySession),
}
