//! Talking to a key's mirror: a second homeserver its data is copied to.
//!
//! A mirror is not the key's homeserver in PKDNS, so the SDK's storage types,
//! which address a key's own homeserver, cannot reach it. This module speaks
//! the homeserver's HTTP API directly instead: every request goes to the
//! mirror, names the key in the `pubky-host` header and carries the access
//! token of the key's session on the mirror.
//!
//! That session is grant-backed, like the session private data is synced with
//! (see [`session`](super::session)). The owner's grant is not tied to a
//! homeserver, so the secret a session is stored as can name the mirror instead
//! of the key's own homeserver, and restoring it then exchanges the grant for
//! access tokens with the mirror. The key must have an account there.

use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;
use pubky::errors::RequestError;
use pubky::{Method, Pubky, PubkyHttpClient, PubkyResource, PubkySession, PublicKey, StatusCode};
use reqwest::{RequestBuilder, Response};
use url::Url;

use super::session::is_session_rejection;

/// Header naming the key a request to a homeserver is about.
const PUBKY_HOST_HEADER: &str = "pubky-host";

/// Prefix of the secrets grant-backed sessions are stored as, which name the
/// homeserver the session is on.
const GRANT_SECRET_PREFIX: &str = "pubky-grant-credential-";

/// Renew the access token when it has less than this many seconds left.
const REFRESH_SLACK_SECS: u64 = 300;

/// Errors of copying data to a mirror.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub(crate) enum MirrorError {
    /// The mirror does not accept the key's session
    #[error("The mirror rejected the session")]
    SessionRejected,

    /// The mirror could not be reached or refused the request
    #[error("{0}")]
    Failed(String),
}

impl From<crate::storage::StorageError> for MirrorError {
    fn from(error: crate::storage::StorageError) -> Self {
        Self::Failed(error.to_string())
    }
}

/// Somewhere a key's data can be copied to.
///
/// Implemented by [`MirrorSession`]; a trait so that the logic deciding what
/// to copy can be tested without a homeserver.
pub(crate) trait MirrorTarget {
    /// Store `data` as the resource, replacing what the mirror has for it.
    fn put(
        &self,
        resource: &PubkyResource,
        data: Vec<u8>,
    ) -> impl Future<Output = Result<(), MirrorError>> + Send;

    /// Delete the resource. Not an error if the mirror doesn't have it.
    fn delete(
        &self,
        resource: &PubkyResource,
    ) -> impl Future<Output = Result<(), MirrorError>> + Send;
}

/// The secret of a session on `homeserver` backed by the same grant as the
/// session stored as `secret`.
///
/// Stored grant secrets name the homeserver the grant is exchanged with, and
/// nothing else in them depends on it. Returns `None` if `secret` is not a
/// stored grant.
pub(crate) fn session_secret_for(secret: &str, homeserver: &PublicKey) -> Option<String> {
    let (prefix, rest) = secret.split_once(':')?;
    if !prefix.starts_with(GRANT_SECRET_PREFIX) {
        return None;
    }
    let (_homeserver, rest) = rest.split_once(':')?;
    Some(format!("{prefix}:{}:{rest}", homeserver.z32()))
}

/// A key's session on its mirror homeserver.
#[derive(Clone)]
pub(crate) struct MirrorSession {
    client: PubkyHttpClient,
    session: PubkySession,
    homeserver: PublicKey,
    user: PublicKey,
}

impl MirrorSession {
    /// Restore the key's session on `homeserver` from the secret it is stored
    /// as (see [`session_secret_for`]).
    ///
    /// This exchanges the grant with the mirror for an access token, so it
    /// also finds out whether the mirror still accepts the grant.
    ///
    /// # Errors
    ///
    /// Returns `MirrorError::SessionRejected` if the mirror does not accept the
    /// grant, and `MirrorError::Failed` if the key has no account on the
    /// mirror, or the mirror cannot be reached or refuses.
    pub(crate) async fn restore(
        pubky_client: &Pubky,
        homeserver: &PublicKey,
        secret: &str,
    ) -> Result<Self, MirrorError> {
        let session = match pubky_client.restore_session(secret).await {
            Ok(session) => session,
            Err(e) if is_session_rejection(&e) => return Err(MirrorError::SessionRejected),
            Err(pubky::Error::Request(RequestError::Server { status, .. }))
                if status == StatusCode::NOT_FOUND =>
            {
                return Err(MirrorError::Failed(
                    "The key has no account on this homeserver".to_string(),
                ))
            }
            // Transport errors spell out the request, which is for the log, not the user
            Err(e @ pubky::Error::Request(RequestError::Transport(_))) => {
                warn!("Could not reach the mirror {}: {}", homeserver, e);
                return Err(MirrorError::Failed(
                    "Could not reach the homeserver".to_string(),
                ));
            }
            Err(e @ pubky::Error::Pkarr(_)) => {
                warn!("Could not resolve the mirror {}: {}", homeserver, e);
                return Err(MirrorError::Failed(
                    "The homeserver could not be found".to_string(),
                ));
            }
            Err(e) => {
                return Err(MirrorError::Failed(format!(
                    "Could not sign in to the mirror: {}",
                    e
                )))
            }
        };
        Ok(Self {
            client: pubky_client.client().clone(),
            user: session.public_key(),
            session,
            homeserver: homeserver.clone(),
        })
    }

    /// The key this session belongs to.
    pub(crate) fn user(&self) -> &PublicKey {
        &self.user
    }

    /// End this session on the mirror, revoking the grant there.
    pub(crate) async fn end(self) -> Result<(), MirrorError> {
        self.session
            .signout()
            .await
            .map_err(|(e, _)| MirrorError::Failed(e.to_string()))
    }

    /// The session's current access token, renewed first if it is about to expire.
    async fn access_token(&self) -> Result<String, MirrorError> {
        let grant = self.session.as_grant().ok_or_else(|| {
            MirrorError::Failed("The session on the mirror is not grant-backed".to_string())
        })?;

        let expires_at = grant.session_info().await.token_expires_at;
        if expires_at <= unix_now() + REFRESH_SLACK_SECS {
            // Revalidating renews the token with the mirror first
            match self.session.revalidate().await {
                Ok(Some(_)) => {}
                Ok(None) => return Err(MirrorError::SessionRejected),
                Err(e) if is_session_rejection(&e) => return Err(MirrorError::SessionRejected),
                Err(e) => {
                    return Err(MirrorError::Failed(format!(
                        "Could not renew the session on the mirror: {}",
                        e
                    )))
                }
            }
        }
        Ok(grant.current_bearer().await)
    }

    /// A request to the mirror about this session's key, with its access token attached.
    async fn request(&self, method: Method, url: &Url) -> Result<RequestBuilder, MirrorError> {
        let token = self.access_token().await?;
        Ok(self
            .client
            .request(method, url)
            .header(PUBKY_HOST_HEADER, self.user.z32())
            .bearer_auth(token))
    }

    fn resource_url(&self, resource: &PubkyResource) -> Result<Url, MirrorError> {
        homeserver_url(&self.homeserver, resource.path.as_str())
    }
}

impl MirrorTarget for MirrorSession {
    async fn put(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<(), MirrorError> {
        let url = self.resource_url(resource)?;
        let request = self.request(Method::PUT, &url).await?.body(data);
        check_status(send(request).await?).await?;
        Ok(())
    }

    async fn delete(&self, resource: &PubkyResource) -> Result<(), MirrorError> {
        let url = self.resource_url(resource)?;
        let response = send(self.request(Method::DELETE, &url).await?).await?;
        // Already gone, for example because it never reached the mirror
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        check_status(response).await?;
        Ok(())
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// The URL of `path` on a homeserver. Encodes the path as needed.
fn homeserver_url(homeserver: &PublicKey, path: &str) -> Result<Url, MirrorError> {
    let mut url = Url::parse(&format!("https://{}/", homeserver.z32()))
        .map_err(|e| MirrorError::Failed(e.to_string()))?;
    url.set_path(path);
    Ok(url)
}

async fn send(request: RequestBuilder) -> Result<Response, MirrorError> {
    request.send().await.map_err(|e| {
        // The error spells out the request, which is for the log, not the user
        warn!("Could not reach the mirror: {}", e);
        MirrorError::Failed("Could not reach the mirror".to_string())
    })
}

/// Turn an unsuccessful response into the error it stands for.
async fn check_status(response: Response) -> Result<Response, MirrorError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Err(MirrorError::SessionRejected);
    }

    let message = response.text().await.unwrap_or_default();
    Err(MirrorError::Failed(describe_refusal(status, &message)))
}

fn describe_refusal(status: StatusCode, message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        format!("The mirror responded with {}", status)
    } else {
        format!("The mirror responded with {}: {}", status, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use std::str::FromStr;

    const OTHER_HOMESERVER: &str = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";

    fn homeserver() -> PublicKey {
        PublicKey::from_str(TEST_PUBKY).unwrap()
    }

    #[test]
    fn test_session_secret_for_names_the_other_homeserver() {
        let secret = format!("pubky-grant-credential-v1:{OTHER_HOMESERVER}:c2VjcmV0:eyJ.grant.sig");

        let moved = session_secret_for(&secret, &homeserver()).unwrap();

        assert_eq!(
            moved,
            format!("pubky-grant-credential-v1:{TEST_PUBKY}:c2VjcmV0:eyJ.grant.sig")
        );
        // Later versions keep the same layout up to the homeserver
        let with_approval =
            format!("pubky-grant-credential-v2:{OTHER_HOMESERVER}:c2VjcmV0:eyJ.grant.sig:approval");
        assert_eq!(
            session_secret_for(&with_approval, &homeserver()).unwrap(),
            format!("pubky-grant-credential-v2:{TEST_PUBKY}:c2VjcmV0:eyJ.grant.sig:approval")
        );
    }

    #[test]
    fn test_session_secret_for_rejects_other_secrets() {
        assert_eq!(session_secret_for("cookie-secret", &homeserver()), None);
        assert_eq!(
            session_secret_for("pubky-cookie-session:abc:def", &homeserver()),
            None
        );
        assert_eq!(
            session_secret_for("pubky-grant-credential-v1:incomplete", &homeserver()),
            None
        );
    }

    #[test]
    fn test_homeserver_url_addresses_the_homeserver() {
        let url = homeserver_url(&homeserver(), "/pub/app/file.txt").unwrap();

        assert_eq!(
            url.as_str(),
            format!("https://{}/pub/app/file.txt", TEST_PUBKY)
        );
    }

    #[test]
    fn test_homeserver_url_encodes_the_path() {
        let url = homeserver_url(&homeserver(), "/pub/my notes/what?.txt").unwrap();

        assert_eq!(url.path(), "/pub/my%20notes/what%3F.txt");
        // Nothing of the path may be read as a query
        assert_eq!(url.query(), None);
    }

    #[test]
    fn test_describe_refusal_includes_the_homeserver_message() {
        assert_eq!(
            describe_refusal(StatusCode::BAD_REQUEST, "Token required\n"),
            "The mirror responded with 400 Bad Request: Token required"
        );
        assert_eq!(
            describe_refusal(StatusCode::INSUFFICIENT_STORAGE, ""),
            "The mirror responded with 507 Insufficient Storage"
        );
    }
}
