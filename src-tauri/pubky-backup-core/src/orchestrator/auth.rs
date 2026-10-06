//! Getting a key owner's approval, and the sessions that come of it.
//!
//! Signing in gives the backup read access to the key's private data; setting
//! up a mirror gives it write access to a second homeserver. Either way the
//! key's owner approves the request in their signer app (e.g. Pubky Ring), so
//! the key's secret never reaches this library.
//!
//! The owner's approval is a grant to this app. The backup exchanges it for
//! short-lived access tokens by itself, without the signer app, until the grant
//! expires or the owner revokes it.

use log::warn;
use pubky::{
    AuthFlowKind, Capabilities, ClientId, Pubky, PubkyGrantAuthFlow, PubkySession, PublicKey,
    StoragePath,
};
use pubky_common::capabilities::Action;
use tokio::sync::oneshot;
use url::Url;

use super::error::OrchestratorError;
use crate::sync::{PRIVATE_PATH, PUBLIC_PATH};

/// A request that is waiting for the key's owner to approve it.
///
/// Returned by [`BackupManager::start_sign_in`](super::BackupManager::start_sign_in)
/// and [`BackupManager::start_mirror`](super::BackupManager::start_mirror), and
/// as an `ApprovalRequest<PublicKey>` by
/// [`BackupManager::start_sign_in_for_new_key`](super::BackupManager::start_sign_in_for_new_key),
/// whose approval tells which key signed in.
///
/// Show [`authorization_url`](Self::authorization_url) to the owner, then either
/// await [`approved`](Self::approved) or drop the request and follow the key's
/// state instead. Dropping it does not cancel the request.
#[derive(Debug)]
pub struct ApprovalRequest<T = ()> {
    authorization_url: String,
    outcome: oneshot::Receiver<Result<T, OrchestratorError>>,
}

impl<T> ApprovalRequest<T> {
    pub(super) fn new(
        authorization_url: String,
        outcome: oneshot::Receiver<Result<T, OrchestratorError>>,
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

    /// Wait until the owner approves the request and it has taken effect.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::AuthFailed` if the request expires, is approved
    /// by a different key or without the access it asked for, or is cancelled.
    /// A sign-in for a new key also fails with the error that kept the key from
    /// being added, and a mirror request with the error that kept the mirror
    /// from being set up.
    pub async fn approved(self) -> Result<T, OrchestratorError> {
        self.outcome
            .await
            .unwrap_or_else(|_| Err(approval_cancelled()))
    }
}

/// A started approval request that is listening for its approval.
pub(super) type ApprovalFlow = PubkyGrantAuthFlow;

/// What signing in asks for: read access to private data.
pub(super) fn sign_in_capabilities() -> Capabilities {
    Capabilities::builder()
        .read(PRIVATE_PATH)
        .expect("the private data path is a valid scope")
        .finish()
}

/// What a mirror asks for: write access to public and private data, to copy
/// the backup there. It never reads from the mirror.
pub(super) fn mirror_capabilities() -> Capabilities {
    Capabilities::builder()
        .write(PUBLIC_PATH)
        .and_then(|builder| builder.write(PRIVATE_PATH))
        .expect("the data paths are valid scopes")
        .finish()
}

/// Start a request for the key's owner to approve the given capabilities.
///
/// The returned flow carries the link to show to the signer app and starts
/// listening for the approval right away. `client_id` names this app to the
/// owner, who sees it among the apps they granted access to.
pub(super) fn start_approval_flow(
    pubky_client: &Pubky,
    client_id: &ClientId,
    http_relay: Option<&Url>,
    capabilities: &Capabilities,
) -> Result<ApprovalFlow, OrchestratorError> {
    let mut builder =
        PubkyGrantAuthFlow::builder(capabilities, AuthFlowKind::signin(), client_id.clone())
            .client(pubky_client.client().clone());
    if let Some(relay) = http_relay {
        builder = builder.relay(relay.clone());
    }

    builder
        .start()
        .map_err(|e| OrchestratorError::AuthFailed(e.to_string()))
}

/// Wait for a request to be approved, by any key, and return its session on
/// the key's own homeserver.
///
/// # Errors
///
/// Returns `OrchestratorError::AuthFailed` if the request expires, or if the
/// owner approved it without all of the `requested` access.
pub(super) async fn await_any_approval(
    flow: ApprovalFlow,
    requested: &Capabilities,
) -> Result<PubkySession, OrchestratorError> {
    let session = flow
        .await_approval()
        .await
        .map_err(|e| OrchestratorError::AuthFailed(e.to_string()))?;

    // Signer apps may approve fewer capabilities than were asked for
    if let Some((scope, action)) = missing_access(&session, requested) {
        end_session(session);
        return Err(OrchestratorError::AuthFailed(format!(
            "{} access to {} was not approved",
            describe_action(action),
            scope
        )));
    }
    Ok(session)
}

/// The first of the `requested` accesses that `session` does not have, if any.
fn missing_access<'a>(
    session: &PubkySession,
    requested: &'a Capabilities,
) -> Option<(&'a StoragePath, Action)> {
    let granted = session.info();
    requested
        .iter()
        .flat_map(|capability| {
            capability
                .actions()
                .iter()
                .map(move |action| (capability.scope(), *action))
        })
        .find(|(scope, action)| {
            !granted.capabilities().iter().any(|capability| {
                capability.scope_covers_path(scope) && capability.actions().contains(action)
            })
        })
}

fn describe_action(action: Action) -> &'static str {
    match action {
        Action::Read => "Read",
        Action::Write => "Write",
        _ => "The requested",
    }
}

/// Wait for a request to be approved by `pubky` and return its session on the
/// key's own homeserver.
///
/// # Errors
///
/// Returns `OrchestratorError::AuthFailed` if the request expires, or if it is
/// approved by a different key than `pubky` or without all of the `requested`
/// access.
pub(super) async fn await_approval(
    flow: ApprovalFlow,
    pubky: &PublicKey,
    requested: &Capabilities,
) -> Result<PubkySession, OrchestratorError> {
    let session = await_any_approval(flow, requested).await?;

    let approved_by = session.public_key();
    if approved_by != *pubky {
        // Don't leave a session we won't use open on the other key's homeserver
        end_session(session);
        return Err(OrchestratorError::AuthFailed(format!(
            "The request was approved by a different key ({})",
            approved_by.z32()
        )));
    }

    Ok(session)
}

/// The secret a session is stored as, which restores it with
/// [`Pubky::restore_session`].
pub(super) async fn session_secret(session: &PubkySession) -> Result<String, OrchestratorError> {
    let secret = match session.as_grant() {
        Some(grant) => grant.export_local_secret().await,
        None => None,
    };
    secret.ok_or_else(|| OrchestratorError::AuthFailed("The session cannot be stored".to_string()))
}

/// The error for a request that was cancelled before it could complete.
pub(super) fn approval_cancelled() -> OrchestratorError {
    OrchestratorError::AuthFailed("The request was cancelled".to_string())
}

/// The reason to show for a request that failed.
pub(super) fn failure_reason(error: &OrchestratorError) -> String {
    match error {
        OrchestratorError::AuthFailed(reason) | OrchestratorError::MirrorFailed(reason) => {
            reason.clone()
        }
        other => other.to_string(),
    }
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
///
/// The secret names the homeserver the session is on, so this works for a
/// session on a mirror as well as on the key's own homeserver.
pub(super) fn end_stored_session(secret: String, pubky: PublicKey, pubky_client: Pubky) {
    tokio::spawn(async move {
        match pubky_client.restore_session(&secret).await {
            Ok(session) => end_session(session),
            Err(e) => warn!("Failed to end session of {}: {}", pubky, e),
        }
    });
}
