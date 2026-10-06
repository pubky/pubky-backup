//! Copying a pubky's backed-up data to its mirror homeserver.
//!
//! Mirroring replays the events of the pubky's own homeserver from a cursor of
//! its own, and for each event copies the file from the local backup to the
//! mirror, or deletes it there. It runs after the backup in every sync cycle,
//! and never gets ahead of it, since the backup is where the content comes from.
//!
//! Problems with the mirror never fail the sync: they are reported as their
//! own status, and the backup itself carries on.

use futures_util::StreamExt;
use log::{info, warn};
use pubky::{Event, EventType, PublicKey};

use super::{BackupController, ControllerStatus};
use crate::storage::StoredMirror;
use crate::sync::error::SyncError;
use crate::sync::events::{self, EventStream, EVENT_BATCH_SIZE};
use crate::sync::mirror::{MirrorError, MirrorSession, MirrorTarget};
use crate::sync::scope::SyncScope;
use crate::sync::session::SessionSecret;

/// The range of a scope's event stream the mirror is missing.
#[derive(Debug, PartialEq)]
pub(super) struct PendingChanges {
    /// The last event copied to the mirror, if any
    mirrored: Option<u64>,
    /// The last event the backup has, which the copy must not go beyond
    backed_up: u64,
}

/// The pubky's session on its mirror together with the stored mirror it was
/// restored from, so a changed or removed mirror is noticed.
///
/// As with the sign-in session, each restore exchanges the grant for a new
/// access token, replacing the previous one, so the controller restores once
/// and keeps the session.
#[derive(Clone)]
pub(super) struct RestoredMirror {
    stored: StoredMirror,
    session: MirrorSession,
}

impl BackupController {
    /// Copy a batch of backed-up changes to the pubky's mirror, if it has one.
    ///
    /// Returns whether more changes are waiting to be copied.
    pub(super) async fn perform_mirror_batch(&mut self) -> bool {
        // Offline there is no mirror to reach
        if self.offline {
            return false;
        }
        let Some(restored) = self.resolve_mirror().await else {
            return false;
        };

        match self
            .mirror_scopes(&restored.stored.homeserver, &restored.session)
            .await
        {
            Ok(has_more) => {
                self.report_mirror_recovered();
                has_more
            }
            Err(MirrorError::SessionRejected) => {
                self.reject_mirror_session(restored.stored);
                false
            }
            Err(MirrorError::Failed(message)) => {
                self.report_mirror_failure(message);
                false
            }
        }
    }

    /// Get the pubky's mirror and the session to copy to it with.
    ///
    /// The stored mirror is the source of truth: setting a mirror up, stopping
    /// it or approving it again changes it, and the controller picks that up on
    /// its next sync, restoring the session when its secret changed.
    ///
    /// `None` if the pubky has no mirror, the mirror has no session, or the
    /// session could not be restored, which is reported.
    async fn resolve_mirror(&mut self) -> Option<RestoredMirror> {
        let stored = match self.storage.read_mirror(&self.pubky) {
            Ok(Some(stored)) => stored,
            Ok(None) => {
                self.mirror = None;
                self.mirror_failure = None;
                return None;
            }
            Err(e) => {
                warn!("Failed to read mirror of {}: {}", self.pubky, e);
                return None;
            }
        };
        let Some(secret) = stored.session.clone() else {
            self.mirror = None;
            self.mirror_failure = None;
            return None;
        };

        if self.rejected_mirror_secret.as_ref() == Some(&secret) {
            return None;
        }
        if let Some(restored) = self.mirror.as_ref().filter(|m| m.stored == stored) {
            return Some(restored.clone());
        }

        match MirrorSession::restore(&self.pubky_client, &stored.homeserver, &secret).await {
            Ok(session) => {
                info!(
                    "Restored session of {} on its mirror {}",
                    self.pubky, stored.homeserver
                );
                let restored = RestoredMirror { stored, session };
                self.mirror = Some(restored.clone());
                Some(restored)
            }
            Err(MirrorError::SessionRejected) => {
                self.reject_mirror_session(stored);
                None
            }
            Err(MirrorError::Failed(message)) => {
                self.report_mirror_failure(message);
                None
            }
        }
    }

    /// Copy a batch of public changes and, if signed in, of private changes.
    async fn mirror_scopes<M: MirrorTarget + Sync>(
        &self,
        homeserver: &PublicKey,
        mirror: &M,
    ) -> Result<bool, MirrorError> {
        self.ensure_mirror_is_not_source(homeserver).await?;

        let session = self.session.as_ref().map(|restored| &restored.session);
        let scopes = [Some(SyncScope::Public), session.map(SyncScope::Private)];
        let mut has_more = false;
        for scope in scopes.into_iter().flatten() {
            if let Some(changes) = self.pending_changes(scope).await? {
                has_more |= self.mirror_scope_batch(scope, changes, mirror).await?;
            }
        }
        Ok(has_more)
    }

    /// The changes of a scope the mirror is missing: from where copying got to,
    /// up to where the backup got to. `None` if the mirror is up to date.
    pub(super) async fn pending_changes(
        &self,
        scope: SyncScope<'_>,
    ) -> Result<Option<PendingChanges>, MirrorError> {
        let storage = &self.storage;
        let Some(backed_up) = storage.read_cursor(&self.pubky, scope.data_scope()).await? else {
            return Ok(None);
        };
        let mirrored = storage
            .read_mirror_cursor(&self.pubky, scope.data_scope())
            .await?;
        if mirrored >= Some(backed_up) {
            return Ok(None);
        }
        Ok(Some(PendingChanges {
            mirrored,
            backed_up,
        }))
    }

    /// Refuse to copy to a mirror that has become the pubky's own homeserver.
    ///
    /// Every copy would then be a new change to copy, without end.
    async fn ensure_mirror_is_not_source(&self, mirror: &PublicKey) -> Result<(), MirrorError> {
        match self.pubky_client.get_homeserver_of(&self.pubky).await {
            Ok(Some(source)) if source == *mirror => Err(MirrorError::Failed(
                "The mirror is now the key's own homeserver, so there is nothing to mirror"
                    .to_string(),
            )),
            // A homeserver that can't be resolved fails the backup itself
            _ => Ok(()),
        }
    }

    /// Copy a batch of one scope's pending changes. Returns whether more are waiting.
    async fn mirror_scope_batch<M: MirrorTarget + Sync>(
        &self,
        scope: SyncScope<'_>,
        changes: PendingChanges,
        mirror: &M,
    ) -> Result<bool, MirrorError> {
        let event_stream = match events::create_event_stream(
            &self.pubky_client,
            &self.pubky,
            scope,
            changes.mirrored,
        )
        .await
        {
            Ok(stream) => stream,
            // The backup reports a rejected sign-in session; without it there
            // is nothing private to copy
            Err(SyncError::SessionRejected) => return Ok(false),
            Err(e) => return Err(MirrorError::Failed(e.to_string())),
        };

        self.mirror_event_stream(
            event_stream,
            changes.mirrored,
            changes.backed_up,
            scope,
            mirror,
        )
        .await
    }

    /// Copy the changes of a stream of events, up to the event the backup has
    /// reached, saving how far it got. Returns whether more changes are waiting.
    pub(super) async fn mirror_event_stream<M: MirrorTarget + Sync>(
        &self,
        mut event_stream: EventStream,
        mut cursor: Option<u64>,
        backed_up: u64,
        scope: SyncScope<'_>,
        mirror: &M,
    ) -> Result<bool, MirrorError> {
        let mut events_copied = 0;

        let outcome = loop {
            let event = match event_stream.next().await {
                Some(Ok(event)) => event,
                Some(Err(e)) => break Err(MirrorError::Failed(e.to_string())),
                None => break Ok(()),
            };

            // Never get ahead of the backup, which is where the content comes from
            let event_cursor = event.cursor.id();
            if event_cursor > backed_up {
                break Ok(());
            }
            if let Err(e) = self.mirror_event(&event, mirror).await {
                break Err(e);
            }
            cursor = Some(event_cursor);
            events_copied += 1;
        };

        // Keep the progress made, also when an error stopped it
        if let Some(cursor) = cursor {
            self.storage
                .write_mirror_cursor(&self.pubky, scope.data_scope(), cursor)
                .await?;
        }
        outcome?;

        if events_copied > 0 {
            info!(
                "Copied {} events to the mirror of {}",
                events_copied, self.pubky
            );
        }
        // A full batch means there may be more
        Ok(events_copied >= EVENT_BATCH_SIZE as usize)
    }

    /// Apply one event to the mirror.
    async fn mirror_event<M: MirrorTarget + Sync>(
        &self,
        event: &Event,
        mirror: &M,
    ) -> Result<(), MirrorError> {
        // Skip events for other pubkys
        if event.resource.owner != self.pubky {
            return Ok(());
        }

        match event.event_type {
            EventType::Put { .. } => match self.storage.read(&event.resource).await {
                Ok(data) => mirror.put(&event.resource, data).await,
                // Not in the backup: deleted again since, or it could not be fetched
                Err(e) if e.is_not_found() => Ok(()),
                Err(e) => Err(MirrorError::Failed(format!(
                    "Failed to read {} from the backup: {}",
                    event.resource, e
                ))),
            },
            EventType::Delete => mirror.delete(&event.resource).await,
        }
    }

    /// Stop using a mirror session the mirror no longer accepts, and report
    /// it, so nothing is copied until the pubky's owner approves the mirror again.
    ///
    /// The stored mirror is left in place, and not used again; whoever manages
    /// the pubky's mirror decides what to do with it.
    fn reject_mirror_session(&mut self, rejected: StoredMirror) {
        self.mirror = None;
        self.mirror_failure = None;
        let Some(secret) = rejected.session else {
            return;
        };
        self.rejected_mirror_secret = Some(secret.clone());
        self.send_status(ControllerStatus::MirrorSessionRejected {
            secret: SessionSecret::new(secret),
        });
    }

    /// Report a problem with the mirror, once for as long as it stays the same.
    fn report_mirror_failure(&mut self, message: String) {
        if self.mirror_failure.as_deref() == Some(message.as_str()) {
            return;
        }
        warn!("Mirroring {} failed: {}", self.pubky, message);
        self.mirror_failure = Some(message.clone());
        self.send_status(ControllerStatus::MirrorFailed { message });
    }

    /// Report that mirroring works again, if a problem was reported before.
    fn report_mirror_recovered(&mut self) {
        if self.mirror_failure.take().is_some() {
            self.send_status(ControllerStatus::MirrorRecovered);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        create_controlled_controller, create_detached_controller, create_test_storage,
    };
    use super::*;
    use crate::storage::{AppStorage, DataScope};
    use crate::sync::events::test_helpers::{make_test_event, put_event_type};
    use crate::TEST_PUBKY;
    use pubky::PubkyResource;
    use std::str::FromStr;
    use std::sync::Mutex;

    const MIRROR_HOMESERVER: &str = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";

    /// A mirror that records what is copied to it, and can be made to refuse a path.
    #[derive(Default)]
    struct FakeMirror {
        requests: Mutex<Vec<String>>,
        refuses: Option<(&'static str, MirrorError)>,
    }

    impl FakeMirror {
        fn refusing(path: &'static str, error: MirrorError) -> Self {
            Self {
                refuses: Some((path, error)),
                ..Default::default()
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }

        fn handle(&self, request: String, resource: &PubkyResource) -> Result<(), MirrorError> {
            if let Some((path, error)) = &self.refuses {
                if resource.path.as_str() == *path {
                    return Err(error.clone());
                }
            }
            self.requests.lock().unwrap().push(request);
            Ok(())
        }
    }

    impl MirrorTarget for FakeMirror {
        async fn put(&self, resource: &PubkyResource, data: Vec<u8>) -> Result<(), MirrorError> {
            let content = String::from_utf8_lossy(&data);
            self.handle(format!("PUT {} {}", resource.path, content), resource)
        }

        async fn delete(&self, resource: &PubkyResource) -> Result<(), MirrorError> {
            self.handle(format!("DELETE {}", resource.path), resource)
        }
    }

    fn pubky() -> PublicKey {
        PublicKey::from_str(TEST_PUBKY).unwrap()
    }

    fn put(path: &str, cursor: u64) -> Event {
        make_test_event(TEST_PUBKY, put_event_type(), path, cursor)
    }

    fn delete(path: &str, cursor: u64) -> Event {
        make_test_event(TEST_PUBKY, EventType::Delete, path, cursor)
    }

    fn stream_of(events: Vec<Event>) -> EventStream {
        Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
    }

    /// Put a file in the local backup, as backing it up would.
    async fn back_up(storage: &AppStorage, path: &str, content: &str) {
        let resource = PubkyResource::new(pubky(), path).unwrap();
        storage.write(&resource, content.into()).await.unwrap();
    }

    async fn mirror_cursor(storage: &AppStorage) -> Option<u64> {
        storage
            .read_mirror_cursor(&pubky(), DataScope::Public)
            .await
            .unwrap()
    }

    fn stored_mirror(session: Option<&str>) -> StoredMirror {
        StoredMirror {
            homeserver: PublicKey::from_str(MIRROR_HOMESERVER).unwrap(),
            session: session.map(str::to_string),
        }
    }

    // --- Copying events ---

    #[tokio::test]
    async fn test_copies_backed_up_files_in_order_and_saves_progress() {
        let (storage, _temp_dir) = create_test_storage();
        back_up(&storage, "/pub/a.txt", "first").await;
        back_up(&storage, "/pub/b.txt", "second").await;
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        let events = stream_of(vec![put("/pub/a.txt", 1), put("/pub/b.txt", 2)]);

        let has_more = controller
            .mirror_event_stream(events, None, 2, SyncScope::Public, &mirror)
            .await
            .unwrap();

        assert_eq!(
            mirror.requests(),
            ["PUT /pub/a.txt first", "PUT /pub/b.txt second"]
        );
        assert_eq!(mirror_cursor(&storage).await, Some(2));
        assert!(!has_more);
    }

    #[tokio::test]
    async fn test_deletions_are_mirrored() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        let events = stream_of(vec![delete("/pub/gone.txt", 1)]);

        controller
            .mirror_event_stream(events, None, 1, SyncScope::Public, &mirror)
            .await
            .unwrap();

        assert_eq!(mirror.requests(), ["DELETE /pub/gone.txt"]);
        assert_eq!(mirror_cursor(&storage).await, Some(1));
    }

    #[tokio::test]
    async fn test_files_missing_from_the_backup_are_skipped() {
        let (storage, _temp_dir) = create_test_storage();
        back_up(&storage, "/pub/kept.txt", "kept").await;
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        // The first file was deleted again later, so the backup no longer has it
        let events = stream_of(vec![
            put("/pub/deleted-later.txt", 1),
            put("/pub/kept.txt", 2),
        ]);

        controller
            .mirror_event_stream(events, None, 2, SyncScope::Public, &mirror)
            .await
            .unwrap();

        assert_eq!(mirror.requests(), ["PUT /pub/kept.txt kept"]);
        // The skipped event still counts as dealt with
        assert_eq!(mirror_cursor(&storage).await, Some(2));
    }

    #[tokio::test]
    async fn test_never_copies_beyond_what_is_backed_up() {
        let (storage, _temp_dir) = create_test_storage();
        back_up(&storage, "/pub/a.txt", "first").await;
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        // The homeserver already has a second event, which the backup has not reached
        let events = stream_of(vec![put("/pub/a.txt", 1), put("/pub/b.txt", 2)]);

        let has_more = controller
            .mirror_event_stream(events, None, 1, SyncScope::Public, &mirror)
            .await
            .unwrap();

        assert_eq!(mirror.requests(), ["PUT /pub/a.txt first"]);
        assert_eq!(mirror_cursor(&storage).await, Some(1));
        assert!(!has_more);
    }

    #[tokio::test]
    async fn test_events_of_other_pubkys_are_not_copied() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        let other = make_test_event(MIRROR_HOMESERVER, EventType::Delete, "/pub/a.txt", 1);

        controller
            .mirror_event_stream(stream_of(vec![other]), None, 1, SyncScope::Public, &mirror)
            .await
            .unwrap();

        assert!(mirror.requests().is_empty());
    }

    #[tokio::test]
    async fn test_full_batch_reports_more_to_copy() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::default();
        let batch_size = EVENT_BATCH_SIZE as u64;
        let events = (1..=batch_size)
            .map(|cursor| delete(&format!("/pub/{cursor}.txt"), cursor))
            .collect();

        let has_more = controller
            .mirror_event_stream(
                stream_of(events),
                None,
                batch_size + 10,
                SyncScope::Public,
                &mirror,
            )
            .await
            .unwrap();

        assert!(has_more);
        assert_eq!(mirror_cursor(&storage).await, Some(batch_size));
    }

    #[tokio::test]
    async fn test_failure_stops_copying_but_keeps_progress() {
        let (storage, _temp_dir) = create_test_storage();
        for path in ["/pub/a.txt", "/pub/b.txt", "/pub/c.txt"] {
            back_up(&storage, path, "content").await;
        }
        let controller = create_detached_controller(storage.clone());
        let refusal = MirrorError::Failed("507 Insufficient Storage".to_string());
        let mirror = FakeMirror::refusing("/pub/b.txt", refusal.clone());
        let events = stream_of(vec![
            put("/pub/a.txt", 1),
            put("/pub/b.txt", 2),
            put("/pub/c.txt", 3),
        ]);

        let result = controller
            .mirror_event_stream(events, None, 3, SyncScope::Public, &mirror)
            .await;

        assert_eq!(result, Err(refusal));
        assert_eq!(mirror.requests(), ["PUT /pub/a.txt content"]);
        // The refused file is retried next time, rather than skipped
        assert_eq!(mirror_cursor(&storage).await, Some(1));
    }

    #[tokio::test]
    async fn test_rejected_session_stops_copying() {
        let (storage, _temp_dir) = create_test_storage();
        back_up(&storage, "/pub/a.txt", "content").await;
        let controller = create_detached_controller(storage.clone());
        let mirror = FakeMirror::refusing("/pub/a.txt", MirrorError::SessionRejected);

        let result = controller
            .mirror_event_stream(
                stream_of(vec![put("/pub/a.txt", 1)]),
                None,
                1,
                SyncScope::Public,
                &mirror,
            )
            .await;

        assert_eq!(result, Err(MirrorError::SessionRejected));
        assert_eq!(mirror_cursor(&storage).await, None);
    }

    // --- Deciding whether there is anything to copy ---

    #[tokio::test]
    async fn test_nothing_pending_before_anything_is_backed_up() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage);

        let pending = controller.pending_changes(SyncScope::Public).await.unwrap();

        assert_eq!(pending, None);
    }

    #[tokio::test]
    async fn test_nothing_pending_when_mirror_is_up_to_date() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_cursor(&pubky(), DataScope::Public, 5)
            .await
            .unwrap();
        storage
            .write_mirror_cursor(&pubky(), DataScope::Public, 5)
            .await
            .unwrap();
        let controller = create_detached_controller(storage);

        let pending = controller.pending_changes(SyncScope::Public).await.unwrap();

        assert_eq!(pending, None);
    }

    #[tokio::test]
    async fn test_pending_changes_span_from_mirror_to_backup() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_cursor(&pubky(), DataScope::Public, 5)
            .await
            .unwrap();
        storage
            .write_mirror_cursor(&pubky(), DataScope::Public, 2)
            .await
            .unwrap();
        let controller = create_detached_controller(storage);

        let pending = controller.pending_changes(SyncScope::Public).await.unwrap();

        assert_eq!(
            pending,
            Some(PendingChanges {
                mirrored: Some(2),
                backed_up: 5,
            })
        );
    }

    #[tokio::test]
    async fn test_no_mirror_means_nothing_to_copy() {
        let (storage, _temp_dir) = create_test_storage();
        // Online, so that a stored mirror would be reached
        let (mut controller, _control_tx, mut status_rx) = create_controlled_controller(storage);
        controller = controller.offline(false);

        assert!(!controller.perform_mirror_batch().await);
        assert!(status_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_mirror_without_session_is_not_copied_to() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_mirror(&pubky(), &stored_mirror(None))
            .unwrap();
        let mut controller = create_detached_controller(storage).offline(false);

        assert!(controller.resolve_mirror().await.is_none());
    }

    #[tokio::test]
    async fn test_mirror_is_not_reached_offline() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_mirror(&pubky(), &stored_mirror(Some("secret")))
            .unwrap();
        let (mut controller, _control_tx, mut status_rx) = create_controlled_controller(storage);

        assert!(!controller.perform_mirror_batch().await);
        assert!(status_rx.try_recv().is_err());
    }

    // --- Reporting ---

    #[tokio::test]
    async fn test_rejected_mirror_session_is_reported_and_left_stored() {
        let (storage, _temp_dir) = create_test_storage();
        let stored = stored_mirror(Some("secret"));
        storage.write_mirror(&pubky(), &stored).unwrap();
        storage
            .write_mirror_cursor(&pubky(), DataScope::Public, 7)
            .await
            .unwrap();
        let (mut controller, _control_tx, mut status_rx) =
            create_controlled_controller(storage.clone());

        controller.reject_mirror_session(stored.clone());

        match status_rx.try_recv().unwrap().status {
            ControllerStatus::MirrorSessionRejected { secret } => {
                assert_eq!(secret, SessionSecret::new("secret".to_string()));
            }
            other => panic!("Expected MirrorSessionRejected, got {:?}", other),
        }
        // Whoever manages the mirror decides what happens to the stored record
        assert_eq!(storage.read_mirror(&pubky()).unwrap(), Some(stored));
        // Copying resumes where it stopped once the mirror is approved again
        assert_eq!(mirror_cursor(&storage).await, Some(7));
        assert_eq!(controller.rejected_mirror_secret.as_deref(), Some("secret"));
    }

    #[tokio::test]
    async fn test_rejected_secret_is_not_restored_again() {
        let (storage, _temp_dir) = create_test_storage();
        let stored = stored_mirror(Some("secret"));
        storage.write_mirror(&pubky(), &stored).unwrap();
        let mut controller = create_detached_controller(storage).offline(false);
        controller.reject_mirror_session(stored);

        // Restoring would reach the mirror; the rejected secret is skipped instead
        assert!(controller.resolve_mirror().await.is_none());
    }

    #[tokio::test]
    async fn test_mirror_failure_is_reported_once_until_it_changes() {
        let (storage, _temp_dir) = create_test_storage();
        let (mut controller, _control_tx, mut status_rx) = create_controlled_controller(storage);

        controller.report_mirror_failure("unreachable".to_string());
        controller.report_mirror_failure("unreachable".to_string());
        controller.report_mirror_failure("out of space".to_string());

        let mut reported = Vec::new();
        while let Ok(update) = status_rx.try_recv() {
            if let ControllerStatus::MirrorFailed { message } = update.status {
                reported.push(message);
            }
        }
        assert_eq!(reported, ["unreachable", "out of space"]);
    }

    #[tokio::test]
    async fn test_recovery_is_reported_only_after_a_failure() {
        let (storage, _temp_dir) = create_test_storage();
        let (mut controller, _control_tx, mut status_rx) = create_controlled_controller(storage);

        controller.report_mirror_recovered();
        assert!(status_rx.try_recv().is_err());

        controller.report_mirror_failure("unreachable".to_string());
        controller.report_mirror_recovered();
        controller.report_mirror_recovered();

        assert!(matches!(
            status_rx.try_recv().unwrap().status,
            ControllerStatus::MirrorFailed { .. }
        ));
        assert!(matches!(
            status_rx.try_recv().unwrap().status,
            ControllerStatus::MirrorRecovered
        ));
        assert!(status_rx.try_recv().is_err());
    }
}
