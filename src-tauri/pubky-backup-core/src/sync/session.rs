//! Sessions of signed-in keys, used to sync their private data.
//!
//! Sessions are grant-backed. The stored secret holds the grant and this app's
//! key for it, from which a session gets fresh access tokens by itself until the
//! grant expires or is revoked.

use std::fmt;

use pubky::errors::RequestError;
use pubky::StatusCode;

/// The secret a session is stored as, as passed between the sync layer and
/// whoever manages the session. Its `Debug` output hides the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionSecret(String);

impl SessionSecret {
    pub(crate) fn new(secret: String) -> Self {
        Self(secret)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionSecret(..)")
    }
}

/// Whether an SDK error means the homeserver will not accept the session,
/// as opposed to a transient failure that is worth retrying.
pub(crate) fn is_session_rejection(error: &pubky::Error) -> bool {
    match error {
        pubky::Error::Authentication(_) => true,
        pubky::Error::Request(RequestError::Server { status, .. }) => {
            matches!(*status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
        }
        // The session belongs to a homeserver the key has since moved away from.
        // NOTE: String matching is fragile — the SDK doesn't expose a typed error
        // for this, so we check the message.
        pubky::Error::Request(RequestError::Validation { message }) => {
            message.contains("cannot attach session credential")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pubky::errors::AuthError;

    fn server_error(status: StatusCode) -> pubky::Error {
        RequestError::Server {
            status,
            message: "error".to_string(),
        }
        .into()
    }

    #[test]
    fn test_unauthorized_and_forbidden_are_rejections() {
        assert!(is_session_rejection(&server_error(
            StatusCode::UNAUTHORIZED
        )));
        assert!(is_session_rejection(&server_error(StatusCode::FORBIDDEN)));
    }

    #[test]
    fn test_expired_session_is_rejection() {
        assert!(is_session_rejection(&AuthError::RequestExpired.into()));
    }

    #[test]
    fn test_session_for_another_homeserver_is_rejection() {
        let error = RequestError::Validation {
            message: "cannot attach session credential to target homeserver".to_string(),
        };
        assert!(is_session_rejection(&error.into()));
    }

    #[test]
    fn test_transient_failures_are_not_rejections() {
        assert!(!is_session_rejection(&server_error(
            StatusCode::INTERNAL_SERVER_ERROR
        )));
        assert!(!is_session_rejection(&server_error(
            StatusCode::TOO_MANY_REQUESTS
        )));
        // An unresolvable homeserver is a network problem, not a bad session.
        let error = RequestError::Validation {
            message: "could not resolve homeserver for user".to_string(),
        };
        assert!(!is_session_rejection(&error.into()));
    }
}
