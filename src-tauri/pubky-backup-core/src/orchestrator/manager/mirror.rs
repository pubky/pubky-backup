//! Mirroring the keys of a [`BackupManager`] to a second homeserver.
//!
//! Setting a mirror up asks the key's owner for write access to it and stores
//! the resulting session; the key's controller then copies what it backs up
//! there (see `sync::controller::mirroring`). This module keeps each key's
//! [`MirrorStatus`] and stored mirror in step with that, the same way
//! [`sign_in`](super::sign_in) does for sign-ins: the status, the pending
//! request and the stored mirror only change together, under the manager's
//! state lock, and an approved request is claimed before it is carried out.
//!
//! The owner's approval is a grant, which is not tied to a homeserver. The
//! backup uses it to open a session on the mirror; the session it comes with
//! on the key's own homeserver is ended right away, since the backup never
//! writes there.

use log::{info, warn};
use pubky::{Capabilities, PubkySession, PublicKey};
use tokio::sync::oneshot;

use super::approval::{replace_pending_approval, ApprovalTarget, PendingApproval};
use super::status::log_activity;
use super::{BackupManager, ManagerInner};
use crate::orchestrator::auth::{self, ApprovalFlow, ApprovalRequest};
use crate::orchestrator::discovery;
use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::types::{ActivityType, KeyState, MirrorStatus};
use crate::storage::{AppStorage, StoredMirror};
use crate::sync::mirror::{self, MirrorSession};
use crate::sync::SessionSecret;

impl ManagerInner {
    /// Mark a key's mirror session as expired after the mirror rejected it,
    /// and forget the session, keeping the mirror.
    ///
    /// Does nothing if `rejected` is no longer the session stored for the
    /// key's mirror: the mirror may have been stopped, or approved again,
    /// since. Returns the key's new state if its status changed.
    pub(crate) fn expire_mirror_session(
        &mut self,
        storage: &AppStorage,
        pubky: &PublicKey,
        rejected: &SessionSecret,
    ) -> Option<KeyState> {
        let managed = self.keys.get_mut(pubky)?;
        let stored = match storage.read_mirror(pubky) {
            Ok(Some(stored)) if stored.session.as_deref() == Some(rejected.as_str()) => stored,
            Ok(_) => return None,
            Err(e) => {
                warn!("Failed to read mirror of {}: {}", pubky, e);
                return None;
            }
        };

        if let Err(e) = storage.write_mirror(pubky, &stored.without_session()) {
            warn!(
                "Failed to forget rejected mirror session of {}: {}",
                pubky, e
            );
        }
        if !managed.mirror.is_active() {
            return None;
        }
        managed.mirror = MirrorStatus::SessionExpired {
            homeserver: stored.homeserver.z32(),
        };
        Some(managed.state())
    }

    /// Apply a controller's report that copying to a key's mirror is failing
    /// (`Some(message)`), or works again (`None`).
    ///
    /// Only a mirror that is being copied to has anything to report, so other
    /// statuses are left alone: a late notice about a mirror that was stopped
    /// or replaced meanwhile must not bring it back. Returns the key's new
    /// state if its status changed.
    pub(crate) fn apply_mirror_failure(
        &mut self,
        pubky: &PublicKey,
        failure: Option<&str>,
    ) -> Option<KeyState> {
        let managed = self.keys.get_mut(pubky)?;
        let (MirrorStatus::Mirroring { homeserver } | MirrorStatus::Failing { homeserver, .. }) =
            &managed.mirror
        else {
            return None;
        };
        let homeserver = homeserver.clone();

        let status = match failure {
            Some(message) => MirrorStatus::Failing {
                homeserver,
                message: message.to_string(),
            },
            None => MirrorStatus::Mirroring { homeserver },
        };
        if managed.mirror == status {
            return None;
        }
        managed.mirror = status;
        Some(managed.state())
    }
}

impl BackupManager {
    /// Start mirroring a key to a second homeserver, so that it holds the same
    /// data as the key's own.
    ///
    /// The returned request carries a `pubkyauth://` link. Show it to the key's
    /// owner, as a link or QR code, to open with their signer app (e.g. Pubky
    /// Ring), where they approve write access to their data. With that, the
    /// backup signs in to the key's account on the mirror, copies everything
    /// backed up so far there, and follows up with every later change,
    /// deletions included. The key's own homeserver stays the same.
    ///
    /// The key must already have an account on the mirror, for example
    /// created through the mirror's own website.
    ///
    /// Private data is mirrored only while the key is signed in (see
    /// [`BackupManager::start_sign_in`]), since that is when it is backed up.
    ///
    /// Await [`ApprovalRequest::approved`] for the outcome, or follow the key's
    /// [`MirrorStatus`] through [`BackupManager::subscribe`]: it is
    /// `AwaitingApproval` until the owner approves, then `Mirroring`, or
    /// `SetupFailed` if the request expires, is approved by a different key,
    /// or the mirror refuses. The mirror is kept across restarts.
    ///
    /// Starting again replaces a request that is still awaiting approval, and
    /// renews the session of a mirror that stopped accepting the old one.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pubky_backup_core::{BackupManager, OrchestratorError};
    /// # use pubky::PublicKey;
    /// # async fn example(manager: BackupManager, pubky: PublicKey, mirror: PublicKey) -> Result<(), OrchestratorError> {
    /// let request = manager.start_mirror(&pubky, &mirror).await?;
    /// println!("Approve in Pubky Ring: {}", request.authorization_url());
    /// request.approved().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    /// Returns `OrchestratorError::AlreadyMirrored` if the key has a working
    /// mirror; stop it first to mirror somewhere else.
    /// Returns `OrchestratorError::MirrorFailed` if `homeserver` is the key's
    /// own homeserver, or the request cannot be started.
    pub async fn start_mirror(
        &self,
        pubky: &PublicKey,
        homeserver: &PublicKey,
    ) -> Result<ApprovalRequest, OrchestratorError> {
        if self.mirror_status(pubky)?.is_active() {
            return Err(OrchestratorError::AlreadyMirrored(pubky.to_string()));
        }
        if self.config.offline {
            return Err(OrchestratorError::MirrorFailed(
                "Mirroring needs network access, which this backup is configured without"
                    .to_string(),
            ));
        }
        self.ensure_not_own_homeserver(pubky, homeserver).await?;

        let capabilities = auth::mirror_capabilities();
        let flow = auth::start_approval_flow(
            &self.pubky_client,
            &self.config.client_id,
            self.config.http_relay.as_ref(),
            &capabilities,
        )?;
        let authorization_url = flow.authorization_url().to_string();

        let (outcome_tx, outcome_rx) = oneshot::channel();
        let id = PendingApproval::next_id();
        let manager = self.clone();
        let key = pubky.clone();
        let mirror = homeserver.clone();
        let wait_for_approval = async move {
            let outcome = manager
                .finish_mirror(&key, &mirror, id, flow, &capabilities)
                .await;
            // Nobody is listening if the caller follows the key's status instead
            let _ = outcome_tx.send(outcome);
        };

        self.update_key(pubky, |managed| {
            // Checked again here, as the mirror may have been set up meanwhile
            if managed.mirror.is_active() {
                return Err(OrchestratorError::AlreadyMirrored(pubky.to_string()));
            }
            managed.mirror = MirrorStatus::AwaitingApproval {
                homeserver: homeserver.z32(),
                authorization_url: authorization_url.clone(),
            };
            let task = tokio::spawn(wait_for_approval);
            replace_pending_approval(
                &mut managed.pending_mirror,
                Some(PendingApproval { id, task }),
            );
            Ok(())
        })
        .unwrap_or_else(|| Err(OrchestratorError::KeyNotFound(pubky.to_string())))?;

        info!("Started mirror of {} to {}", pubky, homeserver);
        Ok(ApprovalRequest::new(authorization_url, outcome_rx))
    }

    /// Cancel a mirror request that is still awaiting approval.
    ///
    /// Does nothing if the key has no pending mirror request, including one
    /// whose approval has just arrived and is being completed. A mirror that
    /// was set up before, and only needed its session renewed, is kept.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn cancel_mirror(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let cancelled = self
            .update_key(pubky, |managed| {
                let cancelled = replace_pending_approval(&mut managed.pending_mirror, None);
                if cancelled {
                    managed.mirror = self.stored_mirror_status(pubky);
                }
                cancelled
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))?;

        if cancelled {
            info!("Cancelled mirror request for key: {}", pubky);
        }
        Ok(())
    }

    /// Stop mirroring a key and forget its mirror.
    ///
    /// What was copied to the mirror stays there; the key's session on it is
    /// ended. Also cancels a mirror request that is still awaiting approval.
    ///
    /// # Errors
    ///
    /// Returns `OrchestratorError::KeyNotFound` if the key is not being backed up.
    pub async fn stop_mirror(&self, pubky: &PublicKey) -> Result<(), OrchestratorError> {
        let forgotten = self
            .update_key(pubky, |managed| {
                replace_pending_approval(&mut managed.pending_mirror, None);
                let stored = self.storage.read_mirror(pubky)?;
                self.storage.delete_mirror(pubky)?;
                managed.mirror = MirrorStatus::NotMirrored;
                Ok::<_, OrchestratorError>(stored)
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))??;

        let Some(stored) = forgotten else {
            return Ok(());
        };
        if let Some(secret) = stored.session {
            self.end_session_on_homeserver(secret, pubky);
        }
        log_activity(
            &self.storage,
            pubky,
            ActivityType::MirrorStopped,
            format!("Stopped mirroring to {}", stored.homeserver.z32()),
        )
        .await;
        info!("Stopped mirror of key: {}", pubky);
        Ok(())
    }

    /// The mirror status a key starts with, going by what is stored for it.
    ///
    /// The controller finds out on its first sync whether the mirror still
    /// accepts the stored session.
    pub(super) fn stored_mirror_status(&self, pubky: &PublicKey) -> MirrorStatus {
        match self.storage.read_mirror(pubky) {
            Ok(Some(stored)) => {
                let homeserver = stored.homeserver.z32();
                match stored.session {
                    Some(_) => MirrorStatus::Mirroring { homeserver },
                    None => MirrorStatus::SessionExpired { homeserver },
                }
            }
            Ok(None) => MirrorStatus::NotMirrored,
            Err(e) => {
                warn!("Failed to read mirror of {}: {}", pubky, e);
                MirrorStatus::NotMirrored
            }
        }
    }

    /// Forget the mirror session of a key that was just removed from the
    /// manager, and end it on the mirror. The mirror itself stays stored, so
    /// mirroring can resume if the key is added again and approves it.
    ///
    /// Failures are logged, not returned, so they don't keep the key from being removed.
    pub(super) fn discard_mirror_session_of_removed_key(&self, pubky: &PublicKey) {
        let stored = match self.storage.read_mirror(pubky) {
            Ok(Some(stored)) => stored,
            Ok(None) => return,
            Err(e) => {
                warn!("Failed to read mirror of {}: {}", pubky, e);
                return;
            }
        };
        let Some(secret) = stored.session.clone() else {
            return;
        };

        if let Err(e) = self.storage.write_mirror(pubky, &stored.without_session()) {
            warn!("Failed to forget mirror session of {}: {}", pubky, e);
        }
        self.end_session_on_homeserver(secret, pubky);
    }

    /// Get a key's mirror status.
    fn mirror_status(&self, pubky: &PublicKey) -> Result<MirrorStatus, OrchestratorError> {
        self.inner
            .read()
            .keys
            .get(pubky)
            .map(|managed| managed.mirror.clone())
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))
    }

    /// Refuse to mirror a key to the homeserver it already lives on.
    async fn ensure_not_own_homeserver(
        &self,
        pubky: &PublicKey,
        homeserver: &PublicKey,
    ) -> Result<(), OrchestratorError> {
        let own_homeserver = discovery::discover_homeserver(
            &self.pubky_client,
            pubky,
            self.config.validation_timeout_secs,
        )
        .await?;

        if own_homeserver == *homeserver {
            return Err(OrchestratorError::MirrorFailed(
                "A key cannot be mirrored to its own homeserver".to_string(),
            ));
        }
        Ok(())
    }

    /// Wait for a key's mirror request to be approved, then set the mirror up.
    async fn finish_mirror(
        &self,
        pubky: &PublicKey,
        homeserver: &PublicKey,
        id: u64,
        flow: ApprovalFlow,
        requested: &Capabilities,
    ) -> Result<(), OrchestratorError> {
        let approval = auth::await_approval(flow, pubky, requested).await;
        if !self.claim_approval(ApprovalTarget::Mirror(pubky), id) {
            if let Ok(session) = approval {
                auth::end_session(session);
            }
            return Err(auth::approval_cancelled());
        }

        let outcome = match approval {
            Ok(session) => self.complete_mirror(pubky, homeserver, session).await,
            Err(e) => Err(e),
        };
        if let Err(e) = &outcome {
            warn!("Setting up the mirror of {} failed: {}", pubky, e);
            self.update_key(pubky, |managed| {
                managed.mirror = MirrorStatus::SetupFailed {
                    homeserver: homeserver.z32(),
                    message: auth::failure_reason(e),
                };
            });
        }
        outcome
    }

    /// Open the key's session on the mirror with the grant behind an approved
    /// session, store the mirror, and start copying.
    async fn complete_mirror(
        &self,
        pubky: &PublicKey,
        homeserver: &PublicKey,
        approved: PubkySession,
    ) -> Result<(), OrchestratorError> {
        let opened = self.open_mirror_session(homeserver, &approved).await;
        // The session on the key's own homeserver only served to obtain the grant
        auth::end_session(approved);
        let (session, secret) = opened?;

        let stored = StoredMirror {
            homeserver: homeserver.clone(),
            session: Some(secret),
        };
        if let Err(e) = self.store_mirror(pubky, &stored) {
            // Don't leave a session nobody will use open on the mirror
            end_mirror_session(session);
            return Err(e);
        }

        info!("Mirroring {} to {}", pubky, homeserver);
        log_activity(
            &self.storage,
            pubky,
            ActivityType::MirrorStarted,
            format!("Started mirroring to {}", homeserver.z32()),
        )
        .await;
        if let Err(e) = self.force_sync(pubky).await {
            warn!("Failed to sync {} after mirror setup: {}", pubky, e);
        }
        Ok(())
    }

    /// Sign the key in to its account on the mirror with the grant behind
    /// `approved`. Returns the session and the secret it is stored as.
    async fn open_mirror_session(
        &self,
        homeserver: &PublicKey,
        approved: &PubkySession,
    ) -> Result<(MirrorSession, String), OrchestratorError> {
        let secret = auth::session_secret(approved).await?;
        let secret = mirror::session_secret_for(&secret, homeserver).ok_or_else(|| {
            OrchestratorError::MirrorFailed("The session cannot be moved to the mirror".to_string())
        })?;
        let session = MirrorSession::restore(&self.pubky_client, homeserver, &secret)
            .await
            .map_err(|e| OrchestratorError::MirrorFailed(e.to_string()))?;
        Ok((session, secret))
    }

    /// Store a key's mirror and mark the key as mirroring.
    ///
    /// The mirror replaces any the key had, including a request still awaiting
    /// approval, and the replaced mirror's session is ended.
    fn store_mirror(
        &self,
        pubky: &PublicKey,
        mirror: &StoredMirror,
    ) -> Result<(), OrchestratorError> {
        let replaced = self
            .update_key(pubky, |managed| {
                replace_pending_approval(&mut managed.pending_mirror, None);
                let replaced = self.storage.read_mirror(pubky)?;
                if let Err(e) = self.storage.write_mirror(pubky, mirror) {
                    managed.mirror = MirrorStatus::SetupFailed {
                        homeserver: mirror.homeserver.z32(),
                        message: e.to_string(),
                    };
                    return Err(e.into());
                }
                managed.mirror = MirrorStatus::Mirroring {
                    homeserver: mirror.homeserver.z32(),
                };
                Ok::<_, OrchestratorError>(replaced)
            })
            .ok_or_else(|| OrchestratorError::KeyNotFound(pubky.to_string()))??;

        if let Some(secret) = replaced
            .and_then(|replaced| replaced.session)
            .filter(|replaced| Some(replaced) != mirror.session.as_ref())
        {
            self.end_session_on_homeserver(secret, pubky);
        }
        Ok(())
    }
}

/// End a session on a mirror, in the background and on a best-effort basis.
fn end_mirror_session(session: MirrorSession) {
    tokio::spawn(async move {
        let pubky = session.user().clone();
        if let Err(e) = session.end().await {
            warn!("Failed to end session of {} on its mirror: {}", pubky, e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::approval::test_helpers::pending_forever;
    use super::super::status::SyncState;
    use super::super::tests::{setup, setup_with_dir, test_pubky};
    use super::*;
    use crate::storage::DataScope;
    use std::str::FromStr;
    use tempfile::TempDir;

    const MIRROR_HOMESERVER: &str = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";
    const OTHER_HOMESERVER: &str = "z4e8s17cou9qmuia76iczxkwakp8xnck5qrfbzh8rbpiq1sugbuo";

    fn stored_mirror(session: Option<&str>) -> StoredMirror {
        StoredMirror {
            homeserver: PublicKey::from_str(MIRROR_HOMESERVER).unwrap(),
            session: session.map(str::to_string),
        }
    }

    fn mirroring() -> MirrorStatus {
        MirrorStatus::Mirroring {
            homeserver: MIRROR_HOMESERVER.to_string(),
        }
    }

    fn session_expired() -> MirrorStatus {
        MirrorStatus::SessionExpired {
            homeserver: MIRROR_HOMESERVER.to_string(),
        }
    }

    /// Add the test key with a mirror already stored, as if it had been set up.
    async fn add_mirrored_key(manager: &BackupManager, session: Option<&str>) -> PublicKey {
        let pubky = test_pubky();
        manager
            .storage
            .write_mirror(&pubky, &stored_mirror(session))
            .unwrap();
        manager.add_key(pubky.clone()).await.unwrap();
        pubky
    }

    /// Register a pending mirror request for a tracked key, as `start_mirror`
    /// would. Returns the request's id.
    fn register_pending_mirror(
        manager: &BackupManager,
        pubky: &PublicKey,
        homeserver: &str,
    ) -> u64 {
        let (id, pending) = pending_forever();
        manager.update_key(pubky, |managed| {
            managed.mirror = MirrorStatus::AwaitingApproval {
                homeserver: homeserver.to_string(),
                authorization_url: format!("pubkyauth://example/{id}"),
            };
            replace_pending_approval(&mut managed.pending_mirror, Some(pending));
        });
        id
    }

    /// Add the test key and put it in the state of awaiting a mirror approval.
    async fn add_key_awaiting_mirror(manager: &BackupManager) -> (PublicKey, u64) {
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let id = register_pending_mirror(manager, &pubky, MIRROR_HOMESERVER);
        (pubky, id)
    }

    fn stored(manager: &BackupManager, pubky: &PublicKey) -> Option<StoredMirror> {
        manager.storage.read_mirror(pubky).unwrap()
    }

    #[tokio::test]
    async fn test_new_key_is_not_mirrored() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();

        manager.add_key(pubky.clone()).await.unwrap();

        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
    }

    #[tokio::test]
    async fn test_stored_mirror_resumes_after_restart() {
        let temp_dir = TempDir::new().unwrap();
        let pubky = {
            let (_, manager) = setup_with_dir(&temp_dir).await;
            let pubky = add_mirrored_key(&manager, Some("secret")).await;
            manager.shutdown().await;
            pubky
        };

        let (_, manager) = setup_with_dir(&temp_dir).await;

        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
    }

    #[tokio::test]
    async fn test_stored_mirror_without_session_needs_approval_again() {
        let (_dir, manager) = setup().await;

        let pubky = add_mirrored_key(&manager, None).await;

        assert_eq!(manager.mirror_status(&pubky).unwrap(), session_expired());
    }

    // --- Starting a mirror ---

    #[tokio::test]
    async fn test_mirror_methods_reject_unknown_key() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        let homeserver = PublicKey::from_str(MIRROR_HOMESERVER).unwrap();

        assert!(matches!(
            manager.start_mirror(&pubky, &homeserver).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
        assert!(matches!(
            manager.cancel_mirror(&pubky).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
        assert!(matches!(
            manager.stop_mirror(&pubky).await,
            Err(OrchestratorError::KeyNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_start_mirror_rejects_key_with_working_mirror() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;
        let other = PublicKey::from_str(OTHER_HOMESERVER).unwrap();

        let result = manager.start_mirror(&pubky, &other).await;

        assert!(matches!(result, Err(OrchestratorError::AlreadyMirrored(_))));
        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
    }

    #[tokio::test]
    async fn test_start_mirror_unavailable_offline() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();
        let homeserver = PublicKey::from_str(MIRROR_HOMESERVER).unwrap();

        let result = manager.start_mirror(&pubky, &homeserver).await;

        assert!(matches!(result, Err(OrchestratorError::MirrorFailed(_))));
        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
    }

    // --- Claiming an approved request ---

    #[tokio::test]
    async fn test_pending_mirror_can_be_claimed_once() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_mirror(&manager).await;

        assert!(manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
        assert!(!manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
    }

    #[tokio::test]
    async fn test_cancelled_mirror_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_mirror(&manager).await;

        manager.cancel_mirror(&pubky).await.unwrap();

        assert!(!manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
    }

    #[tokio::test]
    async fn test_replaced_mirror_request_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, replaced) = add_key_awaiting_mirror(&manager).await;
        let current = register_pending_mirror(&manager, &pubky, OTHER_HOMESERVER);

        assert!(!manager.claim_approval(ApprovalTarget::Mirror(&pubky), replaced));
        assert!(manager.claim_approval(ApprovalTarget::Mirror(&pubky), current));
    }

    #[tokio::test]
    async fn test_mirror_request_of_removed_key_cannot_be_claimed() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_mirror(&manager).await;

        manager.remove_key(&pubky).await.unwrap();

        assert!(!manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
    }

    #[tokio::test]
    async fn test_pending_sign_in_and_mirror_are_claimed_apart() {
        let (_dir, manager) = setup().await;
        let (pubky, mirror_id) = add_key_awaiting_mirror(&manager).await;

        assert!(!manager.claim_approval(ApprovalTarget::SignIn(&pubky), mirror_id));
        assert!(manager.claim_approval(ApprovalTarget::Mirror(&pubky), mirror_id));
    }

    // --- Storing an approved mirror ---

    #[tokio::test]
    async fn test_stored_mirror_starts_mirroring() {
        let (_dir, manager) = setup().await;
        let (pubky, _id) = add_key_awaiting_mirror(&manager).await;
        let mirror = stored_mirror(Some("secret"));

        manager.store_mirror(&pubky, &mirror).unwrap();

        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
        assert_eq!(stored(&manager, &pubky), Some(mirror));
    }

    #[tokio::test]
    async fn test_stored_mirror_replaces_expired_one() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, None).await;
        manager
            .storage
            .write_mirror_cursor(&pubky, DataScope::Public, 7)
            .await
            .unwrap();

        manager
            .store_mirror(&pubky, &stored_mirror(Some("newer")))
            .unwrap();

        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
        // Copying resumes where it stopped
        assert_eq!(
            manager
                .storage
                .read_mirror_cursor(&pubky, DataScope::Public)
                .await
                .unwrap(),
            Some(7)
        );
    }

    #[tokio::test]
    async fn test_storing_mirror_of_removed_key_fails() {
        let (_dir, manager) = setup().await;
        let (pubky, _id) = add_key_awaiting_mirror(&manager).await;
        manager.remove_key(&pubky).await.unwrap();

        let result = manager.store_mirror(&pubky, &stored_mirror(Some("secret")));

        assert!(matches!(result, Err(OrchestratorError::KeyNotFound(_))));
        assert_eq!(stored(&manager, &pubky), None);
    }

    // --- Cancelling and stopping ---

    #[tokio::test]
    async fn test_cancel_returns_first_request_to_not_mirrored() {
        let (_dir, manager) = setup().await;
        let (pubky, _id) = add_key_awaiting_mirror(&manager).await;
        let mut rx = manager.subscribe();

        manager.cancel_mirror(&pubky).await.unwrap();

        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
        // Subscribers are told about the change
        assert_eq!(
            rx.try_recv().unwrap().state.mirror,
            MirrorStatus::NotMirrored
        );
    }

    #[tokio::test]
    async fn test_cancel_of_renewal_keeps_the_mirror() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, None).await;
        register_pending_mirror(&manager, &pubky, MIRROR_HOMESERVER);

        manager.cancel_mirror(&pubky).await.unwrap();

        assert_eq!(manager.mirror_status(&pubky).unwrap(), session_expired());
        assert_eq!(stored(&manager, &pubky), Some(stored_mirror(None)));
    }

    #[tokio::test]
    async fn test_cancel_leaves_working_mirror_alone() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;

        manager.cancel_mirror(&pubky).await.unwrap();

        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
    }

    #[tokio::test]
    async fn test_cancel_after_approval_is_claimed_does_nothing() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_mirror(&manager).await;
        assert!(manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
        let completing = manager.mirror_status(&pubky).unwrap();

        manager.cancel_mirror(&pubky).await.unwrap();

        // The request being completed is left to finish
        assert_eq!(manager.mirror_status(&pubky).unwrap(), completing);
    }

    #[tokio::test]
    async fn test_stop_mirror_forgets_mirror_and_logs_activity() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;
        manager
            .storage
            .write_mirror_cursor(&pubky, DataScope::Public, 7)
            .await
            .unwrap();
        let mut rx = manager.subscribe();

        manager.stop_mirror(&pubky).await.unwrap();

        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
        assert_eq!(stored(&manager, &pubky), None);
        // A mirror set up later starts from the beginning
        assert_eq!(
            manager
                .storage
                .read_mirror_cursor(&pubky, DataScope::Public)
                .await
                .unwrap(),
            None
        );
        let entries = manager.get_activity(&pubky, 10).await;
        assert!(entries
            .iter()
            .any(|e| e.activity_type == ActivityType::MirrorStopped));
        assert_eq!(
            rx.try_recv().unwrap().state.mirror,
            MirrorStatus::NotMirrored
        );
    }

    #[tokio::test]
    async fn test_stop_mirror_cancels_pending_request() {
        let (_dir, manager) = setup().await;
        let (pubky, id) = add_key_awaiting_mirror(&manager).await;

        manager.stop_mirror(&pubky).await.unwrap();

        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
        assert!(!manager.claim_approval(ApprovalTarget::Mirror(&pubky), id));
    }

    #[tokio::test]
    async fn test_stop_mirror_without_mirror_logs_no_activity() {
        let (_dir, manager) = setup().await;
        let pubky = test_pubky();
        manager.add_key(pubky.clone()).await.unwrap();

        manager.stop_mirror(&pubky).await.unwrap();

        let entries = manager.get_activity(&pubky, 10).await;
        assert!(!entries
            .iter()
            .any(|e| e.activity_type == ActivityType::MirrorStopped));
    }

    #[tokio::test]
    async fn test_removing_key_forgets_mirror_session_but_keeps_mirror() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;

        manager.remove_key(&pubky).await.unwrap();

        assert_eq!(stored(&manager, &pubky), Some(stored_mirror(None)));
    }

    // --- Controller notices ---

    #[tokio::test]
    async fn test_failure_and_recovery_notices_update_status() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;

        let failing = manager
            .inner
            .write()
            .apply_mirror_failure(&pubky, Some("unreachable"));
        assert_eq!(
            failing.unwrap().mirror,
            MirrorStatus::Failing {
                homeserver: MIRROR_HOMESERVER.to_string(),
                message: "unreachable".to_string(),
            }
        );
        // The same failure again changes nothing
        assert!(manager
            .inner
            .write()
            .apply_mirror_failure(&pubky, Some("unreachable"))
            .is_none());

        let recovered = manager.inner.write().apply_mirror_failure(&pubky, None);
        assert_eq!(recovered.unwrap().mirror, mirroring());
        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
    }

    #[tokio::test]
    async fn test_rejected_mirror_session_expires_mirror() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;
        let rejected = SessionSecret::new("secret".to_string());

        let state =
            manager
                .inner
                .write()
                .expire_mirror_session(&manager.storage, &pubky, &rejected);

        assert_eq!(state.unwrap().mirror, session_expired());
        assert_eq!(manager.mirror_status(&pubky).unwrap(), session_expired());
        assert_eq!(stored(&manager, &pubky), Some(stored_mirror(None)));
    }

    #[tokio::test]
    async fn test_rejection_of_replaced_mirror_session_is_ignored() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;
        // The mirror was approved again while a sync used an older session
        let rejected = SessionSecret::new("older".to_string());

        let state =
            manager
                .inner
                .write()
                .expire_mirror_session(&manager.storage, &pubky, &rejected);

        assert!(state.is_none());
        assert_eq!(manager.mirror_status(&pubky).unwrap(), mirroring());
        assert_eq!(
            stored(&manager, &pubky),
            Some(stored_mirror(Some("secret")))
        );
    }

    #[tokio::test]
    async fn test_late_notice_does_not_bring_back_stopped_mirror() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;
        manager.stop_mirror(&pubky).await.unwrap();

        let state = manager
            .inner
            .write()
            .apply_mirror_failure(&pubky, Some("unreachable"));

        assert!(state.is_none());
        assert_eq!(
            manager.mirror_status(&pubky).unwrap(),
            MirrorStatus::NotMirrored
        );
    }

    #[tokio::test]
    async fn test_controller_updates_keep_mirror_status() {
        let (_dir, manager) = setup().await;
        let pubky = add_mirrored_key(&manager, Some("secret")).await;

        // Controllers don't know the mirror status, so their states carry the default
        let applied = manager
            .inner
            .write()
            .apply_sync_state(&pubky, SyncState::default())
            .unwrap();

        assert_eq!(applied.mirror, mirroring());
    }
}
