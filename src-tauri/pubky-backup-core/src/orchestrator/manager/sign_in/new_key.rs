//! Signing in to a key that may not be backed up yet.
//!
//! A `pubkyauth://` request doesn't name the key that is to approve it. So an
//! owner can start backing up their key, public and private data alike, just by
//! approving a sign-in in their signer app, without typing out their pubky.

use log::{info, warn};
use pubky::PublicKey;
use tokio::sync::oneshot;

use super::{replace_pending_sign_in, BackupManager, PendingSignIn, SignInTarget};
use crate::orchestrator::auth::{self, SignInFlow, SignInRequest};
use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::types::KeyStatus;

impl BackupManager {
    /// Start a sign-in that backs up whichever key approves it, public and
    /// private data alike.
    ///
    /// Like [`BackupManager::start_sign_in`], but for a key that need not be
    /// backed up yet: the owner doesn't have to enter their pubky, only approve
    /// the returned request in their signer app (e.g. Pubky Ring). The approving
    /// key is added if it is not backed up yet, then signed in, replacing any
    /// sign-in it had. [`SignInRequest::approved`] returns that key.
    ///
    /// Only one such sign-in is pending at a time: starting another replaces it.
    /// Until it is approved there is no key to report its status on, so follow
    /// it through the returned request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pubky_backup_core::{BackupManager, OrchestratorError};
    /// # async fn example(manager: BackupManager) -> Result<(), OrchestratorError> {
    /// let request = manager.start_sign_in_for_new_key().await?;
    /// println!("Approve in Pubky Ring: {}", request.authorization_url());
    /// let pubky = request.approved().await?;
    /// println!("Backing up {}", pubky);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::AuthFailed` if the request cannot be started.
    /// Once approved, the request fails with the error that kept the key from
    /// being added, e.g. `OrchestratorError::HomeserverNotFound`.
    pub async fn start_sign_in_for_new_key(
        &self,
    ) -> Result<SignInRequest<PublicKey>, OrchestratorError> {
        let flow = self.start_sign_in_flow()?;
        let authorization_url = flow.authorization_url().to_string();

        let (outcome_tx, outcome_rx) = oneshot::channel();
        let id = PendingSignIn::next_id();
        let manager = self.clone();
        let wait_for_approval = async move {
            let outcome = manager.finish_sign_in_for_new_key(id, flow).await;
            // Nobody is listening if the caller dropped the request
            let _ = outcome_tx.send(outcome);
        };

        {
            // Spawning under the lock keeps the task from claiming its approval
            // before it is registered
            let mut inner = self.inner.write();
            let task = tokio::spawn(wait_for_approval);
            replace_pending_sign_in(&mut inner.new_key_sign_in, Some(PendingSignIn { id, task }));
        }

        info!("Started sign-in for a new key");
        Ok(SignInRequest::new(authorization_url, outcome_rx))
    }

    /// Cancel the sign-in started by [`BackupManager::start_sign_in_for_new_key`],
    /// if it is still awaiting approval.
    pub fn cancel_sign_in_for_new_key(&self) {
        let cancelled = replace_pending_sign_in(&mut self.inner.write().new_key_sign_in, None);
        if cancelled {
            info!("Cancelled sign-in for a new key");
        }
    }

    /// Wait for a new-key sign-in to be approved, then back up and sign in the
    /// key that approved it.
    async fn finish_sign_in_for_new_key(
        &self,
        id: u64,
        flow: SignInFlow,
    ) -> Result<PublicKey, OrchestratorError> {
        let approval = auth::await_any_approval(flow).await;
        if !self.claim_sign_in(SignInTarget::NewKey, id) {
            if let Ok(session) = approval {
                auth::end_session(session);
            }
            return Err(auth::sign_in_cancelled());
        }

        let session = approval.inspect_err(|e| warn!("Sign-in for a new key failed: {}", e))?;
        let pubky = session.public_key();
        if let Err(e) = self.ensure_backed_up(&pubky).await {
            warn!("Failed to back up {} after sign-in: {}", pubky, e);
            // Don't leave a session nobody will use open on the homeserver
            auth::end_session(session);
            return Err(e);
        }

        self.complete_sign_in(&pubky, session).await?;
        Ok(pubky)
    }

    /// Back up `pubky` if it isn't yet, or retry it if it is in error.
    async fn ensure_backed_up(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        match self.get_key_state(pubky) {
            None => match self.add_key(pubky.clone()).await {
                // Added in the meantime by someone else
                Ok(()) | Err(OrchestratorError::KeyAlreadyExists(_)) => Ok(()),
                Err(e) => Err(e),
            },
            Some(state) if state.status == KeyStatus::Error => self.retry_errored_key(pubky).await,
            Some(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::setup;
    use super::*;

    /// Register a pending new-key sign-in whose task never finishes.
    fn register_pending_sign_in(manager: &BackupManager) -> u64 {
        let id = PendingSignIn::next_id();
        let task = tokio::spawn(std::future::pending());
        replace_pending_sign_in(
            &mut manager.inner.write().new_key_sign_in,
            Some(PendingSignIn { id, task }),
        );
        id
    }

    #[tokio::test]
    async fn test_start_sign_in_for_new_key_unavailable_in_developer_mode() {
        let (_dir, manager) = setup().await;

        let result = manager.start_sign_in_for_new_key().await;

        assert!(matches!(result, Err(OrchestratorError::AuthFailed(_))));
        assert!(manager.inner.read().new_key_sign_in.is_none());
    }

    #[tokio::test]
    async fn test_pending_sign_in_for_new_key_can_be_claimed_once() {
        let (_dir, manager) = setup().await;
        let id = register_pending_sign_in(&manager);

        assert!(manager.claim_sign_in(SignInTarget::NewKey, id));
        assert!(!manager.claim_sign_in(SignInTarget::NewKey, id));
    }

    #[tokio::test]
    async fn test_cancelled_sign_in_for_new_key_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let id = register_pending_sign_in(&manager);

        manager.cancel_sign_in_for_new_key();

        assert!(!manager.claim_sign_in(SignInTarget::NewKey, id));
    }

    #[tokio::test]
    async fn test_replaced_sign_in_for_new_key_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let replaced = register_pending_sign_in(&manager);
        let current = register_pending_sign_in(&manager);

        assert!(!manager.claim_sign_in(SignInTarget::NewKey, replaced));
        assert!(manager.claim_sign_in(SignInTarget::NewKey, current));
    }

    #[tokio::test]
    async fn test_cancel_sign_in_for_new_key_without_one_does_nothing() {
        let (_dir, manager) = setup().await;

        manager.cancel_sign_in_for_new_key();

        assert!(manager.inner.read().new_key_sign_in.is_none());
    }

    #[tokio::test]
    async fn test_no_key_is_added_after_shutdown() {
        let (_dir, manager) = setup().await;
        manager.shutdown().await;

        let result = manager
            .add_key(super::super::super::tests::test_pubky())
            .await;

        assert!(result.is_err());
        assert!(manager.get_keys().is_empty());
    }
}
