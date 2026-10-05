//! Signing a key in and out.
//!
//! Signing in gives the backup read access to the key's private data. The key's
//! owner approves the request in their signer app (e.g. Pubky Ring), so the
//! key's secret never reaches this library.
//!
//! Sessions use the SDK's cookie authentication. The SDK deprecates it in
//! favour of grants, but cookie auth is what we use.
#![allow(deprecated)]

use log::warn;
use pubky::{AuthFlowKind, Capabilities, Pubky, PubkyCookieAuthFlow, PubkySession, PublicKey};
use tokio::sync::oneshot;
use url::Url;

use super::error::OrchestratorError;
use crate::sync::session::{export_session_secret, restore_session};
use crate::sync::PRIVATE_PATH;

/// A sign-in that is waiting for the key's owner to approve it.
///
/// Returned by [`BackupManager::start_sign_in`](super::BackupManager::start_sign_in).
/// Show [`authorization_url`](Self::authorization_url) to the owner, then either
/// await [`approved`](Self::approved) or drop the request and follow the key's
/// [`AuthStatus`](super::AuthStatus) instead. Dropping it does not cancel the sign-in.
#[derive(Debug)]
pub struct SignInRequest {
    authorization_url: String,
    outcome: oneshot::Receiver<Result<(), OrchestratorError>>,
}

impl SignInRequest {
    pub(super) fn new(
        authorization_url: String,
        outcome: oneshot::Receiver<Result<(), OrchestratorError>>,
    ) -> Self {
        Self {
            authorization_url,
            outcome,
        }
    }

    /// The `pubkyauth://` link the owner opens or scans with their signer app.
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    /// Wait until the owner approves the sign-in.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::AuthFailed` if the request expires, is approved
    /// by a different key, or is cancelled.
    pub async fn approved(self) -> Result<(), OrchestratorError> {
        self.outcome
            .await
            .unwrap_or_else(|_| Err(sign_in_cancelled()))
    }
}

/// A started sign-in request that is listening for its approval.
pub(super) type SignInFlow = PubkyCookieAuthFlow;

/// Start a sign-in request asking for read access to private data.
///
/// The returned flow carries the link to show to the signer app and starts
/// listening for the approval right away.
pub(super) fn start_sign_in_flow(
    pubky_client: &Pubky,
    http_relay: Option<&Url>,
) -> Result<SignInFlow, OrchestratorError> {
    let capabilities = Capabilities::builder()
        .read(PRIVATE_PATH)
        .map_err(|e| OrchestratorError::AuthFailed(e.to_string()))?
        .finish();

    let mut builder = PubkyCookieAuthFlow::builder(&capabilities, AuthFlowKind::signin())
        .client(pubky_client.client().clone());
    if let Some(relay) = http_relay {
        builder = builder.relay(relay.clone());
    }

    builder
        .start()
        .map_err(|e| OrchestratorError::AuthFailed(e.to_string()))
}

/// Wait for a sign-in request to be approved by `pubky` and return its session.
///
/// # Errors
///
/// Returns `OrchestratorError::AuthFailed` if the request expires, or if it is
/// approved by a different key than `pubky`.
pub(super) async fn await_approval(
    flow: SignInFlow,
    pubky: &PublicKey,
) -> Result<PubkySession, OrchestratorError> {
    let session = flow
        .await_approval()
        .await
        .map_err(|e| OrchestratorError::AuthFailed(e.to_string()))?;

    let approved_by = session.public_key();
    if approved_by != *pubky {
        // Don't leave a session we won't use open on the other key's homeserver
        end_session(session);
        return Err(OrchestratorError::AuthFailed(format!(
            "The sign-in was approved by a different key ({})",
            approved_by.z32()
        )));
    }

    Ok(session)
}

/// The secret a session is stored as.
pub(super) fn session_secret(session: &PubkySession) -> Result<String, OrchestratorError> {
    export_session_secret(session)
        .ok_or_else(|| OrchestratorError::AuthFailed("The session cannot be stored".to_string()))
}

/// The error for a sign-in that was cancelled before it could complete.
pub(super) fn sign_in_cancelled() -> OrchestratorError {
    OrchestratorError::AuthFailed("The sign-in was cancelled".to_string())
}

/// End a session on its homeserver, in the background and on a best-effort
/// basis: the caller no longer uses the session either way.
pub(super) fn end_session(session: PubkySession) {
    tokio::spawn(async move {
        let pubky = session.public_key();
        if let Err((e, _)) = session.signout().await {
            warn!(
                "Failed to end session of {} on its homeserver: {}",
                pubky, e
            );
        }
    });
}

/// End a session that is only known by its stored secret, like [`end_session`].
pub(super) fn end_stored_session(secret: String, pubky: PublicKey, pubky_client: Pubky) {
    tokio::spawn(async move {
        match restore_session(&secret, &pubky_client).await {
            Ok(session) => end_session(session),
            Err(e) => warn!(
                "Failed to end session of {} on its homeserver: {}",
                pubky, e
            ),
        }
    });
}
