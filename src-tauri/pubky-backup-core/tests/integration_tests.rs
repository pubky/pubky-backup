//! Integration tests using real pubky-testnet infrastructure.
//!
//! These tests spin up an ephemeral testnet with embedded PostgreSQL and a real
//! homeserver, testing the backup controller against actual network operations.
//!
//! # Running the tests
//!
//! ```bash
//! cargo test --test integration_tests
//! ```
//!
//! # Note
//!
//! The first run will download PostgreSQL binaries (~50-100MB), which are cached
//! for subsequent runs.

use futures_util::StreamExt;
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

#[tokio::test]
async fn test_backup_controller_syncs_real_data() {
    let testnet = create_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    // Create a user for this test
    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

    // Write test data
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

    // Create backup controller
    let (storage, _temp_dir) = create_test_storage();
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );

    // Run sync batch - may need multiple batches to get all events
    loop {
        let result = controller.perform_sync_batch().await;
        assert!(result.is_ok(), "Sync should succeed: {:?}", result.err());

        match result.unwrap() {
            std::ops::ControlFlow::Continue(_) => continue,
            std::ops::ControlFlow::Break(()) => break,
        }
    }

    // Verify data was backed up
    let resource = PubkyResource::new(user_pk.clone(), "/pub/test/file.txt").unwrap();
    let backed_up_data = storage.as_ref().read(&resource).await.unwrap();
    assert_eq!(backed_up_data, b"hello world");

    let resource = PubkyResource::new(user_pk.clone(), "/pub/posts/001.json").unwrap();
    let backed_up_data = storage.as_ref().read(&resource).await.unwrap();
    assert!(String::from_utf8_lossy(&backed_up_data).contains("First post"));
}

#[tokio::test]
async fn test_backup_controller_handles_deletes() {
    let testnet = create_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    // Create user and write initial data
    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

    session
        .storage()
        .put("/pub/to_delete.txt", "this will be deleted")
        .await
        .unwrap();
    session
        .storage()
        .put("/pub/to_keep.txt", "this stays")
        .await
        .unwrap();

    // Initial sync
    let (storage, _temp_dir) = create_test_storage();
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    let _ = controller.perform_sync_batch().await.unwrap();

    // Verify both files were backed up
    let resource_delete = PubkyResource::new(user_pk.clone(), "/pub/to_delete.txt").unwrap();
    let resource_keep = PubkyResource::new(user_pk.clone(), "/pub/to_keep.txt").unwrap();
    assert!(storage.as_ref().read(&resource_delete).await.is_ok());
    assert!(storage.as_ref().read(&resource_keep).await.is_ok());

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
    let _ = controller.perform_sync_batch().await.unwrap();

    // Verify: deleted file should be gone, kept file should remain
    assert!(
        storage.as_ref().read(&resource_delete).await.is_err(),
        "Deleted file should be removed from backup"
    );
    assert!(
        storage.as_ref().read(&resource_keep).await.is_ok(),
        "Kept file should still exist"
    );
}

#[tokio::test]
async fn test_backup_controller_cursor_persistence() {
    let testnet = create_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

    // Write initial data
    session
        .storage()
        .put("/pub/file1.txt", "content1")
        .await
        .unwrap();

    let (storage, _temp_dir) = create_test_storage();

    // First sync
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    let _ = controller.perform_sync_batch().await.unwrap();

    // Check cursor was saved
    let cursor1 = storage.read_cursor(&user_pk).await.unwrap();
    assert!(cursor1.is_some(), "Cursor should be saved after first sync");

    // Write more data
    session
        .storage()
        .put("/pub/file2.txt", "content2")
        .await
        .unwrap();

    // Second sync
    let controller = BackupController::new(
        user_pk.clone(),
        storage.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    let _ = controller.perform_sync_batch().await.unwrap();

    // Cursor should have advanced
    let cursor2 = storage.read_cursor(&user_pk).await.unwrap();
    assert!(cursor2.is_some());
    assert!(
        cursor2 > cursor1,
        "Cursor should advance after processing new events"
    );

    // Verify both files exist
    let resource1 = PubkyResource::new(user_pk.clone(), "/pub/file1.txt").unwrap();
    let resource2 = PubkyResource::new(user_pk.clone(), "/pub/file2.txt").unwrap();
    assert!(storage.as_ref().read(&resource1).await.is_ok());
    assert!(storage.as_ref().read(&resource2).await.is_ok());
}

#[tokio::test]
async fn test_backup_controller_run_loop_with_cancel() {
    let testnet = create_testnet().await;
    let pubky_client = Arc::new(testnet.sdk().unwrap());
    let homeserver_pk = testnet.homeserver_app().public_key();

    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let _session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

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

    // Wait for first status update
    let status = tokio::time::timeout(Duration::from_secs(10), status_rx.recv())
        .await
        .expect("Should receive status within timeout")
        .unwrap();

    match status {
        BackupControllerStatus::Syncing { .. } | BackupControllerStatus::Idle => {}
        other => panic!("Unexpected initial status: {:?}", other),
    }

    // Send cancel message
    control_tx.send(BackupControllerMessage::Cancel).unwrap();

    // Controller should finish
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("Controller should finish after cancel")
        .unwrap();
}

#[tokio::test]
async fn test_backup_controller_force_sync() {
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
        user_pk.clone(),
        storage.clone(),
        pubky_client,
        Some(control_rx),
        Some(status_tx),
    );

    tokio::spawn(controller.run());

    // Trigger force sync
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

    // Cleanup
    control_tx.send(BackupControllerMessage::Cancel).unwrap();
}

#[tokio::test]
async fn test_event_stream_receives_real_events() {
    let testnet = create_testnet().await;
    let pubky_client = testnet.sdk().unwrap();
    let homeserver_pk = testnet.homeserver_app().public_key();

    let keypair = Keypair::random();
    let signer = pubky_client.signer(keypair.clone());
    let session = signer.signup(&homeserver_pk.into(), None).await.unwrap();
    let user_pk: PublicKey = keypair.public_key().into();

    // Write some data first
    session
        .storage()
        .put("/pub/event_test.txt", "event data")
        .await
        .unwrap();

    // Create event stream
    let mut stream = pubky_client
        .event_stream()
        .add_user(&user_pk, None)
        .unwrap()
        .limit(10)
        .subscribe()
        .await
        .unwrap();

    // Should receive at least one event (the PUT we just did, plus signup events)
    let event = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("Should receive event within timeout");

    assert!(event.is_some(), "Should receive at least one event");
    let event = event.unwrap().unwrap();
    assert_eq!(event.resource.owner, user_pk);
}

#[tokio::test]
async fn test_multiple_users_on_same_homeserver() {
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
    let _ = controller1.perform_sync_batch().await.unwrap();

    // Backup user2
    let (storage2, _temp_dir2) = create_test_storage();
    let controller2 = BackupController::new(
        user2_pk.clone(),
        storage2.clone(),
        pubky_client.clone(),
        None,
        None,
    );
    let _ = controller2.perform_sync_batch().await.unwrap();

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
