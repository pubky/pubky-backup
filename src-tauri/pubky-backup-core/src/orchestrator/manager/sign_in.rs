//! Signing keys in and out of a [`BackupManager`].
//!
//! The steps that talk to the signer app and the homeserver live in
//! [`auth`](crate::orchestrator::auth). This module keeps each key's
//! [`AuthStatus`] and stored session in step with them.
//!
//! A key's status, its pending sign-in and its stored session only change
//! together, under the manager's state lock (see [`BackupManager::update_key`]).
//! Otherwise a sign-in approved in the same instant as it is cancelled could
//! leave a session stored, and private data syncing, for a key that shows as
//! signed out.
//!
//! A pending sign-in waits for its approval in a background task. Once the
//! approval arrives, the task claims the sign-in, which takes it out of the
//! manager: from then on it can no longer be cancelled or replaced, so it is
//! never cut off half done.

use std::sync::atomic::{AtomicU64, Ordering};

use log::{info, warn};
use pubky::{PubkySession, PublicKey};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::status::log_activity;
use super::{BackupManager, ManagedKey, ManagerInner};
use crate::orchestrator::auth::{self, SignInFlow, SignInRequest};
use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::types::{ActivityType, AuthStatus, KeyState, KeyUpdate};
use crate::storage::AppStorage;
use crate::sync::SessionSecret;

mod new_key;

/// Source of the ids that tell sign-ins apart.
static NEXT_SIGN_IN_ID: AtomicU64 = AtomicU64::new(0);

/// A sign-in that awaits approval in a background task.
pub(crate) struct PendingSignIn {
    /// Tells this sign-in apart from the ones that replace it
    id: u64,
    /// The task waiting for the approval
    task: JoinHandle<()>,
}

impl PendingSignIn {
    fn next_id() -> u64 {
        NEXT_SIGN_IN_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// Stop waiting for the approval.
    pub(super) fn cancel(self) {
        self.task.abort();
    }
}

/// Put `next` in place of the pending sign-in in `slot`, cancelling that one.
///
/// Returns whether there was one to cancel.
fn replace_pending_sign_in(slot: &mut Option<PendingSignIn>, next: Option<PendingSignIn>) -> bool {
    let replaced = std::mem::replace(slot, next);
    let had_one = replaced.is_some();
    if let Some(replaced) = replaced {
        replaced.cancel();
    }
    had_one
}

/// What a sign-in is for, which decides where it is kept while pending.
#[derive(Clone, Copy)]
enum SignInTarget<'a> {
    /// A key that is backed up, which keeps its own pending sign-in
    Key(&'a PublicKey),
    /// Whichever key approves it, kept by the manager
    NewKey,
}

impl ManagerInner {
    /// Where the pending sign-in for `target` is kept, or `None` if its key is
    /// not tracked.
    fn pending_sign_in(&mut self, target: SignInTarget<'_>) -> Option<&mut Option<PendingSignIn>> {
        match target {
            SignInTarget::Key(pubky) => self.keys.get_mut(pubky).map(|k| &mut k.pending_sign_in),
            SignInTarget::NewKey => Some(&mut self.new_key_sign_in),
        }
    }

    /// Mark a key's session as expired after the homeserver rejected it, and
    /// forget the stored session.
    ///
    /// Does nothing if `rejected` is no longer the key's stored session: the key
    /// may have signed out, or signed in again, since. Returns the key's new
    /// state if its status changed.
    pub(crate) fn expire_session(
        &mut self,
        storage: &AppStorage,
        pubky: &PublicKey,
        rejected: &SessionSecret,
    ) -> Option<KeyState> {
        let managed = self.keys.get_mut(pubky)?;
        match storage.read_session_secret(pubky) {
            Ok(Some(stored)) if stored == rejected.as_str() => {}
            Ok(_) => return None,
            Err(e) => {
                warn!("Failed to read session of {}: {}", pubky, e);
                return None;
            }
        }

        if let Err(e) = storage.delete_session_secret(pubky) {
            warn!("Failed to delete rejected session of {}: {}", pubky, e);
        }
        if managed.auth != AuthStatus::SignedIn {
            return None;
        }
        managed.auth = AuthStatus::SessionExpired;
        Some(managed.state())
    }
}

impl BackupManager {
    /// Start signing in to a key, so that its private data is backed up as well.
    ///
    /// The returned request carries a `pubkyauth://` link. Show it to the key's
    /// owner, as a link or QR code, to open with their signer app (e.g. Pubky
    /// Ring), where they approve read access to their private data. The key's
    /// secret never leaves the signer.
    ///
    /// Await [`SignInRequest::approved`] for the outcome, or follow the key's
    /// [`AuthStatus`] through [`BackupManager::subscribe`]: it is
    /// `AwaitingApproval` until the owner approves, then `SignedIn`, or
    /// `SignInFailed` if the request expires or is approved by a different key.
    /// Once signed in, private data is synced right away, and the session is
    /// kept across restarts.
    ///
    /// Starting a new sign-in replaces one that is still awaiting approval.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pubky_backup_core::{BackupManager, OrchestratorError};
    /// # async fn example(manager: BackupManager, pubky: pubky::PublicKey) -> Result<(), OrchestratorError> {
    /// let request = manager.start_sign_in(&pubky).await?;
    /// println!("Approve in Pubky Ring: {}", request.authorization_url());
    /// request.approved().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    /// Returns `OrchestratorError::AlreadySignedIn` if the key is signed in.
    /// Returns `OrchestratorError::AuthFailed` if the request cannot be started.
    pub async fn start_sign_in(
        &self,
        pubky: &PublicKey,
    ) -> Result<SignInRequest, OrchestratorError> {
        if self.auth_status(pubky)? == AuthStatus::SignedIn {
            return Err(OrchestratorError::AlreadySignedIn(pubky.to_string()));
        }
        let flow = self.start_sign_in_flow()?;
        let authorization_url = flow.authorization_url().to_string();

        let (outcome_tx, outcome_rx) = oneshot::channel();
        let id = PendingSignIn::next_id();
        let manager = self.clone();
        let key = pubky.clone();
        let wait_for_approval = async move {
            let outcome = manager.finish_sign_in(&key, id, flow).await;
            // Nobody is listening if the caller follows the key's status instead
            let _ = outcome_tx.send(outcome);
        };

        self.update_key(pubky, |managed| {
            // Checked again here, as the key may have signed in meanwhile
            if managed.auth == AuthStatus::SignedIn {
                return Err(OrchestratorError::AlreadySignedIn(pubky.to_string()));
            }
            managed.auth = AuthStatus::AwaitingApproval {
                authorization_url: authorization_url.clone(),
            };
            let task = tokio::spawn(wait_for_approval);
            replace_pending_sign_in(
                &mut managed.pending_sign_in,
                Some(PendingSignIn { id, task }),
            );
            Ok(())
        })
        .unwrap_or_else(|| Err(OrchestratorError::KeyNotFound(pubky.to_string())))?;

        info!("Started sign-in for key: {}", pubky);
        Ok(SignInRequest::new(authorization_url, outcome_rx))
    }

    /// Cancel a sign-in that is still awaiting approval.
    ///
    /// Does nothing if the key has no pending sign-in, including one whose
    /// approval has just arrived and is being completed.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn cancel_sign_in(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let cancelled = self
            .update_key(pubky, |managed| {
                let cancelled = replace_pending_sign_in(&mut managed.pending_sign_in, None);
                if cancelled {
                    managed.auth = AuthStatus::SignedOut;
                }
                cancelled
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))?;

        if cancelled {
            info!("Cancelled sign-in for key: {}", pubky);
        }
        Ok(())
    }

    /// Sign out of a key, so that only its public data is backed up from now on.
    ///
    /// Private data that was already backed up stays on disk. Also cancels a
    /// sign-in that is still awaiting approval.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn sign_out(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let forgotten = self
            .update_key(pubky, |managed| {
                replace_pending_sign_in(&mut managed.pending_sign_in, None);
                let secret = self.storage.take_session_secret(pubky)?;
                managed.auth = AuthStatus::SignedOut;
                Ok::<_, OrchestratorError>(secret)
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))??;

        let Some(secret) = forgotten else {
            return Ok(());
        };
        self.end_session_on_homeserver(secret, pubky);
        log_activity(
            &self.storage,
            pubky,
            ActivityType::SignedOut,
            "Signed out — only public data is backed up".to_string(),
        )
        .await;
        info!("Signed out of key: {}", pubky);
        Ok(())
    }

    /// The sign-in status a key starts with: signed in if it has a stored session.
    ///
    /// The controller validates the session on its first sync and reports it
    /// if the homeserver no longer accepts it.
    pub(super) fn stored_auth_status(&self, pubky: &PublicKey) -> AuthStatus {
        match self.storage.read_session_secret(pubky) {
            Ok(Some(_)) => AuthStatus::SignedIn,
            Ok(None) => AuthStatus::SignedOut,
            Err(e) => {
                warn!("Failed to read session of {}: {}", pubky, e);
                AuthStatus::SignedOut
            }
        }
    }

    /// Forget the session of a key that was just removed from the manager, and
    /// end it on its homeserver.
    ///
    /// Failures are logged, not returned, so they don't keep the key from being removed.
    pub(super) fn discard_session_of_removed_key(&self, pubky: &PublicKey) {
        match self.storage.take_session_secret(pubky) {
            Ok(Some(secret)) => self.end_session_on_homeserver(secret, pubky),
            Ok(None) => {}
            Err(e) => warn!("Failed to sign out of {}: {}", pubky, e),
        }
    }

    /// Start a sign-in request with the signer app, asking for read access to
    /// private data.
    fn start_sign_in_flow(&self) -> Result<SignInFlow, OrchestratorError> {
        if self.config.offline {
            return Err(OrchestratorError::AuthFailed(
                "Signing in needs network access, which this backup is configured without"
                    .to_string(),
            ));
        }
        auth::start_sign_in_flow(
            &self.pubky_client,
            &self.config.client_id,
            self.config.http_relay.as_ref(),
        )
    }

    /// Wait for a key's sign-in to be approved, then sign the key in.
    async fn finish_sign_in(
        &self,
        pubky: &PublicKey,
        id: u64,
        flow: SignInFlow,
    ) -> Result<(), OrchestratorError> {
        let approval = auth::await_approval(flow, pubky).await;
        if !self.claim_sign_in(SignInTarget::Key(pubky), id) {
            if let Ok(session) = approval {
                auth::end_session(session);
            }
            return Err(auth::sign_in_cancelled());
        }

        match approval {
            Ok(session) => self.complete_sign_in(pubky, session).await,
            Err(e) => {
                warn!("Sign-in failed for {}: {}", pubky, e);
                self.update_key(pubky, |managed| {
                    managed.auth = AuthStatus::SignInFailed {
                        message: failure_reason(&e),
                    };
                });
                Err(e)
            }
        }
    }

    /// Take the pending sign-in with this `id` out of the manager, so that it
    /// can no longer be cancelled or replaced.
    ///
    /// Returns `false` if it already was cancelled or replaced, or its key removed.
    fn claim_sign_in(&self, target: SignInTarget<'_>, id: u64) -> bool {
        let mut inner = self.inner.write();
        let Some(slot) = inner.pending_sign_in(target) else {
            return false;
        };
        if slot.as_ref().is_none_or(|pending| pending.id != id) {
            return false;
        }
        // Dropping the handle leaves the claiming task running
        *slot = None;
        true
    }

    /// Sign a backed-up key in with an approved session, then start backing up
    /// its private data.
    async fn complete_sign_in(
        &self,
        pubky: &PublicKey,
        session: PubkySession,
    ) -> Result<(), OrchestratorError> {
        if let Err(e) = self.store_approved_session(pubky, &session).await {
            // Don't leave a session nobody will use open on the homeserver
            auth::end_session(session);
            return Err(e);
        }

        info!("Signed in to key: {}", pubky);
        log_activity(
            &self.storage,
            pubky,
            ActivityType::SignedIn,
            "Signed in — private data is backed up too".to_string(),
        )
        .await;
        if let Err(e) = self.force_sync(pubky).await {
            warn!("Failed to sync {} after sign-in: {}", pubky, e);
        }
        Ok(())
    }

    /// Store the secret of a key's approved session and mark the key signed in.
    async fn store_approved_session(
        &self,
        pubky: &PublicKey,
        session: &PubkySession,
    ) -> Result<(), OrchestratorError> {
        let secret = auth::session_secret(session).await?;
        self.store_session(pubky, &secret)
    }

    /// Store a key's session secret and mark the key signed in.
    ///
    /// The session replaces any sign-in the key had, including one still
    /// awaiting approval, and the replaced session is ended.
    fn store_session(&self, pubky: &PublicKey, secret: &str) -> Result<(), OrchestratorError> {
        let replaced = self
            .update_key(pubky, |managed| {
                replace_pending_sign_in(&mut managed.pending_sign_in, None);
                let replaced = self.storage.read_session_secret(pubky)?;
                if let Err(e) = self.storage.write_session_secret(pubky, secret) {
                    managed.auth = AuthStatus::SignInFailed {
                        message: e.to_string(),
                    };
                    return Err(e.into());
                }
                managed.auth = AuthStatus::SignedIn;
                Ok::<_, OrchestratorError>(replaced)
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))??;

        if let Some(replaced) = replaced.filter(|replaced| replaced != secret) {
            self.end_session_on_homeserver(replaced, pubky);
        }
        Ok(())
    }

    /// Get a key's sign-in status.
    fn auth_status(&self, pubky: &PublicKey) -> Result<AuthStatus, OrchestratorError> {
        self.inner
            .read()
            .keys
            .get(pubky)
            .map(|managed| managed.auth.clone())
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))
    }

    /// Run `update` on a tracked key under the state lock, then broadcast the
    /// key's state if its sign-in status changed.
    ///
    /// Changes to the stored session belong inside `update`, so that they can't
    /// interleave with another change of status. `update` must not use the
    /// manager's state itself. Returns `None` if the key is not tracked.
    fn update_key<R>(
        &self,
        pubky: &PublicKey,
        update: impl FnOnce(&mut ManagedKey) -> R,
    ) -> Option<R> {
        let (result, changed_state) = {
            let mut inner = self.inner.write();
            let managed = inner.keys.get_mut(pubky)?;
            let previous = managed.auth.clone();
            let result = update(managed);
            let changed_state = (managed.auth != previous).then(|| managed.state());
            (result, changed_state)
        };

        if let Some(state) = changed_state {
            let _ = self.update_tx.send(KeyUpdate {
                pubky: pubky.clone(),
                state,
            });
        }
        Some(result)
    }

    /// End a session that was already forgotten locally on its homeserver.
    fn end_session_on_homeserver(&self, secret: String, pubky: &PublicKey) {
        if self.config.offline {
            return;
        }
        auth::end_stored_session(secret, pubky.clone(), self.pubky_client.as_ref().clone());
    }
}

/// The reason to show for a failed sign-in.
fn failure_reason(error: &OrchestratorError) -> String {
    match error {
        OrchestratorError::AuthFailed(reason) => reason.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::status::SyncState;
    use super::super::tests::{setup, setup_with_dir, test_pubky};
    use super::*;
    use tempfile::TempDir;

    /// Add the test key with a session already stored, as if it had signed in.
    async fn add_signed_in_key(manager: &BackupManager) -> PublicKey {
        let pubky = test_pubky();
        manager
            .storage
            .write_session_secret(&pubky, "secret")
            .unwrap();
        manager.add_key(pubky.clone()).await.unwrap();
        pubky
    }

    /// Add the test key and put it in the state of awaiting a sign-in approval,
    /// with a task that never finishes. Returns the key and the sign-in's id.
    async fn add_key_awaiting_approval(manager: &BackupManager) -> (PublicKey, u64) {
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let id = register_pending_sign_in(manager, &pubky);
        (pubky, id)
    }

    /// Register a pending sign-in for a tracked key, as `start_sign_in` would.
    fn register_pending_sign_in(manager: &BackupManager, pubky: &PublicKey) -> u64 {
        let id = PendingSignIn::next_id();
        manager.update_key(pubky, |managed| {
            managed.auth = AuthStatus::AwaitingApproval {
                authorization_url: format!("pubkyauth://example/{id}"),
            };
            let task = tokio::spawn(std::future::pending());
            replace_pending_sign_in(
                &mut managed.pending_sign_in,
                Some(PendingSignIn { id, task }),
            );
        });
        id
    }

    fn stored_secret(manager: &BackupManager, pubky: &PublicKey) -> Option<String> {
        manager.storage.read_session_secret(pubky).unwrap()
    }

    #[tokio::test]
    async fn test_new_key_is_signed_out() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();

        manager.add_key(pubky.clone()).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
    }

    #[tokio::test]
    async fn test_key_with_stored_session_resumes_signed_in() {
        let temp_dir = TempDir::new().unwrap();
        let pubky = {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let pubky = add_signed_in_key(&manager).await;
            manager.shutdown().await;
            pubky
        };

        let (_, manager) = setup_with_dir(&temp_dir).await;

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
    }

    // --- Starting a sign-in ---

    #[tokio::test]
    async fn test_start_sign_in_unavailable_offline() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        let result = manager.start_sign_in(&pubky).await;

        assert!(matches!(result, Err(OrchestratorError::AuthFailed(_))));
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
    }

    #[tokio::test]
    async fn test_start_sign_in_rejects_signed_in_key() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;

        let result = manager.start_sign_in(&pubky).await;

        assert!(matches!(result, Err(OrchestratorError::AlreadySignedIn(_))));
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
    }

    #[tokio::test]
    async fn test_sign_in_methods_reject_unknown_key() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();

        assert!(matches!(
            manager.start_sign_in(&pubky).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
        assert!(matches!(
            manager.cancel_sign_in(&pubky).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
        assert!(matches!(
            manager.sign_out(&pubky).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
    }

    // --- Claiming an approved sign-in ---

    #[tokio::test]
    async fn test_pending_sign_in_can_be_claimed_once() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_approval(&manager).await;

        assert!(manager.claim_sign_in(SignInTarget::Key(&pubky), id));
        assert!(!manager.claim_sign_in(SignInTarget::Key(&pubky), id));
    }

    #[tokio::test]
    async fn test_cancelled_sign_in_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_approval(&manager).await;

        manager.cancel_sign_in(&pubky).await.unwrap();

        assert!(!manager.claim_sign_in(SignInTarget::Key(&pubky), id));
    }

    #[tokio::test]
    async fn test_replaced_sign_in_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, replaced) = add_key_awaiting_approval(&manager).await;
        let current = register_pending_sign_in(&manager, &pubky);

        assert!(!manager.claim_sign_in(SignInTarget::Key(&pubky), replaced));
        assert!(manager.claim_sign_in(SignInTarget::Key(&pubky), current));
    }

    #[tokio::test]
    async fn test_sign_in_of_removed_key_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_approval(&manager).await;

        manager.remove_key(&pubky).await.unwrap();

        assert!(!manager.claim_sign_in(SignInTarget::Key(&pubky), id));
    }

    #[tokio::test]
    async fn test_cancel_after_approval_is_claimed_does_nothing() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_approval(&manager).await;
        assert!(manager.claim_sign_in(SignInTarget::Key(&pubky), id));
        let completing = manager.auth_status(&pubky).unwrap();

        manager.cancel_sign_in(&pubky).await.unwrap();

        // The sign-in being completed is left to finish
        assert_eq!(manager.auth_status(&pubky).unwrap(), completing);
    }

    // --- Cancelling and signing out ---

    #[tokio::test]
    async fn test_cancel_sign_in_returns_pending_key_to_signed_out() {
        let (_dir, manager) = setup().await;
        let (pubky, _id) = add_key_awaiting_approval(&manager).await;
        let mut rx = manager.subscribe();

        manager.cancel_sign_in(&pubky).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
        // Subscribers are told about the change
        assert_eq!(rx.try_recv().unwrap().state.auth, AuthStatus::SignedOut);
    }

    #[tokio::test]
    async fn test_cancel_sign_in_leaves_signed_in_key_alone() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;

        manager.cancel_sign_in(&pubky).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
        assert!(stored_secret(&manager, &pubky).is_some());
    }

    #[tokio::test]
    async fn test_sign_out_forgets_session_and_logs_activity() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;
        let mut rx = manager.subscribe();

        manager.sign_out(&pubky).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
        assert_eq!(stored_secret(&manager, &pubky), None);

        let entries = manager.get_activity(&pubky, 10).await;
        assert!(entries
            .iter()
            .any(|e| e.activity_type == ActivityType::SignedOut));

        // Subscribers are told about the change
        let mut signed_out_broadcast = false;
        while let Ok(update) = rx.try_recv() {
            signed_out_broadcast |= update.state.auth == AuthStatus::SignedOut;
        }
        assert!(signed_out_broadcast);
    }

    #[tokio::test]
    async fn test_sign_out_cancels_pending_sign_in() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_approval(&manager).await;

        manager.sign_out(&pubky).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
        assert!(!manager.claim_sign_in(SignInTarget::Key(&pubky), id));
    }

    #[tokio::test]
    async fn test_sign_out_of_signed_out_key_logs_no_activity() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        manager.sign_out(&pubky).await.unwrap();

        let entries = manager.get_activity(&pubky, 10).await;
        assert!(!entries
            .iter()
            .any(|e| e.activity_type == ActivityType::SignedOut));
    }

    #[tokio::test]
    async fn test_removing_key_signs_it_out() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;

        manager.remove_key(&pubky).await.unwrap();

        assert_eq!(stored_secret(&manager, &pubky), None);
    }

    // --- Controller updates ---

    #[tokio::test]
    async fn test_controller_updates_keep_auth_status() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;

        // Controllers don't know the sign-in status, so their states carry the default
        let applied = manager
            .inner
            .write()
            .apply_sync_state(&pubky, SyncState::default())
            .unwrap();

        assert_eq!(applied.auth, AuthStatus::SignedIn);
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
    }

    #[tokio::test]
    async fn test_rejected_session_expires_signed_in_key() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;
        let rejected = SessionSecret::new("secret".to_string());

        let state = manager
            .inner
            .write()
            .expire_session(&manager.storage, &pubky, &rejected);

        assert_eq!(state.unwrap().auth, AuthStatus::SessionExpired);
        assert_eq!(
            manager.auth_status(&pubky).unwrap(),
            AuthStatus::SessionExpired
        );
        assert_eq!(stored_secret(&manager, &pubky), None);
    }

    #[tokio::test]
    async fn test_rejection_of_replaced_session_is_ignored() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;
        // The key signed in again while a sync was using an older session
        let rejected = SessionSecret::new("older".to_string());

        let state = manager
            .inner
            .write()
            .expire_session(&manager.storage, &pubky, &rejected);

        assert!(state.is_none());
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
        assert_eq!(stored_secret(&manager, &pubky).as_deref(), Some("secret"));
    }
}
