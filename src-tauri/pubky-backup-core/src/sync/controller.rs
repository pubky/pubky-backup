//! Backup controller for synchronizing a single pubky.
//!
//! The [`BackupController`] is the core sync loop that:
//! 1. Polls for new events from a pubky's homeserver
//! 2. Downloads and stores new/updated resources
//! 3. Deletes resources that have been removed
//! 4. Tracks progress via cursors for resumable syncing
//!
//! Public data is always synced. Private data is synced as well while a session
//! secret is stored for the pubky (see [`AppStorage::write_session_secret`]).
//! Each sync cycle syncs public data to the end, then private data, so a
//! problem with private data never holds back the backup of public data. If a
//! mirror is stored for the pubky (see [`AppStorage::write_mirror`]), the cycle
//! ends by copying what was backed up to it (see [`mirroring`]).
//!
//! # Responsibilities
//!
//! - Main sync loop with interval-based polling
//! - Event stream processing (PUT/DELETE operations)
//! - Status reporting to the orchestrator layer
//! - Handling control messages (Cancel, ForceSync)
//!
//! # Channel Architecture
//!
//! The controller uses two channels:
//! - `control_rx`: Receives commands from the orchestrator (Cancel, ForceSync)
//! - `status_tx`: Sends status updates to the orchestrator ([`ControllerUpdate`])

use super::error::SyncError;
use super::events::{self, EventStream, EVENT_BATCH_SIZE};
use super::fetcher;
use super::scope::SyncScope;
use super::session::{is_session_rejection, SessionSecret};
use crate::storage::{AppStorage, DataScope};
#[cfg(test)]
use crate::TEST_PUBKY;
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use pubky::{Event, EventType, Pubky, PubkySession, PublicKey};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time;

mod mirroring;

use mirroring::RestoredMirror;

/// Minimum allowed sync interval in seconds.
pub const MIN_SYNC_INTERVAL_SECONDS: u64 = 10;

/// Default sync interval in seconds between backup batches.
pub const DEFAULT_SYNC_INTERVAL_SECONDS: u64 = 300;

/// Messages that can be sent to control the backup controller.
///
/// These messages are sent from the orchestrator layer to control
/// individual backup controllers.
#[derive(Debug, Clone)]
pub enum ControllerCommand {
    /// Stop the backup controller gracefully
    Cancel,
    /// Trigger an immediate sync (bypasses the interval timer)
    ForceSync,
}

/// A status update from a backup controller, as sent on the status channel.
///
/// All controllers send on the same channel, so each update names the pubky
/// it is for. The orchestrator transforms these into
/// [`KeyUpdate`](crate::orchestrator::KeyUpdate) messages for external consumers.
#[derive(Debug, Clone)]
pub struct ControllerUpdate {
    /// The public key the update is for
    pub pubky: PublicKey,
    /// The controller's new status
    pub status: ControllerStatus,
}

/// Status of a backup controller, as reported to the orchestrator.
#[derive(Debug, Clone)]
pub enum ControllerStatus {
    /// Controller is starting up (waiting for initial delay before first sync)
    Starting,
    /// Controller is actively syncing data
    Syncing {
        /// Number of events processed in this sync cycle so far
        events_processed: usize,
    },
    /// Controller is idle, waiting for next sync interval
    Idle,
    /// Controller has been stopped gracefully
    Ended,
    /// Controller encountered an error (will retry on next sync interval)
    Error {
        /// Human-readable error message
        message: String,
    },
    /// The homeserver rejected the pubky's session, so its private data is no
    /// longer synced. Public data keeps syncing.
    ///
    /// The controller leaves the stored session secret in place, and doesn't use
    /// it again. Whoever manages the pubky's sessions decides what to do with it.
    SessionRejected {
        /// The stored session secret that was rejected
        secret: SessionSecret,
    },
    /// The pubky's mirror rejected its session, so nothing is copied to the
    /// mirror any more. The backup itself keeps syncing.
    ///
    /// As with `SessionRejected`, the controller leaves the stored mirror in
    /// place and doesn't use its session again.
    MirrorSessionRejected {
        /// The stored secret of the session on the mirror that was rejected
        secret: SessionSecret,
    },
    /// Copying to the pubky's mirror failed (will retry on next sync interval).
    /// The backup itself keeps syncing. Reported once per distinct failure.
    MirrorFailed {
        /// Human-readable error message
        message: String,
    },
    /// Copying to the pubky's mirror works again after a failure.
    MirrorRecovered,
}

/// Main backup controller which manages the backup process for a Pubky user.
///
/// The controller continuously syncs data from a Pubky homeserver to local storage,
/// processing events as they stream from the homeserver.
///
/// # Example
///
/// ```no_run
/// use pubky_backup_core::{AppStorage, BackupController, ControllerCommand, ControllerStatus};
/// use pubky::{Pubky, PublicKey};
/// use std::sync::Arc;
/// use std::str::FromStr;
/// use std::time::Duration;
/// use tokio::sync::{broadcast, mpsc, watch};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let storage = Arc::new(AppStorage::new()?);
///     let pubky_client = Arc::new(Pubky::new()?);
///     let pubky = PublicKey::from_str("your_pubky_here")?;
///
///     let (control_tx, control_rx) = mpsc::channel(5);
///     let (status_tx, mut status_rx) = broadcast::channel(5);
///     let (_interval_tx, interval_rx) = watch::channel(30u64);
///
///     let controller = BackupController::new(
///         pubky.clone(),
///         storage,
///         pubky_client,
///         Some(control_rx),
///         Some(status_tx),
///         interval_rx,
///     ).with_initial_delay(Duration::from_secs(5));
///
///     tokio::spawn(controller.run());
///
///     // Listen for status updates
///     tokio::spawn(async move {
///         while let Ok(update) = status_rx.recv().await {
///             let pubky = update.pubky;
///             match update.status {
///                 ControllerStatus::Starting => println!("{}: Starting...", pubky),
///                 ControllerStatus::Syncing { events_processed } => {
///                     println!("{}: Syncing... ({} events)", pubky, events_processed)
///                 },
///                 ControllerStatus::Idle => println!("{}: Idle", pubky),
///                 ControllerStatus::Ended => println!("{}: Ended", pubky),
///                 ControllerStatus::Error { message } => {
///                     println!("{}: Error: {}", pubky, message)
///                 },
///                 ControllerStatus::SessionRejected { .. } => {
///                     println!("{}: Session rejected", pubky)
///                 },
///                 ControllerStatus::MirrorSessionRejected { .. } => {
///                     println!("{}: Mirror session rejected", pubky)
///                 },
///                 ControllerStatus::MirrorFailed { message } => {
///                     println!("{}: Mirror error: {}", pubky, message)
///                 },
///                 ControllerStatus::MirrorRecovered => {
///                     println!("{}: Mirror recovered", pubky)
///                 },
///             }
///         }
///     });
///
///     control_tx.send(ControllerCommand::ForceSync).await?;
///
///     Ok(())
/// }
/// ```
pub struct BackupController {
    pubky: PublicKey,
    storage: Arc<AppStorage>,
    pubky_client: Arc<Pubky>,
    control_rx: Option<mpsc::Receiver<ControllerCommand>>,
    status_tx: Option<broadcast::Sender<ControllerUpdate>>,
    /// Initial delay before starting the first sync (for staggering multiple controllers)
    initial_delay: Duration,
    /// Shared sync interval receiver - reads current interval dynamically
    sync_interval_rx: watch::Receiver<u64>,
    /// Make no network calls: syncs find no events and no session is restored
    offline: bool,
    /// Session of a signed-in pubky, restored from the stored session secret
    session: Option<RestoredSession>,
    /// A stored session secret the homeserver rejected, which is not used again
    rejected_secret: Option<String>,
    /// Session on the pubky's mirror, restored from the stored mirror
    mirror: Option<RestoredMirror>,
    /// A stored mirror session secret the mirror rejected, which is not used again
    rejected_mirror_secret: Option<String>,
    /// The problem last reported for the pubky's mirror, to report each only once
    mirror_failure: Option<String>,
    /// Whether a sync is due now, rather than at the next interval
    sync_now: bool,
    /// The sync cycle in progress, which may take several batches
    cycle: Option<SyncCycle>,
}

/// Progress through a sync cycle, which syncs public data to the end, then
/// private data, then copies what was backed up to the mirror.
struct SyncCycle {
    /// Events processed in this cycle so far
    events: usize,
    /// The phase the cycle is in
    phase: SyncPhase,
}

/// The phases of a sync cycle, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncPhase {
    /// Backing up one scope of the pubky's data
    Backup(DataScope),
    /// Copying the backup to the pubky's mirror
    Mirror,
}

/// A session together with the stored secret it was restored from, so a
/// changed or removed secret is noticed.
///
/// Only the most recently restored session of a grant stays usable: each
/// restore exchanges the grant for a new access token, replacing the previous
/// one. So the controller restores once and keeps the session.
#[derive(Clone)]
struct RestoredSession {
    secret: String,
    session: PubkySession,
}

impl BackupController {
    /// Creates a new backup controller instance.
    ///
    /// # Arguments
    ///
    /// * `pubky` - The public key to backup data for
    /// * `storage` - Shared storage instance for persisting data
    /// * `pubky_client` - Pubky SDK client for connecting to the homeserver
    /// * `control_rx` - Optional receiver for control messages (Cancel, ForceSync)
    /// * `status_tx` - Optional sender for status updates (shared across all controllers)
    /// * `sync_interval_rx` - Watch receiver for dynamically reading the current sync interval (seconds)
    ///
    /// # Returns
    ///
    /// A new `BackupController` ready to be run via [`BackupController::run`]
    pub fn new(
        pubky: PublicKey,
        storage: Arc<AppStorage>,
        pubky_client: Arc<Pubky>,
        control_rx: Option<mpsc::Receiver<ControllerCommand>>,
        status_tx: Option<broadcast::Sender<ControllerUpdate>>,
        sync_interval_rx: watch::Receiver<u64>,
    ) -> Self {
        Self {
            pubky,
            storage,
            pubky_client,
            control_rx,
            status_tx,
            initial_delay: Duration::ZERO,
            sync_interval_rx,
            offline: false,
            session: None,
            rejected_secret: None,
            mirror: None,
            rejected_mirror_secret: None,
            mirror_failure: None,
            sync_now: false,
            cycle: None,
        }
    }

    /// Sets an initial delay before the first sync.
    ///
    /// This is useful for staggering multiple controllers to avoid
    /// overwhelming the network with simultaneous requests.
    ///
    /// During the delay, the controller emits `Starting` status and
    /// responds to commands:
    /// - `Cancel`: Stops the controller immediately
    /// - `ForceSync`: Skips the remaining delay and starts syncing
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Make no network calls.
    ///
    /// Offline, every sync finds no events and no session is restored, so the
    /// controller only keeps the data already on disk. For tests and for
    /// working on the sync loop without a network.
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Reads the current sync interval from the shared watch channel.
    fn sync_interval(&self) -> Duration {
        Duration::from_secs(*self.sync_interval_rx.borrow())
    }

    /// Runs the backup controller loop.
    ///
    /// This method consumes `self` and runs until:
    /// - A `Cancel` message is received via the control channel
    /// - The control channel is closed
    ///
    /// The controller will:
    /// 1. Emit `Starting` status and wait for initial delay (if any)
    /// 2. Poll for new events from the pubky's homeserver
    /// 3. Download and store new/updated resources
    /// 4. Delete resources that have been removed
    /// 5. Wait for the next sync interval
    /// 6. Emit status updates via the status channel
    ///
    /// On errors, the controller logs the error and retries on the next sync interval.
    ///
    /// Commands are handled between batches, so a long sync stays responsive:
    /// - `Cancel`: Stops after the current batch
    /// - `ForceSync`: Starts a new sync cycle right away
    pub async fn run(mut self) {
        self.send_status(ControllerStatus::Starting);
        if self.wait_initial_delay().await.is_break() {
            return;
        }

        // Sync right away, then on every interval
        self.sync_now = true;
        loop {
            if let Some(command) = self.try_recv_command() {
                if self.handle_command(Some(command)).is_break() {
                    return;
                }
            }

            if self.sync_now {
                self.sync_now = false;
                if self.sync_batch().await.is_continue() {
                    // More to sync in this cycle: go on without waiting
                    self.sync_now = true;
                    continue;
                }
            }

            if self.wait_for_next_sync().await.is_break() {
                return;
            }
        }
    }

    /// Wait out the initial delay, while responding to commands.
    ///
    /// Returns `Break` if the controller is to stop.
    async fn wait_initial_delay(&mut self) -> ControlFlow<()> {
        if self.initial_delay.is_zero() {
            return ControlFlow::Continue(());
        }
        tokio::select! {
            _ = time::sleep(self.initial_delay) => ControlFlow::Continue(()),
            // A ForceSync skips the rest of the delay
            command = Self::recv_command(&mut self.control_rx) => self.handle_command(command),
        }
    }

    /// Wait for the next sync interval, a change of the interval, or a command.
    ///
    /// Returns `Break` if the controller is to stop.
    async fn wait_for_next_sync(&mut self) -> ControlFlow<()> {
        let sleep = time::sleep(self.sync_interval());
        tokio::pin!(sleep);

        tokio::select! {
            _ = &mut sleep => {
                self.sync_now = true;
                ControlFlow::Continue(())
            }
            // Interval changed: loop back and sleep with the new value
            _ = self.sync_interval_rx.changed() => ControlFlow::Continue(()),
            command = Self::recv_command(&mut self.control_rx) => self.handle_command(command),
        }
    }

    /// Wait for the next command, or forever if there is no control channel.
    async fn recv_command(
        control_rx: &mut Option<mpsc::Receiver<ControllerCommand>>,
    ) -> Option<ControllerCommand> {
        match control_rx {
            Some(rx) => rx.recv().await,
            None => std::future::pending().await,
        }
    }

    /// Non-blocking check for a pending command on the control channel.
    fn try_recv_command(&mut self) -> Option<ControllerCommand> {
        self.control_rx.as_mut().and_then(|rx| rx.try_recv().ok())
    }

    /// Act on a command, or on the control channel having closed (`None`).
    ///
    /// Returns `Break` if the controller is to stop.
    fn handle_command(&mut self, command: Option<ControllerCommand>) -> ControlFlow<()> {
        match command {
            Some(ControllerCommand::ForceSync) => {
                info!("Force sync triggered for {}", self.pubky);
                self.sync_now = true;
                self.cycle = None;
                ControlFlow::Continue(())
            }
            Some(ControllerCommand::Cancel) => {
                info!("Backup controller for {} cancelled", self.pubky);
                self.send_status(ControllerStatus::Ended);
                ControlFlow::Break(())
            }
            None => {
                warn!(
                    "Control channel of backup controller for {} closed",
                    self.pubky
                );
                self.send_status(ControllerStatus::Ended);
                ControlFlow::Break(())
            }
        }
    }

    fn send_status(&self, status: ControllerStatus) {
        if let Some(tx) = &self.status_tx {
            let update = ControllerUpdate {
                pubky: self.pubky.clone(),
                status,
            };
            if let Err(e) = tx.send(update) {
                warn!(
                    "Failed to send status update (no receivers or lagging): {:?}",
                    e
                );
            }
        }
    }

    /// Sync one batch of the cycle in progress, starting a cycle if there is none.
    ///
    /// Returns `Continue` if the cycle has more to sync, or `Break` once it is
    /// complete or has failed.
    async fn sync_batch(&mut self) -> ControlFlow<()> {
        let mut cycle = match self.cycle.take() {
            Some(cycle) => cycle,
            None => self.start_cycle(),
        };

        match self.perform_sync_batch(&mut cycle.phase).await {
            Ok(ControlFlow::Continue(batch_count)) => {
                cycle.events += batch_count;
                self.send_status(ControllerStatus::Syncing {
                    events_processed: cycle.events,
                });
                self.cycle = Some(cycle);
                ControlFlow::Continue(())
            }
            Ok(ControlFlow::Break(batch_count)) => {
                cycle.events += batch_count;
                if cycle.events > 0 {
                    self.send_status(ControllerStatus::Syncing {
                        events_processed: cycle.events,
                    });
                }
                self.send_status(ControllerStatus::Idle);
                ControlFlow::Break(())
            }
            Err(e) => {
                let message = format!("Sync error: {}", e);
                error!("{}: {}", self.pubky, message);
                self.send_status(ControllerStatus::Error { message });
                // Retried on the next sync interval
                ControlFlow::Break(())
            }
        }
    }

    /// Start a sync cycle, which begins with public data.
    fn start_cycle(&self) -> SyncCycle {
        info!("Syncing key: {}", self.pubky);
        self.send_status(ControllerStatus::Syncing {
            events_processed: 0,
        });
        SyncCycle {
            events: 0,
            phase: SyncPhase::Backup(DataScope::Public),
        }
    }

    /// Perform a single sync batch of the cycle's current phase.
    ///
    /// Once public data is synced to the end, moves the cycle on to private
    /// data, and after that to copying to the mirror. Returns
    /// `ControlFlow::Continue(count)` if the cycle has more to do, or
    /// `ControlFlow::Break(count)` if it is complete. The count is the number
    /// of events backed up.
    async fn perform_sync_batch(
        &mut self,
        phase: &mut SyncPhase,
    ) -> Result<ControlFlow<usize, usize>, SyncError> {
        match phase {
            SyncPhase::Backup(DataScope::Public) => {
                let batch = self.perform_scope_batch(SyncScope::Public).await?;
                let ControlFlow::Break(count) = batch else {
                    return Ok(batch);
                };
                *phase = SyncPhase::Backup(DataScope::Private);
                Ok(ControlFlow::Continue(count))
            }
            SyncPhase::Backup(DataScope::Private) => {
                let batch = self.perform_private_batch().await;
                let ControlFlow::Break(count) = batch else {
                    return Ok(batch);
                };
                // Copying follows the backup, so it only ever sees complete files
                *phase = SyncPhase::Mirror;
                Ok(ControlFlow::Continue(count))
            }
            SyncPhase::Mirror => Ok(if self.perform_mirror_batch().await {
                ControlFlow::Continue(0)
            } else {
                ControlFlow::Break(0)
            }),
        }
    }

    /// Perform a single sync batch of private data, if the pubky is signed in.
    ///
    /// Failures are logged rather than returned: they only stop the sync of
    /// private data until the next cycle, and the sync of public data still counts.
    async fn perform_private_batch(&mut self) -> ControlFlow<usize, usize> {
        let restored = match self.resolve_session().await {
            Ok(Some(restored)) => restored,
            Ok(None) => return ControlFlow::Break(0),
            Err(e) => {
                self.log_private_sync_failure(&e).await;
                return ControlFlow::Break(0);
            }
        };

        match self
            .perform_scope_batch(SyncScope::Private(&restored.session))
            .await
        {
            Ok(batch) => batch,
            Err(SyncError::SessionRejected) => {
                self.reject_session(restored.secret);
                ControlFlow::Break(0)
            }
            Err(e) => {
                self.log_private_sync_failure(&e).await;
                ControlFlow::Break(0)
            }
        }
    }

    async fn log_private_sync_failure(&self, error: &SyncError) {
        warn!("Failed to sync private data of {}: {}", self.pubky, error);
        let _ = self
            .storage
            .write_global_error("private_sync", &format!("Private sync error: {}", error))
            .await;
    }

    /// Process events of one scope by streaming from the homeserver (or an
    /// empty stream offline).
    ///
    /// Fetches events from the scope's cursor position and processes them.
    async fn perform_scope_batch(
        &self,
        scope: SyncScope<'_>,
    ) -> Result<ControlFlow<usize, usize>, SyncError> {
        let cursor = self
            .storage
            .read_cursor(&self.pubky, scope.data_scope())
            .await?;

        let event_stream: EventStream = if self.offline {
            Box::pin(futures_util::stream::empty())
        } else {
            events::create_event_stream(&self.pubky_client, &self.pubky, scope, cursor).await?
        };

        self.process_event_stream(event_stream, cursor, scope).await
    }

    /// Get the session to sync private data with, or `None` if the pubky is not signed in.
    ///
    /// The stored session secret is the source of truth: signing in or out changes
    /// it, and the controller picks that up on its next sync.
    async fn resolve_session(&mut self) -> Result<Option<RestoredSession>, SyncError> {
        // Restoring validates the session with the homeserver, so skip it when offline
        if self.offline {
            return Ok(None);
        }

        let Some(secret) = self.storage.read_session_secret(&self.pubky)? else {
            self.session = None;
            return Ok(None);
        };

        if self.rejected_secret.as_ref() == Some(&secret) {
            return Ok(None);
        }

        if let Some(restored) = self.session.as_ref().filter(|s| s.secret == secret) {
            return Ok(Some(restored.clone()));
        }

        match self.pubky_client.restore_session(&secret).await {
            Ok(session) => {
                info!("Restored session for key: {}", self.pubky);
                let restored = RestoredSession { secret, session };
                self.session = Some(restored.clone());
                Ok(Some(restored))
            }
            Err(e) if is_session_rejection(&e) => {
                warn!(
                    "Stored session for {} is no longer valid: {}",
                    self.pubky, e
                );
                self.reject_session(secret);
                Ok(None)
            }
            Err(e) => Err(SyncError::Internal(format!(
                "Failed to restore session: {}",
                e
            ))),
        }
    }

    /// Stop using a session secret the homeserver no longer accepts, and report
    /// it, so only public data is synced until the pubky signs in again.
    fn reject_session(&mut self, secret: String) {
        self.session = None;
        self.rejected_secret = Some(secret.clone());
        self.send_status(ControllerStatus::SessionRejected {
            secret: SessionSecret::new(secret),
        });
    }

    /// Save cursor progress of a scope if we have a cursor value.
    async fn save_cursor_if_present(
        &self,
        cursor: Option<u64>,
        scope: SyncScope<'_>,
    ) -> Result<(), SyncError> {
        if let Some(c) = cursor {
            self.storage
                .write_cursor(&self.pubky, scope.data_scope(), c)
                .await?;
        }
        Ok(())
    }

    /// Process events from a stream, saving cursor progress periodically and on error.
    async fn process_event_stream(
        &self,
        mut event_stream: EventStream,
        initial_cursor: Option<u64>,
        scope: SyncScope<'_>,
    ) -> Result<ControlFlow<usize, usize>, SyncError> {
        let mut events_processed = 0;
        let mut last_cursor: Option<u64> = initial_cursor;

        // Process events as they stream in
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(event) => {
                    let event_cursor = event.cursor.id();
                    self.process_single_event(&event, scope).await?;
                    events_processed += 1;
                    last_cursor = Some(event_cursor);

                    // Save cursor periodically (every EVENT_BATCH_SIZE events)
                    if events_processed % EVENT_BATCH_SIZE as usize == 0 {
                        self.save_cursor_if_present(last_cursor, scope).await?;
                    }
                }
                Err(e) => {
                    let _ = self
                        .storage
                        .write_global_error("event_stream", &format!("Event stream error: {}", e))
                        .await;
                    // Save progress and break out of stream loop. The next sync interval will reconnect
                    self.save_cursor_if_present(last_cursor, scope).await?;
                    break;
                }
            }
        }

        info!(
            "Processed {} events for key: {}",
            events_processed, self.pubky
        );

        // Save final cursor
        self.save_cursor_if_present(last_cursor, scope).await?;

        if events_processed >= EVENT_BATCH_SIZE as usize {
            // Full batch processed — there may be more events, fetch again immediately
            Ok(ControlFlow::Continue(events_processed))
        } else {
            // Partial or empty batch — we've caught up, wait for next sync interval
            Ok(ControlFlow::Break(events_processed))
        }
    }

    /// Process a single event
    async fn process_single_event(
        &self,
        event: &Event,
        scope: SyncScope<'_>,
    ) -> Result<(), SyncError> {
        // Skip events for other pubkys
        if event.resource.owner != self.pubky {
            return Ok(());
        }

        match event.event_type {
            // Offline there is nothing to fetch the data from. An offline sync
            // finds no events, so this only matters for a stream fed directly,
            // as the tests do.
            EventType::Put { .. } if self.offline => {}
            EventType::Put { .. } => {
                debug!("Processing PUT event for: {}", event.resource);
                match fetcher::fetch_resource_data(&self.pubky_client, scope, &event.resource).await
                {
                    Ok(data_vec) => {
                        // Skip storing empty data (404 responses)
                        if !data_vec.is_empty() {
                            self.storage.write(&event.resource, data_vec).await?;
                        }
                    }
                    // Stop here so the cursor doesn't move past data we couldn't read
                    Err(e @ SyncError::SessionRejected) => return Err(e),
                    Err(e) => {
                        // Log fetch errors and continue processing other events
                        self.storage
                            .write_global_error(
                                &event.resource.to_string(),
                                &format!("Fetch failed: {}", e),
                            )
                            .await?;
                    }
                }
            }
            EventType::Delete => {
                debug!("Processing DEL event for: {}", event.resource);
                self.storage.delete(&event.resource).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppStorage;
    use pubky::PubkyResource;
    use std::str::FromStr;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    fn test_pubky() -> PublicKey {
        PublicKey::from_str(TEST_PUBKY).unwrap()
    }

    // Test helper to create a storage instance with a temporary directory
    pub(super) fn create_test_storage() -> (Arc<AppStorage>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    /// An offline controller for the test key, so that tests make no network calls.
    fn create_test_controller(
        storage: Arc<AppStorage>,
        control_rx: Option<mpsc::Receiver<ControllerCommand>>,
        status_tx: Option<broadcast::Sender<ControllerUpdate>>,
    ) -> BackupController {
        let (interval_tx, interval_rx) = watch::channel(DEFAULT_SYNC_INTERVAL_SECONDS);
        // Keep the interval channel open for as long as the controller runs
        std::mem::forget(interval_tx);
        BackupController::new(
            test_pubky(),
            storage,
            Arc::new(Pubky::testnet().expect("Failed to create testnet client")),
            control_rx,
            status_tx,
            interval_rx,
        )
        .offline(true)
    }

    /// An offline controller without channels, for calling its methods directly.
    pub(super) fn create_detached_controller(storage: Arc<AppStorage>) -> BackupController {
        create_test_controller(storage, None, None)
    }

    /// An offline controller to run as a task, with the channels to control it
    /// and to observe its status.
    pub(super) fn create_controlled_controller(
        storage: Arc<AppStorage>,
    ) -> (
        BackupController,
        mpsc::Sender<ControllerCommand>,
        broadcast::Receiver<ControllerUpdate>,
    ) {
        let (control_tx, control_rx) = mpsc::channel(5);
        let (status_tx, status_rx) = broadcast::channel(10);
        let controller = create_test_controller(storage, Some(control_rx), Some(status_tx));
        (controller, control_tx, status_rx)
    }

    /// The controller's next status, which must arrive within `timeout`.
    async fn next_status(
        status_rx: &mut broadcast::Receiver<ControllerUpdate>,
        timeout: Duration,
    ) -> ControllerStatus {
        tokio::time::timeout(timeout, status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap()
            .status
    }

    #[tokio::test]
    async fn test_controller_runs_and_can_be_cancelled() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);

        let handle = tokio::spawn(controller.run());

        let status = next_status(&mut status_rx, Duration::from_secs(5)).await;
        assert!(
            matches!(status, ControllerStatus::Starting),
            "First status should be Starting, got {:?}",
            status
        );

        let status = next_status(&mut status_rx, Duration::from_secs(5)).await;
        match status {
            ControllerStatus::Syncing { .. } | ControllerStatus::Idle => {}
            _ => panic!("Unexpected ControllerStatus after Starting: {:?}", status),
        }

        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("Controller should finish")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_force_sync() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);

        tokio::spawn(controller.run());

        let status = next_status(&mut status_rx, Duration::from_secs(5)).await;
        assert!(matches!(status, ControllerStatus::Starting));

        control_tx.send(ControllerCommand::ForceSync).await.unwrap();

        let status = next_status(&mut status_rx, Duration::from_secs(5)).await;
        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Expected Syncing status after force sync, got {:?}",
            status
        );

        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_partial_batch_completes_and_saves_cursor() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());

        // A partial batch (< EVENT_BATCH_SIZE) returns Break: caught up, no re-fetch needed
        let stream = events::test_helpers::create_test_event_stream(None);
        let result = controller
            .process_event_stream(stream, None, SyncScope::Public)
            .await
            .unwrap();
        assert!(matches!(result, ControlFlow::Break(_)));

        let cursor = storage
            .read_cursor(&test_pubky(), DataScope::Public)
            .await
            .unwrap();
        assert_eq!(cursor, Some(3));
    }

    #[tokio::test]
    async fn test_process_single_event_skips_put_offline() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());

        let resource = PubkyResource::new(test_pubky(), "/pub/test.json").unwrap();
        let event = Event {
            event_type: events::test_helpers::put_event_type(),
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
        };

        controller
            .process_single_event(&event, SyncScope::Public)
            .await
            .unwrap();

        // Offline there is nothing to fetch, so nothing is written
        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_process_single_event_handles_delete() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());

        let resource = PubkyResource::new(test_pubky(), "/pub/test.json").unwrap();
        storage
            .write(&resource, b"test data".to_vec())
            .await
            .unwrap();
        assert!(storage.read(&resource).await.is_ok());

        let event = Event {
            event_type: EventType::Delete,
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
        };

        controller
            .process_single_event(&event, SyncScope::Public)
            .await
            .unwrap();

        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_process_single_event_skips_other_pubky() {
        let (storage, _temp_dir) = create_test_storage();
        // Online, so that a PUT for the controller's own key would be fetched
        let controller = create_detached_controller(storage.clone()).offline(false);
        let other_pubky =
            PublicKey::from_str("o4dksfbqk85ogzdb5osziw6befigbuxmuxkuxq8434q89uj56uxo").unwrap();

        let resource = PubkyResource::new(other_pubky, "/pub/test.json").unwrap();
        let event = Event {
            event_type: events::test_helpers::put_event_type(),
            resource: resource.clone(),
            cursor: pubky::EventCursor::new(1),
        };

        controller
            .process_single_event(&event, SyncScope::Public)
            .await
            .unwrap();

        assert!(storage.read(&resource).await.is_err());
    }

    #[tokio::test]
    async fn test_waiting_for_the_next_sync_wakes_on_an_interval_change() {
        let (storage, _temp_dir) = create_test_storage();
        let (interval_tx, interval_rx) = watch::channel(DEFAULT_SYNC_INTERVAL_SECONDS);
        let mut controller = BackupController::new(
            test_pubky(),
            storage,
            Arc::new(Pubky::testnet().expect("Failed to create testnet client")),
            None,
            None,
            interval_rx,
        )
        .offline(true);

        let woken = tokio::time::timeout(Duration::from_millis(500), async {
            let change = async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                interval_tx.send(MIN_SYNC_INTERVAL_SECONDS).unwrap();
            };
            tokio::join!(controller.wait_for_next_sync(), change).0
        })
        .await
        .expect("A change of interval should end the wait");

        assert!(woken.is_continue());
        // The change is not a sync: the loop goes back to waiting with the new interval
        assert!(!controller.sync_now);
    }

    #[tokio::test]
    async fn test_force_sync_restarts_the_cycle() {
        let (storage, _temp_dir) = create_test_storage();
        let mut controller = create_detached_controller(storage);
        controller.cycle = Some(SyncCycle {
            events: 5,
            phase: SyncPhase::Backup(DataScope::Private),
        });

        let flow = controller.handle_command(Some(ControllerCommand::ForceSync));

        assert!(flow.is_continue());
        assert!(controller.sync_now);
        assert!(
            controller.cycle.is_none(),
            "The next sync should start a fresh cycle from public data"
        );
    }

    #[tokio::test]
    async fn test_cancel_and_a_closed_control_channel_end_the_controller() {
        let (storage, _temp_dir) = create_test_storage();
        let (mut controller, _control_tx, mut status_rx) = create_controlled_controller(storage);

        assert!(controller
            .handle_command(Some(ControllerCommand::Cancel))
            .is_break());
        assert!(matches!(
            status_rx.try_recv().unwrap().status,
            ControllerStatus::Ended
        ));

        assert!(controller.handle_command(None).is_break());
        assert!(matches!(
            status_rx.try_recv().unwrap().status,
            ControllerStatus::Ended
        ));
    }

    #[tokio::test]
    async fn test_stream_error_saves_cursor_progress() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());

        // A stream that yields 3 events then fails
        let failing_stream = events::test_helpers::create_failing_test_event_stream(3);

        let result = controller
            .process_event_stream(failing_stream, None, SyncScope::Public)
            .await;

        // Stream errors are handled gracefully, not propagated
        let flow = result.expect("Stream errors should be handled gracefully, not propagated");
        assert!(
            matches!(flow, ControlFlow::Break(_)),
            "Partial batch should return Break, got {:?}",
            flow
        );

        let saved_cursor = storage
            .read_cursor(&test_pubky(), DataScope::Public)
            .await
            .unwrap();
        assert_eq!(
            saved_cursor,
            Some(3),
            "Cursor should be saved at last successful event before error"
        );
    }

    #[tokio::test]
    async fn test_multi_batch_accumulates_cycle_events() {
        // Verify that when process_event_stream returns Continue (full batch),
        // the caller can accumulate counts across batches, and the final Break
        // batch adds to the total. This mirrors the accumulation logic in run().
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage.clone());

        let mut cycle_events: usize = 0;

        // First batch: exactly EVENT_BATCH_SIZE events → should return Continue
        let stream =
            events::test_helpers::create_sized_test_event_stream(EVENT_BATCH_SIZE as usize, 0);
        let result = controller
            .process_event_stream(stream, None, SyncScope::Public)
            .await
            .unwrap();
        match result {
            ControlFlow::Continue(count) => {
                cycle_events += count;
                assert_eq!(count, EVENT_BATCH_SIZE as usize);
            }
            ControlFlow::Break(_) => panic!("Full batch should return Continue"),
        }

        // Second batch: 7 events (< EVENT_BATCH_SIZE) → should return Break
        let cursor = storage
            .read_cursor(&test_pubky(), DataScope::Public)
            .await
            .unwrap();
        let stream = events::test_helpers::create_sized_test_event_stream(7, cursor.unwrap_or(0));
        let result = controller
            .process_event_stream(stream, cursor, SyncScope::Public)
            .await
            .unwrap();
        match result {
            ControlFlow::Break(count) => {
                cycle_events += count;
                assert_eq!(count, 7);
            }
            ControlFlow::Continue(_) => panic!("Partial batch should return Break"),
        }

        assert_eq!(
            cycle_events,
            EVENT_BATCH_SIZE as usize + 7,
            "cycle_events should accumulate across batches"
        );
    }

    #[tokio::test]
    async fn test_controller_runs_without_channels() {
        let (storage, _temp_dir) = create_test_storage();
        let controller = create_detached_controller(storage);

        // Without a control channel the controller loops forever, so bound the run
        let result = tokio::time::timeout(Duration::from_millis(500), controller.run()).await;

        assert!(result.is_err(), "Should timeout since no cancel signal");
    }

    #[tokio::test]
    async fn test_controller_status_ended_on_cancel() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);

        let handle = tokio::spawn(controller.run());

        next_status(&mut status_rx, Duration::from_secs(2)).await;

        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        let mut received_ended = false;
        for _ in 0..5 {
            if let Ok(Ok(status)) =
                tokio::time::timeout(Duration::from_millis(500), status_rx.recv()).await
            {
                if matches!(status.status, ControllerStatus::Ended) {
                    received_ended = true;
                    break;
                }
            }
        }
        assert!(received_ended, "Should receive Ended status after cancel");

        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("Controller should finish")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_emits_starting_status() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);

        tokio::spawn(controller.run());

        let update = tokio::time::timeout(Duration::from_secs(5), status_rx.recv())
            .await
            .expect("Should receive status")
            .unwrap();
        assert_eq!(update.pubky, test_pubky());
        assert!(
            matches!(update.status, ControllerStatus::Starting),
            "First status should be Starting, got {:?}",
            update.status
        );

        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_initial_delay_responds_to_cancel() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);
        let controller = controller.with_initial_delay(Duration::from_secs(60));

        let handle = tokio::spawn(controller.run());

        let status = next_status(&mut status_rx, Duration::from_secs(1)).await;
        assert!(
            matches!(status, ControllerStatus::Starting),
            "Should receive Starting status"
        );

        // Cancel during the starting phase
        control_tx.send(ControllerCommand::Cancel).await.unwrap();

        let status = next_status(&mut status_rx, Duration::from_secs(1)).await;
        assert!(
            matches!(status, ControllerStatus::Ended),
            "Should receive Ended status after cancel during starting phase"
        );

        // The controller finishes without waiting out the delay
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("Controller should finish quickly after cancel during starting phase")
            .unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_initial_delay_responds_to_force_sync() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);
        let controller = controller.with_initial_delay(Duration::from_secs(60));

        tokio::spawn(controller.run());

        let status = next_status(&mut status_rx, Duration::from_secs(1)).await;
        assert!(matches!(status, ControllerStatus::Starting));

        // ForceSync during the starting phase skips the delay
        control_tx.send(ControllerCommand::ForceSync).await.unwrap();

        let status = next_status(&mut status_rx, Duration::from_secs(2)).await;
        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Should receive Syncing status after ForceSync during starting phase, got {:?}",
            status
        );

        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_with_zero_delay_proceeds_immediately() {
        let (storage, _temp_dir) = create_test_storage();
        let (controller, control_tx, mut status_rx) = create_controlled_controller(storage);

        tokio::spawn(controller.run());

        let status = next_status(&mut status_rx, Duration::from_millis(100)).await;
        assert!(matches!(status, ControllerStatus::Starting));

        let status = next_status(&mut status_rx, Duration::from_millis(500)).await;
        assert!(
            matches!(status, ControllerStatus::Syncing { .. }),
            "Should proceed to Syncing immediately with zero delay, got {:?}",
            status
        );

        control_tx.send(ControllerCommand::Cancel).await.unwrap();
    }

    #[tokio::test]
    async fn test_resolve_session_without_stored_secret() {
        let (storage, _temp_dir) = create_test_storage();
        // Online, so that a stored secret would be restored
        let mut controller = create_detached_controller(storage).offline(false);

        let session = controller.resolve_session().await.unwrap();

        assert!(session.is_none(), "A signed-out key has no session");
    }

    #[tokio::test]
    async fn test_resolve_session_offline_keeps_stored_secret() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_session_secret(&test_pubky(), "secret")
            .unwrap();
        let (mut controller, _control_tx, mut status_rx) =
            create_controlled_controller(storage.clone());

        let session = controller.resolve_session().await.unwrap();

        // Offline the session can't be validated, but that must not sign the key out
        assert!(session.is_none());
        assert!(storage
            .read_session_secret(&test_pubky())
            .unwrap()
            .is_some());
        assert!(status_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_reject_session_reports_secret_and_leaves_it_stored() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .write_session_secret(&test_pubky(), "secret")
            .unwrap();
        let (mut controller, _control_tx, mut status_rx) =
            create_controlled_controller(storage.clone());

        controller.reject_session("secret".to_string());

        match status_rx.try_recv().unwrap().status {
            ControllerStatus::SessionRejected { secret } => {
                assert_eq!(secret, SessionSecret::new("secret".to_string()));
            }
            other => panic!("Expected SessionRejected, got {:?}", other),
        }
        // Whoever manages the session decides what happens to the stored secret
        assert_eq!(
            storage
                .read_session_secret(&test_pubky())
                .unwrap()
                .as_deref(),
            Some("secret")
        );
        assert_eq!(controller.rejected_secret.as_deref(), Some("secret"));
    }

    #[test]
    fn test_session_secret_is_hidden_from_debug_output() {
        let secret = SessionSecret::new("very-secret".to_string());

        assert!(!format!("{:?}", secret).contains("very-secret"));
    }

    #[tokio::test]
    async fn test_sync_cycle_moves_from_public_to_private_data_to_the_mirror() {
        let (storage, _temp_dir) = create_test_storage();
        let mut controller = create_detached_controller(storage.clone());
        let mut phase = SyncPhase::Backup(DataScope::Public);

        // Public data is caught up, so the cycle moves on to private data
        let public = controller.perform_sync_batch(&mut phase).await.unwrap();
        assert_eq!(public, ControlFlow::Continue(0));
        assert_eq!(phase, SyncPhase::Backup(DataScope::Private));

        // A signed-out key has no private data to sync, so the cycle moves on
        // to the mirror
        let private = controller.perform_sync_batch(&mut phase).await.unwrap();
        assert_eq!(private, ControlFlow::Continue(0));
        assert_eq!(phase, SyncPhase::Mirror);
        assert_eq!(
            storage
                .read_cursor(&test_pubky(), DataScope::Private)
                .await
                .unwrap(),
            None
        );

        // A key without a mirror has nothing to copy, which ends the cycle
        let mirror = controller.perform_sync_batch(&mut phase).await.unwrap();
        assert_eq!(mirror, ControlFlow::Break(0));
    }
}
