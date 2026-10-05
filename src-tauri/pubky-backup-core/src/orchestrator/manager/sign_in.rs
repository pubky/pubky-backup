//! Signing keys in and out of a [`BackupManager`].
//!
//! The steps that talk to the signer app and the homeserver live in
//! [`auth`](crate::orchestrator::auth). This module keeps each key's
//! [`AuthStatus`] and stored session in step with them.
//!
//! A key's status and its stored session only change together, under the
//! manager's state lock (see [`BackupManager::update_auth`]). Otherwise a
//! sign-in approved in the same instant as it is cancelled could leave a
//! session stored, and private data syncing, for a key that shows as signed out.

use log::{info, warn};
use pubky::{PubkySession, PublicKey};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::{BackupManager, ManagerInner};
use crate::orchestrator::auth::{self, SignInFlow, SignInRequest};
use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::types::{ActivityType, AuthStatus, KeyUpdate};

impl ManagerInner {
    /// Mark a signed-in key's session as expired.
    ///
    /// Leaves other sign-in statuses alone, so a late rejection of an old
    /// session doesn't interrupt a new sign-in.
    pub(crate) fn expire_session(&mut self, pubky: &PublicKey) {
        if let Some(managed) = self.keys.get_mut(pubky) {
            if managed.state.auth == AuthStatus::SignedIn {
                managed.state.auth = AuthStatus::SessionExpired;
            }
        }
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
        if self.config.developer_mode {
            return Err(OrchestratorError::AuthFailed(
                "Signing in needs network access, which developer mode disables".to_string(),
            ));
        }

        let pubky_client = self.inner.read().pubky_client.clone();
        let flow = auth::start_sign_in_flow(&pubky_client, self.config.http_relay.as_ref())?;
        let authorization_url = flow.authorization_url().to_string();
        let pending = AuthStatus::AwaitingApproval {
            authorization_url: authorization_url.clone(),
        };

        self.update_auth(pubky, |auth| *auth = pending.clone());
        let outcome = self.spawn_sign_in_task(pubky, flow, pending);

        info!("Started sign-in for key: {}", pubky);
        Ok(SignInRequest::new(authorization_url, outcome))
    }

    /// Cancel a sign-in that is still awaiting approval.
    ///
    /// Does nothing if the key has no pending sign-in.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn cancel_sign_in(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let auth = self.auth_status(pubky)?;
        if !matches!(auth, AuthStatus::AwaitingApproval { .. }) {
            return Ok(());
        }

        self.end_session(pubky)?;
        info!("Cancelled sign-in for key: {}", pubky);
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
        self.auth_status(pubky)?;

        let was_signed_in = self.end_session(pubky)?;
        if was_signed_in {
            self.write_activity(
                pubky,
                ActivityType::SignedOut,
                "Signed out — only public data is backed up".to_string(),
            )
            .await;
            info!("Signed out of key: {}", pubky);
        }
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

    /// Get a key's sign-in status.
    fn auth_status(&self, pubky: &PublicKey) -> Result<AuthStatus, OrchestratorError> {
        self.get_key_state(pubky)
            .map(|state| state.auth)
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))
    }

    /// Run `update` on a key's sign-in status under the state lock, then
    /// broadcast the key's state if the status changed.
    ///
    /// Changes to the stored session belong inside `update`, so that they can't
    /// interleave with another change of status. `update` must not use the
    /// manager's state itself. Returns `None` if the key is not tracked.
    fn update_auth<R>(
        &self,
        pubky: &PublicKey,
        update: impl FnOnce(&mut AuthStatus) -> R,
    ) -> Option<R> {
        let (result, changed_state) = {
            let mut inner = self.inner.write();
            let managed_key = inner.keys.get_mut(pubky)?;
            let previous = managed_key.state.auth.clone();
            let result = update(&mut managed_key.state.auth);
            let changed_state =
                (managed_key.state.auth != previous).then(|| managed_key.state.clone());
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

    /// Wait for a sign-in to be approved in the background.
    ///
    /// Returns a receiver for the outcome, for callers that want to await it
    /// rather than follow the key's sign-in status.
    fn spawn_sign_in_task(
        &self,
        pubky: &PublicKey,
        flow: SignInFlow,
        pending: AuthStatus,
    ) -> oneshot::Receiver<Result<(), OrchestratorError>> {
        let (outcome_tx, outcome_rx) = oneshot::channel();
        let manager = self.clone();
        let key = pubky.clone();

        let sign_in_task = tokio::spawn(async move {
            let approval = auth::await_approval(flow, &key).await;
            let outcome = manager.finish_sign_in(&key, &pending, approval).await;
            // Nobody is listening if the caller follows the key's status instead
            let _ = outcome_tx.send(outcome);
        });
        self.replace_sign_in_task(pubky, Some(sign_in_task));

        outcome_rx
    }

    /// Replace the task waiting for a key's sign-in approval, cancelling the previous one.
    fn replace_sign_in_task(&self, pubky: &PublicKey, sign_in_task: Option<JoinHandle<()>>) {
        let mut inner = self.inner.write();
        match inner.keys.get_mut(pubky) {
            Some(managed_key) => {
                let previous = std::mem::replace(&mut managed_key.sign_in_task, sign_in_task);
                if let Some(previous) = previous {
                    previous.abort();
                }
            }
            // The key was removed in the meantime, so nobody is waiting for the outcome
            None => {
                if let Some(sign_in_task) = sign_in_task {
                    sign_in_task.abort();
                }
            }
        }
    }

    /// Record the outcome of the sign-in a key was `pending` on and, on success,
    /// start backing up its private data.
    async fn finish_sign_in(
        &self,
        pubky: &PublicKey,
        pending: &AuthStatus,
        approval: Result<PubkySession, OrchestratorError>,
    ) -> Result<(), OrchestratorError> {
        let session = match approval {
            Ok(session) => session,
            Err(e) => {
                warn!("Sign-in failed for {}: {}", pubky, e);
                self.update_auth(pubky, |auth| {
                    if auth == pending {
                        *auth = AuthStatus::SignInFailed {
                            message: failure_reason(&e),
                        };
                    }
                });
                return Err(e);
            }
        };

        let signed_in = auth::session_secret(&session)
            .and_then(|secret| self.sign_in_if_pending(pubky, pending, &secret));
        if let Err(e) = signed_in {
            // Don't leave a session nobody will use open on the homeserver
            auth::end_session(session);
            return Err(e);
        }

        info!("Signed in to key: {}", pubky);
        self.write_activity(
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

    /// Store a session and mark its key signed in, provided the key is still
    /// `pending` on the sign-in that produced the session.
    ///
    /// It no longer is if that sign-in was cancelled or replaced, or the key
    /// removed, just as the approval arrived.
    fn sign_in_if_pending(
        &self,
        pubky: &PublicKey,
        pending: &AuthStatus,
        secret: &str,
    ) -> Result<(), OrchestratorError> {
        self.update_auth(pubky, |auth| {
            if auth != pending {
                return Err(auth::sign_in_cancelled());
            }
            if let Err(e) = self.storage.write_session_secret(pubky, secret) {
                *auth = AuthStatus::SignInFailed {
                    message: e.to_string(),
                };
                return Err(e.into());
            }
            *auth = AuthStatus::SignedIn;
            Ok(())
        })
        .unwrap_or_else(|| Err(auth::sign_in_cancelled()))
    }

    /// Cancel a pending sign-in, forget the key's session and end it on its
    /// homeserver, leaving the key signed out.
    ///
    /// Returns whether the key had a session.
    fn end_session(&self, pubky: &PublicKey) -> Result<bool, OrchestratorError> {
        self.replace_sign_in_task(pubky, None);

        let forgotten = self
            .update_auth(pubky, |auth| {
                let secret = self.storage.take_session_secret(pubky)?;
                *auth = AuthStatus::SignedOut;
                Ok::<_, OrchestratorError>(secret)
            })
            .transpose()?
            .flatten();

        match forgotten {
            Some(secret) => {
                self.end_session_on_homeserver(secret, pubky);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// End a session that was already forgotten locally on its homeserver.
    fn end_session_on_homeserver(&self, secret: String, pubky: &PublicKey) {
        // Developer mode makes no network calls
        if self.config.developer_mode {
            return;
        }
        let pubky_client = self.inner.read().pubky_client.as_ref().clone();
        auth::end_stored_session(secret, pubky.clone(), pubky_client);
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
    use super::super::tests::{setup, setup_with_dir, test_pubky};
    use super::*;
    use crate::orchestrator::types::KeyState;
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

    /// Add the test key and put it in the state of awaiting a sign-in approval.
    async fn add_key_awaiting_approval(manager: &BackupManager) -> (PublicKey, AuthStatus) {
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let pending = AuthStatus::AwaitingApproval {
            authorization_url: "pubkyauth://example".to_string(),
        };
        manager.update_auth(&pubky, |auth| *auth = pending.clone());
        (pubky, pending)
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
    async fn test_start_sign_in_unavailable_in_developer_mode() {
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

    // --- Finishing a sign-in ---

    #[tokio::test]
    async fn test_approved_sign_in_stores_session_and_signs_in() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;
        let mut rx = manager.subscribe();

        manager
            .sign_in_if_pending(&pubky, &pending, "secret")
            .unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
        assert_eq!(stored_secret(&manager, &pubky).as_deref(), Some("secret"));
        // Subscribers are told about the change
        assert_eq!(rx.try_recv().unwrap().state.auth, AuthStatus::SignedIn);
    }

    #[tokio::test]
    async fn test_approval_arriving_after_cancel_is_discarded() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;
        manager.cancel_sign_in(&pubky).await.unwrap();

        let result = manager.sign_in_if_pending(&pubky, &pending, "secret");

        assert!(matches!(result, Err(OrchestratorError::AuthFailed(_))));
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
        assert_eq!(stored_secret(&manager, &pubky), None);
    }

    #[tokio::test]
    async fn test_approval_of_replaced_sign_in_is_discarded() {
        let (_dir, manager) = setup().await;
        let (pubky, replaced) = add_key_awaiting_approval(&manager).await;
        let current = AuthStatus::AwaitingApproval {
            authorization_url: "pubkyauth://newer".to_string(),
        };
        manager.update_auth(&pubky, |auth| *auth = current.clone());

        let result = manager.sign_in_if_pending(&pubky, &replaced, "secret");

        assert!(result.is_err());
        assert_eq!(manager.auth_status(&pubky).unwrap(), current);
        assert_eq!(stored_secret(&manager, &pubky), None);
    }

    #[tokio::test]
    async fn test_approval_arriving_after_key_removal_is_discarded() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;
        manager.remove_key(&pubky).await.unwrap();

        let result = manager.sign_in_if_pending(&pubky, &pending, "secret");

        assert!(result.is_err());
        assert_eq!(stored_secret(&manager, &pubky), None);
    }

    #[tokio::test]
    async fn test_failed_sign_in_reports_reason() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;
        let error = OrchestratorError::AuthFailed("request expired".to_string());

        let outcome = manager.finish_sign_in(&pubky, &pending, Err(error)).await;

        assert!(outcome.is_err());
        assert_eq!(
            manager.auth_status(&pubky).unwrap(),
            AuthStatus::SignInFailed {
                message: "request expired".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_failure_of_cancelled_sign_in_is_not_reported() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;
        manager.cancel_sign_in(&pubky).await.unwrap();
        let error = OrchestratorError::AuthFailed("request expired".to_string());

        let _ = manager.finish_sign_in(&pubky, &pending, Err(error)).await;

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
    }

    // --- Cancelling and signing out ---

    #[tokio::test]
    async fn test_cancel_sign_in_returns_pending_key_to_signed_out() {
        let (_dir, manager) = setup().await;
        let (pubky, _pending) = add_key_awaiting_approval(&manager).await;

        manager.cancel_sign_in(&pubky).await.unwrap();

        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedOut);
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
            .apply_controller_state(&pubky, KeyState::default())
            .unwrap();

        assert_eq!(applied.auth, AuthStatus::SignedIn);
        assert_eq!(manager.auth_status(&pubky).unwrap(), AuthStatus::SignedIn);
    }

    #[tokio::test]
    async fn test_rejected_session_expires_signed_in_key() {
        let (_dir, manager) = setup().await;
        let pubky = add_signed_in_key(&manager).await;

        manager.inner.write().expire_session(&pubky);

        assert_eq!(
            manager.auth_status(&pubky).unwrap(),
            AuthStatus::SessionExpired
        );
    }

    #[tokio::test]
    async fn test_rejected_session_does_not_interrupt_new_sign_in() {
        let (_dir, manager) = setup().await;
        let (pubky, pending) = add_key_awaiting_approval(&manager).await;

        // A late rejection of the previous session arrives while signing in again
        manager.inner.write().expire_session(&pubky);

        assert_eq!(manager.auth_status(&pubky).unwrap(), pending);
    }
}
