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
//! # Note
//!
//! The first run will download PostgreSQL binaries (~50-100MB), which are cached
//! for subsequent runs.
//!
//! Tests are combined where possible to minimize testnet startup overhead.

use pubky::{Keypair, PubkyResource, PublicKey};
use pubky_backup_core::{
    AppStorage, BackupController, BackupControllerMessage, BackupControllerStatus,
};
use pubky_testnet::EphemeralTestnet;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;

/// Helper to create a storage instance with a temporary directory
fn create_test_storage() -> (Arc<AppStorage>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let storage = AppStorage::new_with_path(&temp_dir.path().to_path_buf()).unwrap();
    (Arc::new(storage), temp_dir)
}

/// Helper to create a testnet with embedded postgres
async fn create_testnet() -> EphemeralTestnet {
    EphemeralTestnet::builder()
        .with_embedded_postgres()
        .build()
        .await
        .expect("Failed to start testnet with embedded postgres")
}

/// Helper to run sync batches until completion (with 30 second timeout)
async fn sync_until_complete(controller: &BackupController) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match controller.perform_sync_batch().await.unwrap() {
                std::ops::ControlFlow::Continue(_) => continue,
                std::ops::ControlFlow::Break(()) => break,
            }
        }
    })
    .await
    .expect("Sync should complete within 30 seconds")
}

/// Tests core backup functionality: PUT sync, DELETE sync, cursor persistence, and event stream.
///
/// This test combines several related scenarios to minimize testnet overhead:
/// 1. Event stream receives real events from homeserver
/// 2. Syncing PUT events backs up data correctly
/// 3. Syncing DELETE events removes backed up data
/// 4. Cursor persists and advances across sync operations
#[tokio::test]
async fn test_backup_sync_put_delete_and_cursor() {
    let testnet = create_testnet().await;
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

    // === Part 2: Test event stream receives real events ===
    {
        use futures_util::StreamExt;

        let mut stream = pubky_client
            .event_stream()
            .add_user(&user_pk, None)
            .unwrap()
            .limit(10)
            .subscribe()
            .await
            .unwrap();

        // Should receive at least one event (the PUTs we just did)
        let event = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("Should receive event within timeout");

        assert!(event.is_some(), "Should receive at least one event");
        let event = event.unwrap().unwrap();
        assert_eq!(event.resource.owner, user_pk);
    }

    // === Part 3: Test PUT sync ===
    // Create backup controller and sync
    let (storage, _temp_dir) = create_test_storage();
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    sync_until_complete(&controller).await;

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

    // === Part 4: Test cursor persistence ===
    let cursor1 = storage.read_cursor(&user_pk).await.unwrap();
    assert!(cursor1.is_some(), "Cursor should be saved after first sync");

    // Write more data
    session
        .storage()
        .put("/pub/file2.txt", "content2")
        .await
        .unwrap();

    // Sync again
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    sync_until_complete(&controller).await;

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

    // === Part 5: Test DELETE sync ===
    // Delete one file on homeserver
    session
        .storage()
        .delete("/pub/to_delete.txt")
        .await
        .unwrap();

    // Sync again
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    sync_until_complete(&controller).await;

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
async fn test_backup_controller_run_loop() {
    let testnet = create_testnet().await;
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
    let (control_tx, control_rx) = broadcast::channel(5);
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
        BackupControllerStatus::Syncing { .. } | BackupControllerStatus::Idle => {}
        other => panic!("Unexpected initial status: {:?}", other),
    }

    // === Part 2: Test ForceSync ===
    control_tx.send(BackupControllerMessage::ForceSync).unwrap();

    // Collect status updates until we see Syncing
    let mut saw_syncing = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(5), status_rx.recv()).await {
            Ok(Ok(BackupControllerStatus::Syncing { .. })) => {
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
    control_tx.send(BackupControllerMessage::Cancel).unwrap();

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
async fn test_multiple_users_isolation() {
    let testnet = create_testnet().await;
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
    let controller1 = BackupController::new(
        user1_pk.clone(),
        storage1.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    sync_until_complete(&controller1).await;

    // Backup user2
    let (storage2, _temp_dir2) = create_test_storage();
    let controller2 = BackupController::new(
        user2_pk.clone(),
        storage2.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    sync_until_complete(&controller2).await;

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
