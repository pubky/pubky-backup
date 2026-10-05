//! Integration tests using real pubky-testnet infrastructure.
//!
//! These tests spin up an ephemeral testnet with a Docker PostgreSQL container and a
//! real homeserver, testing the backup controller against actual network operations.
//! Docker must be running on the host.
//!
//! # Running the tests
//!
//! ```bash
//! cargo test -p pubky-backup-core --test integration_tests
//! ```
//!
//! A single testnet instance is shared across all tests to minimize startup overhead.
//! Tests are serialized using `serial_test` to ensure proper sequencing.

use pubky::{Keypair, PubkyResource, PubkySession, PubkySigner, PublicKey};
use pubky_backup_core::{
    AppStorage, AuthStatus, BackupController, BackupManager, BackupManagerConfig,
    ControllerCommand, ControllerStatus, KeyState, KeyStatus,
};
use pubky_testnet::{docker_postgres::DockerPostgres, EphemeralTestnet};
use serial_test::serial;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc, watch};

use pubky_backup_core::sync::DEFAULT_SYNC_INTERVAL_SECONDS;

fn create_test_interval_rx() -> (watch::Sender<u64>, watch::Receiver<u64>) {
    watch::channel(DEFAULT_SYNC_INTERVAL_SECONDS)
}

/// Helper to create a storage instance with a temporary directory
fn create_test_storage() -> (Arc<AppStorage>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();
    (Arc::new(storage), temp_dir)
}

/// Shared testnet instance across all integration tests.
/// Uses a dedicated tokio runtime to keep the testnet alive across test boundaries.
static SHARED_TESTNET: OnceLock<(tokio::runtime::Runtime, EphemeralTestnet)> = OnceLock::new();

/// Get the shared testnet instance, initializing it on first use.
/// The testnet runs in its own dedicated runtime to survive across test boundaries.
async fn get_shared_testnet() -> &'static EphemeralTestnet {
    // Use spawn_blocking to avoid nested runtime issues
    tokio::task::spawn_blocking(|| {
        let (_, testnet) = SHARED_TESTNET.get_or_init(|| {
            // Create a dedicated runtime for the testnet
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create testnet runtime");

            let testnet = rt.block_on(async {
                // The testnet lives in a static, which is never dropped. Use the shared
                // container, which is removed by an `atexit` hook, so it isn't left running.
                let postgres = DockerPostgres::shared()
                    .await
                    .connection_string()
                    .expect("Failed to get docker postgres connection string");
                EphemeralTestnet::builder()
                    .postgres(postgres)
                    // Sign-in approvals travel through the relay
                    .with_http_relay()
                    .build()
                    .await
                    .expect("Failed to start testnet with docker postgres")
            });

            (rt, testnet)
        });
        testnet
    })
    .await
    .expect("Failed to get testnet")
}

/// Helper to create an account on the homeserver, returning a cookie session.
///
/// The SDK deprecates cookie auth in favour of grants, but cookie auth is what we use.
#[allow(deprecated)]
async fn signup_with_cookie(signer: &PubkySigner, homeserver_pk: &PublicKey) -> PubkySession {
    signer.signup_cookie(homeserver_pk, None).await.unwrap()
}

/// Helper to run a controller until it reaches Idle state (sync complete).
///
/// Spawns the controller's run loop and waits for it to transition to Idle,
/// then cancels it. Times out after 30 seconds.
async fn run_controller_until_idle(
    user_pk: PublicKey,
    storage: Arc<AppStorage>,
    pubky_client: Arc<pubky::Pubky>,
) {
    let (control_tx, control_rx) = mpsc::channel(5);
    let (status_tx, mut status_rx) = broadcast::channel(10);

    let controller = BackupController::new(
        user_pk,
        storage,
        pubky_client,
        Some(control_rx),
        Some(status_tx),
        create_test_interval_rx().1,
    );

    // Spawn the controller
    let handle = tokio::spawn(controller.run());

    // Wait for Idle status (sync complete)
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match status_rx.recv().await {
                Ok(ControllerStatus::Idle { .. }) => break,
                Ok(_) => continue,
                Err(_) => panic!("Status channel closed unexpectedly"),
            }
        }
    })
    .await;

    // Cancel the controller
    let _ = control_tx.send(ControllerCommand::Cancel).await;

    // Wait for controller to finish
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

    result.expect("Controller should reach Idle state within 30 seconds");
}

/// Tests core backup functionality: PUT sync, DELETE sync, and cursor persistence.
///
/// This test combines several related scenarios to minimize testnet overhead:
/// 1. Syncing PUT events backs up data correctly
/// 2. Cursor persists and advances across sync operations
/// 3. Syncing DELETE events removes backed up data
#[tokio::test]
#[serial]
async fn test_backup_sync_put_delete_and_cursor() {
    let testnet = get_shared_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    // Create a user for this test
    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signup_with_cookie(&signer, &homeserver_pk).await;
    let user_pk = keypair.public_key();

    // === Part 1: Write test data ===
    session
        .storage()
        .put("/pub/test/file.txt", "hello world")
        .await
        .unwrap();
    session
        .storage()
        .put(
            "/pub/posts/001.json",
            r#"{"id":"001","content":"First post"}"#,
        )
        .await
        .unwrap();
    session
        .storage()
        .put("/pub/to_delete.txt", "this will be deleted")
        .await
        .unwrap();

    // === Part 2: Test PUT sync ===
    // Create backup controller and sync
    let (storage, _temp_dir) = create_test_storage();
    run_controller_until_idle(user_pk.clone(), storage.clone(), pubky_client.clone()).await;

    // Verify data was backed up
    let resource = PubkyResource::new(user_pk.clone(), "/pub/test/file.txt").unwrap();
    let backed_up_data = storage.as_ref().read(&resource).await.unwrap();
    assert_eq!(backed_up_data, b"hello world");

    let resource = PubkyResource::new(user_pk.clone(), "/pub/posts/001.json").unwrap();
    let backed_up_data = storage.as_ref().read(&resource).await.unwrap();
    assert!(String::from_utf8_lossy(&backed_up_data).contains("First post"));

    let resource_to_delete = PubkyResource::new(user_pk.clone(), "/pub/to_delete.txt").unwrap();
    assert!(
        storage.as_ref().read(&resource_to_delete).await.is_ok(),
        "File should exist before deletion"
    );

    // === Part 3: Test cursor persistence ===
    let cursor1 = storage.read_cursor(&user_pk).await.unwrap();
    assert!(cursor1.is_some(), "Cursor should be saved after first sync");

    // Write more data
    session
        .storage()
        .put("/pub/file2.txt", "content2")
        .await
        .unwrap();

    // Sync again
    run_controller_until_idle(user_pk.clone(), storage.clone(), pubky_client.clone()).await;

    // Cursor should have advanced
    let cursor2 = storage.read_cursor(&user_pk).await.unwrap();
    assert!(cursor2.is_some());
    assert!(
        cursor2 > cursor1,
        "Cursor should advance after processing new events"
    );

    // Verify new file exists
    let resource2 = PubkyResource::new(user_pk.clone(), "/pub/file2.txt").unwrap();
    assert!(storage.as_ref().read(&resource2).await.is_ok());

    // === Part 4: Test DELETE sync ===
    // Delete one file on homeserver
    session
        .storage()
        .delete("/pub/to_delete.txt")
        .await
        .unwrap();

    // Sync again
    run_controller_until_idle(user_pk.clone(), storage.clone(), pubky_client.clone()).await;

    // Verify: deleted file should be gone, other files should remain
    assert!(
        storage.as_ref().read(&resource_to_delete).await.is_err(),
        "Deleted file should be removed from backup"
    );
    assert!(
        storage.as_ref().read(&resource).await.is_ok(),
        "Other files should still exist"
    );
}

/// Tests the backup controller run loop: ForceSync and Cancel messages.
///
/// This test combines run loop scenarios to minimize testnet overhead:
/// 1. ForceSync triggers an immediate sync
/// 2. Cancel stops the controller gracefully
#[tokio::test]
#[serial]
async fn test_backup_controller_run_loop() {
    let testnet = get_shared_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signup_with_cookie(&signer, &homeserver_pk).await;
    let user_pk = keypair.public_key();

    // Write some data
    session
        .storage()
        .put("/pub/test.txt", "test content")
        .await
        .unwrap();

    let (storage, _temp_dir) = create_test_storage();
    let (control_tx, control_rx) = tokio::sync::mpsc::channel(5);
    let (status_tx, mut status_rx) = broadcast::channel(10);

    let controller = BackupController::new(
        user_pk,
        storage,
        pubky_client,
        Some(control_rx),
        Some(status_tx),
        create_test_interval_rx().1,
    );

    // Spawn the controller
    let handle = tokio::spawn(controller.run());

    // === Part 1: Test initial status ===
    let status = tokio::time::timeout(Duration::from_secs(10), status_rx.recv())
        .await
        .expect("Should receive status within timeout")
        .unwrap();

    match status {
        ControllerStatus::Starting { .. }
        | ControllerStatus::Syncing { .. }
        | ControllerStatus::Idle { .. } => {}
        other => panic!("Unexpected initial status: {:?}", other),
    }

    // === Part 2: Test ForceSync ===
    control_tx.send(ControllerCommand::ForceSync).await.unwrap();

    // Collect status updates until we see Syncing
    let mut saw_syncing = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(5), status_rx.recv()).await {
            Ok(Ok(ControllerStatus::Syncing { .. })) => {
                saw_syncing = true;
                break;
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    assert!(
        saw_syncing,
        "Should have seen Syncing status after ForceSync"
    );

    // === Part 3: Test Cancel ===
    control_tx.send(ControllerCommand::Cancel).await.unwrap();

    // Controller should finish
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("Controller should finish after cancel")
        .unwrap();
}

/// Tests that multiple users on the same homeserver are properly isolated.
///
/// Each user's backup should only contain their own data.
#[tokio::test]
#[serial]
async fn test_multiple_users_isolation() {
    let testnet = get_shared_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    // Create two users
    let keypair1 = Keypair::random();
    let keypair2 = Keypair::random();

    let signer1 = pubky_client.signer(keypair1.clone());
    let signer2 = pubky_client.signer(keypair2.clone());

    let session1 = signup_with_cookie(&signer1, &homeserver_pk).await;
    let session2 = signup_with_cookie(&signer2, &homeserver_pk).await;

    let user1_pk = keypair1.public_key();
    let user2_pk = keypair2.public_key();

    // Each user writes their own data
    session1
        .storage()
        .put("/pub/user1.txt", "user1 content")
        .await
        .unwrap();
    session2
        .storage()
        .put("/pub/user2.txt", "user2 content")
        .await
        .unwrap();

    // Backup user1
    let (storage1, _temp_dir1) = create_test_storage();
    run_controller_until_idle(user1_pk.clone(), storage1.clone(), pubky_client.clone()).await;

    // Backup user2
    let (storage2, _temp_dir2) = create_test_storage();
    run_controller_until_idle(user2_pk.clone(), storage2.clone(), pubky_client.clone()).await;

    // Verify each backup only contains their user's data
    let resource1 = PubkyResource::new(user1_pk.clone(), "/pub/user1.txt").unwrap();
    let resource2 = PubkyResource::new(user2_pk.clone(), "/pub/user2.txt").unwrap();

    assert!(
        storage1.as_ref().read(&resource1).await.is_ok(),
        "User1 backup should have user1 data"
    );
    assert!(
        storage2.as_ref().read(&resource2).await.is_ok(),
        "User2 backup should have user2 data"
    );

    // Cross-check: user1's storage shouldn't have user2's resource (different pubky)
    let cross_resource = PubkyResource::new(user2_pk.clone(), "/pub/user2.txt").unwrap();
    assert!(
        storage1.as_ref().read(&cross_resource).await.is_err(),
        "User1 backup should NOT have user2 data"
    );
}

// --- Signing in to back up private data ---

/// A user on the shared testnet whose key is backed up by its own manager.
struct BackedUpUser {
    signer: PubkySigner,
    session: PubkySession,
    pubky: PublicKey,
    manager: BackupManager,
    data_dir: TempDir,
}

impl BackedUpUser {
    /// Sign up a user with one private and one public file, then start
    /// backing up its key and wait for the first sync to finish.
    async fn create() -> Self {
        let testnet = get_shared_testnet().await;
        let pubky_client = testnet.sdk().unwrap();
        let homeserver_pk = testnet.homeserver_app().public_key();

        let keypair = Keypair::random();
        let signer = pubky_client.signer(keypair.clone());
        let session = signup_with_cookie(&signer, &homeserver_pk).await;
        let pubky = keypair.public_key();

        // The private file is written first, so its event is older than the
        // position the public sync reaches
        session
            .storage()
            .put("/priv/secret.txt", "private data")
            .await
            .unwrap();
        session
            .storage()
            .put("/pub/public.txt", "public data")
            .await
            .unwrap();

        let data_dir = TempDir::new().unwrap();
        let manager = start_manager(&data_dir).await;
        manager.add_key(pubky.clone()).await.unwrap();

        let user = Self {
            signer,
            session,
            pubky,
            manager,
            data_dir,
        };
        user.wait_for_state("first sync", |state| state.status == KeyStatus::Idle)
            .await;
        user
    }

    /// Start a sign-in and approve it with the user's own key, as their signer app would.
    async fn sign_in(&self) {
        let request = self.manager.start_sign_in(&self.pubky).await.unwrap();
        self.signer
            .approve_auth(request.authorization_url())
            .await
            .unwrap();
        request.approved().await.unwrap();
        assert_eq!(self.auth_status(), AuthStatus::SignedIn);
    }

    fn auth_status(&self) -> AuthStatus {
        self.manager.get_key_state(&self.pubky).unwrap().auth
    }

    /// Read one of the sync cursors the backup stores for this user, if it exists.
    fn stored_cursor(&self, filename: &str) -> Option<u64> {
        let file = self
            .manager
            .keys_dir()
            .join(self.pubky.z32())
            .join("state")
            .join(filename);
        std::fs::read_to_string(file).ok()?.trim().parse().ok()
    }

    /// The file the backup stores this user's session secret in.
    fn session_secret_file(&self) -> std::path::PathBuf {
        self.manager
            .keys_dir()
            .join(self.pubky.z32())
            .join("state/session")
    }

    /// Wait until the key's state satisfies `predicate`, or panic after 30 seconds.
    async fn wait_for_state(&self, what: &str, predicate: impl Fn(&KeyState) -> bool) {
        wait_until(what, || async {
            self.manager
                .get_key_state(&self.pubky)
                .is_some_and(|state| predicate(&state))
        })
        .await;
    }

    /// Sync now and wait until the backup of `path` holds `expected` (`None` = no file).
    async fn sync_until_backed_up(&self, path: &str, expected: Option<&str>) {
        self.manager.force_sync(&self.pubky).await.unwrap();
        wait_until(&format!("backup of {path}"), || async {
            self.backed_up(path).as_deref() == expected
        })
        .await;
    }

    /// Read the backed-up copy of one of the user's files, if there is one.
    fn backed_up(&self, path: &str) -> Option<String> {
        let file = self
            .manager
            .keys_dir()
            .join(self.pubky.z32())
            .join("data")
            .join(path.trim_start_matches('/'));
        std::fs::read_to_string(file).ok()
    }
}

/// Start a manager that talks to the shared testnet and stores its data in `data_dir`.
async fn start_manager(data_dir: &TempDir) -> BackupManager {
    let testnet = get_shared_testnet().await;
    let config = BackupManagerConfig {
        data_dir: Some(data_dir.path().to_path_buf()),
        http_relay: Some(testnet.http_relay().local_link_url()),
        ..Default::default()
    };
    BackupManager::with_client(config, testnet.sdk().unwrap())
        .await
        .unwrap()
}

/// Poll `condition` until it holds, or panic after 30 seconds.
async fn wait_until<F, Fut>(what: &str, condition: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let poll = async {
        while !condition().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(30), poll)
        .await
        .unwrap_or_else(|_| panic!("Timed out waiting for {what}"));
}

/// Without signing in only public data is backed up; signing in adds private
/// data, including later changes and deletions.
#[tokio::test]
#[serial]
async fn test_sign_in_backs_up_private_data() {
    let user = BackedUpUser::create().await;

    // Signed out: public data only
    assert_eq!(
        user.backed_up("/pub/public.txt").as_deref(),
        Some("public data")
    );
    assert_eq!(user.backed_up("/priv/secret.txt"), None);
    assert_eq!(user.auth_status(), AuthStatus::SignedOut);
    // Only the public sync has made progress
    let public_cursor = user.stored_cursor("cursor");
    assert!(public_cursor.is_some());
    assert_eq!(user.stored_cursor("private_cursor"), None);

    // Signing in backs up the private data that already exists, without being asked to sync.
    // That data is older than the public cursor, so this relies on the private sync
    // keeping its own position rather than continuing from the public one.
    user.sign_in().await;
    wait_until("backup of existing private data", || async {
        user.backed_up("/priv/secret.txt").as_deref() == Some("private data")
    })
    .await;
    wait_until("private cursor to be stored", || async {
        user.stored_cursor("private_cursor").is_some()
    })
    .await;
    assert!(user.stored_cursor("private_cursor") < public_cursor);
    assert_eq!(user.stored_cursor("cursor"), public_cursor);

    // Later private changes and deletions are picked up too
    user.session
        .storage()
        .put("/priv/notes/later.txt", "written later")
        .await
        .unwrap();
    user.sync_until_backed_up("/priv/notes/later.txt", Some("written later"))
        .await;

    user.session
        .storage()
        .delete("/priv/secret.txt")
        .await
        .unwrap();
    user.sync_until_backed_up("/priv/secret.txt", None).await;

    // Public data is unaffected
    assert_eq!(
        user.backed_up("/pub/public.txt").as_deref(),
        Some("public data")
    );

    user.manager.shutdown().await;
}

/// The session is stored, so a restarted manager keeps backing up private data.
#[tokio::test]
#[serial]
async fn test_sign_in_survives_restart() {
    let mut user = BackedUpUser::create().await;
    user.sign_in().await;
    user.manager.shutdown().await;

    user.session
        .storage()
        .put("/priv/after-restart.txt", "still private")
        .await
        .unwrap();

    user.manager = start_manager(&user.data_dir).await;

    assert_eq!(user.auth_status(), AuthStatus::SignedIn);
    user.sync_until_backed_up("/priv/after-restart.txt", Some("still private"))
        .await;

    user.manager.shutdown().await;
}

/// A sign-in approved by another key must not give access to that key's data
/// under the wrong name.
#[tokio::test]
#[serial]
async fn test_sign_in_approved_by_other_key_fails() {
    let user = BackedUpUser::create().await;
    let other = BackedUpUser::create().await;

    let request = user.manager.start_sign_in(&user.pubky).await.unwrap();
    assert_eq!(
        user.auth_status(),
        AuthStatus::AwaitingApproval {
            authorization_url: request.authorization_url().to_string()
        }
    );

    other
        .signer
        .approve_auth(request.authorization_url())
        .await
        .unwrap();

    let error = request.approved().await.unwrap_err();
    assert!(
        error.to_string().contains("different key"),
        "Unexpected error: {error}"
    );
    assert!(matches!(
        user.auth_status(),
        AuthStatus::SignInFailed { .. }
    ));
    user.manager.force_sync(&user.pubky).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(user.backed_up("/priv/secret.txt"), None);

    user.manager.shutdown().await;
    other.manager.shutdown().await;
}

/// A cancelled sign-in can no longer be approved.
#[tokio::test]
#[serial]
async fn test_cancelled_sign_in_cannot_be_approved() {
    let user = BackedUpUser::create().await;
    let request = user.manager.start_sign_in(&user.pubky).await.unwrap();
    let authorization_url = request.authorization_url().to_string();

    user.manager.cancel_sign_in(&user.pubky).await.unwrap();

    assert_eq!(user.auth_status(), AuthStatus::SignedOut);
    assert!(request.approved().await.is_err());

    // A late approval is never picked up, so the signer is left waiting
    let late_approval = user.signer.approve_auth(&authorization_url);
    assert!(tokio::time::timeout(Duration::from_secs(2), late_approval)
        .await
        .is_err());
    assert_eq!(user.auth_status(), AuthStatus::SignedOut);

    user.manager.shutdown().await;
}

/// Signing out stops the backup of private data but keeps what was backed up,
/// and ends the session on the homeserver.
#[tokio::test]
#[serial]
async fn test_sign_out_stops_private_backup() {
    let user = BackedUpUser::create().await;
    user.sign_in().await;
    user.sync_until_backed_up("/priv/secret.txt", Some("private data"))
        .await;
    let session_secret = std::fs::read_to_string(user.session_secret_file()).unwrap();

    user.manager.sign_out(&user.pubky).await.unwrap();

    assert_eq!(user.auth_status(), AuthStatus::SignedOut);
    assert!(!user.session_secret_file().exists());

    // New private data is no longer backed up, but public data still is
    user.session
        .storage()
        .put("/priv/after-sign-out.txt", "not backed up")
        .await
        .unwrap();
    user.session
        .storage()
        .put("/pub/after-sign-out.txt", "backed up")
        .await
        .unwrap();
    user.sync_until_backed_up("/pub/after-sign-out.txt", Some("backed up"))
        .await;
    assert_eq!(user.backed_up("/priv/after-sign-out.txt"), None);
    assert_eq!(
        user.backed_up("/priv/secret.txt").as_deref(),
        Some("private data")
    );

    // The homeserver no longer accepts the session
    wait_until("session to end on the homeserver", || async {
        restore_cookie_session(&session_secret).await.is_err()
    })
    .await;

    user.manager.shutdown().await;
}

/// When the homeserver stops accepting the session, the key falls back to
/// backing up public data only instead of failing.
#[tokio::test]
#[serial]
async fn test_rejected_session_falls_back_to_public_backup() {
    let user = BackedUpUser::create().await;
    user.sign_in().await;
    user.sync_until_backed_up("/priv/secret.txt", Some("private data"))
        .await;

    // End the backup's session behind its back
    let session_secret = std::fs::read_to_string(user.session_secret_file()).unwrap();
    let backup_session = restore_cookie_session(&session_secret).await.unwrap();
    backup_session.signout().await.map_err(|(e, _)| e).unwrap();

    user.session
        .storage()
        .put("/priv/unreadable.txt", "written after rejection")
        .await
        .unwrap();
    user.session
        .storage()
        .put("/pub/readable.txt", "still public")
        .await
        .unwrap();
    user.manager.force_sync(&user.pubky).await.unwrap();

    user.wait_for_state("session expiry", |state| {
        state.auth == AuthStatus::SessionExpired
    })
    .await;
    assert!(!user.session_secret_file().exists());

    // Public data keeps syncing and the key is not in an error state
    user.sync_until_backed_up("/pub/readable.txt", Some("still public"))
        .await;
    assert_eq!(user.backed_up("/priv/unreadable.txt"), None);
    assert!(user
        .manager
        .get_key_state(&user.pubky)
        .unwrap()
        .error
        .is_none());

    // Signing in again resumes the private backup where it left off
    user.sign_in().await;
    user.sync_until_backed_up("/priv/unreadable.txt", Some("written after rejection"))
        .await;

    user.manager.shutdown().await;
}

/// Restore a cookie session from its stored secret, as the backup does.
#[allow(deprecated)]
async fn restore_cookie_session(secret: &str) -> pubky::Result<PubkySession> {
    let client = get_shared_testnet().await.sdk().unwrap().client().clone();
    PubkySession::import_secret(secret, Some(client)).await
}
