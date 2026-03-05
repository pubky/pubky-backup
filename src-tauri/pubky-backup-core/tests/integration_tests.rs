//! Integration tests using real pubky-testnet infrastructure.
//!
//! These tests spin up an ephemeral testnet with embedded PostgreSQL and a real
//! homeserver, testing the backup controller against actual network operations.
//!
//! # Running the tests
//!
//! ```bash
//! cargo test -p pubky-backup-core --test integration_tests
//! ```
//!
//! A single testnet instance is shared across all tests to minimize startup overhead.
//! Tests are serialized using `serial_test` to ensure proper sequencing.

use pubky::{Keypair, PubkyResource, PublicKey};
use pubky_backup_core::{AppStorage, BackupController, ControllerCommand, ControllerStatus};
use pubky_testnet::EphemeralTestnet;
use serial_test::serial;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};

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
                EphemeralTestnet::builder()
                    .with_embedded_postgres()
                    .build()
                    .await
                    .expect("Failed to start testnet with embedded postgres")
            });

            (rt, testnet)
        });
        testnet
    })
    .await
    .expect("Failed to get testnet")
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
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

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
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

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

    let session1 = signer1
        .signup(&homeserver_pk.clone().into(), None)
        .await
        .unwrap();
    let session2 = signer2.signup(&homeserver_pk.into(), None).await.unwrap();

    let user1_pk: PublicKey = keypair1.public_key().into();
    let user2_pk: PublicKey = keypair2.public_key().into();

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
