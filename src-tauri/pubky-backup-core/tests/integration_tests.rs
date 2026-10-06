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

use pubky::{
    AuthFlowKind, Capabilities, ClientId, Keypair, Method, PubkyGrantAuthFlow, PubkyResource,
    PubkySession, PubkySigner, PublicKey,
};
use pubky_backup_core::{
    AppStorage, AuthStatus, BackupController, BackupManager, BackupManagerConfig,
    ControllerCommand, ControllerStatus, ControllerUpdate, DataScope, KeyState, KeyStatus,
    MirrorStatus, OrchestratorError,
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
    let storage = AppStorage::new_with_path(temp_dir.path()).unwrap();
    (Arc::new(storage), temp_dir)
}

/// Shared testnet instance across all integration tests.
/// Uses a dedicated tokio runtime to keep the testnet alive across test boundaries.
static SHARED_TESTNET: OnceLock<(tokio::runtime::Runtime, EphemeralTestnet)> = OnceLock::new();

/// A second homeserver on the shared testnet, for keys to be mirrored to.
static MIRROR_HOMESERVER: OnceLock<PublicKey> = OnceLock::new();

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
                let mut testnet = EphemeralTestnet::builder()
                    .postgres(postgres)
                    // Sign-in approvals travel through the relay
                    .with_http_relay()
                    .build()
                    .await
                    .expect("Failed to start testnet with docker postgres");

                let mirror = testnet
                    .create_random_homeserver()
                    .await
                    .expect("Failed to start the mirror homeserver")
                    .public_key();
                let _ = MIRROR_HOMESERVER.set(mirror);
                testnet
            });

            (rt, testnet)
        });
        testnet
    })
    .await
    .expect("Failed to get testnet")
}

/// Helper to create an account on the homeserver and sign in to it with full
/// access, as the user's own apps would, to write the data that gets backed up.
async fn sign_up(signer: &PubkySigner, homeserver_pk: &PublicKey) -> PubkySession {
    signer.signup(homeserver_pk, None).await.unwrap();
    signer
        .signin(ClientId::new("integration-tests").unwrap())
        .await
        .unwrap()
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
                Ok(ControllerUpdate {
                    status: ControllerStatus::Idle,
                    ..
                }) => break,
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
    let session = sign_up(&signer, &homeserver_pk).await;
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
    let cursor1 = storage
        .read_cursor(&user_pk, DataScope::Public)
        .await
        .unwrap();
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
    let cursor2 = storage
        .read_cursor(&user_pk, DataScope::Public)
        .await
        .unwrap();
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
    let session = sign_up(&signer, &homeserver_pk).await;
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

    match status.status {
        ControllerStatus::Starting | ControllerStatus::Syncing { .. } | ControllerStatus::Idle => {}
        other => panic!("Unexpected initial status: {:?}", other),
    }

    // === Part 2: Test ForceSync ===
    control_tx.send(ControllerCommand::ForceSync).await.unwrap();

    // Collect status updates until we see Syncing
    let mut saw_syncing = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(5), status_rx.recv()).await {
            Ok(Ok(ControllerUpdate {
                status: ControllerStatus::Syncing { .. },
                ..
            })) => {
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

    let session1 = sign_up(&signer1, &homeserver_pk).await;
    let session2 = sign_up(&signer2, &homeserver_pk).await;

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
    ///
    /// The user also has an account on the mirror homeserver, as mirroring
    /// expects, with nothing in it.
    async fn create() -> Self {
        // The private file is written first, so its event is older than the
        // position the public sync reaches
        Self::create_with_files(&[
            ("/priv/secret.txt", "private data"),
            ("/pub/public.txt", "public data"),
        ])
        .await
    }

    /// Sign up a user with the given files, written in order, then start
    /// backing up its key and wait for the first sync to finish.
    async fn create_with_files(files: &[(&str, &str)]) -> Self {
        Self::create_user(files, true).await
    }

    /// Like [`Self::create`], for a user that has no account on the mirror.
    async fn create_without_mirror_account() -> Self {
        Self::create_user(&[("/pub/public.txt", "public data")], false).await
    }

    async fn create_user(files: &[(&str, &str)], account_on_mirror: bool) -> Self {
        let testnet = get_shared_testnet().await;
        let homeserver_pk = testnet.homeserver_app().public_key();
        let signer = testnet.sdk().unwrap().signer(Keypair::random());
        // Signing up publishes the homeserver, so the key's own homeserver goes last
        if account_on_mirror {
            signer.signup(mirror_homeserver(), None).await.unwrap();
        }
        let session = sign_up(&signer, &homeserver_pk).await;
        let pubky = signer.public_key();

        for (path, content) in files {
            session
                .storage()
                .put(*path, content.to_string())
                .await
                .unwrap();
        }

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

    /// Set a mirror up and approve it with the user's own key, as their signer app would.
    async fn start_mirror(&self) {
        let request = self
            .manager
            .start_mirror(&self.pubky, mirror_homeserver())
            .await
            .unwrap();
        self.signer
            .approve_auth(request.authorization_url())
            .await
            .unwrap();
        request.approved().await.unwrap();
        assert_eq!(self.mirror_status(), mirroring());
    }

    fn mirror_status(&self) -> MirrorStatus {
        self.manager.get_key_state(&self.pubky).unwrap().mirror
    }

    /// The secret of the user's session on the mirror, as stored by the backup.
    fn stored_mirror_secret(&self) -> Option<String> {
        let file = self.data_dir.path().join("mirrors").join(self.pubky.z32());
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).ok()?).unwrap();
        record["session"].as_str().map(str::to_string)
    }

    /// Sync now and wait until the mirror holds `expected` at `path` (`None` = no file).
    async fn sync_until_mirrored(&self, path: &str, expected: Option<&str>) {
        self.manager.force_sync(&self.pubky).await.unwrap();
        wait_until(&format!("mirror of {path}"), || async {
            self.mirrored(path).await.as_deref() == expected
        })
        .await;
    }

    /// Read one of the user's public files from the mirror, if it has it.
    async fn mirrored(&self, path: &str) -> Option<String> {
        read_from_mirror(&self.pubky, path, None).await
    }

    /// Read one of the user's private files from the mirror, if it has it.
    async fn mirrored_private(&self, path: &str) -> Option<String> {
        let session = mirror_session_for(&self.signer).await;
        let token = session.as_grant().unwrap().current_bearer().await;
        let found = read_from_mirror(&self.pubky, path, Some(&token)).await;
        session.signout().await.map_err(|(e, _)| e).unwrap();
        found
    }

    /// The file the backup stores this user's session secret in.
    fn session_secret_file(&self) -> std::path::PathBuf {
        self.data_dir.path().join("sessions").join(self.pubky.z32())
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

/// Sign up a user on the shared testnet and write the given files, in order.
async fn sign_up_with_files(files: &[(&str, &str)]) -> (PubkySigner, PubkySession) {
    let testnet = get_shared_testnet().await;
    let homeserver_pk = testnet.homeserver_app().public_key();
    let signer = testnet.sdk().unwrap().signer(Keypair::random());
    let session = sign_up(&signer, &homeserver_pk).await;

    for (path, content) in files {
        session
            .storage()
            .put(*path, content.to_string())
            .await
            .unwrap();
    }
    (signer, session)
}

fn mirror_homeserver() -> &'static PublicKey {
    MIRROR_HOMESERVER
        .get()
        .expect("the shared testnet creates the mirror homeserver")
}

fn mirroring() -> MirrorStatus {
    MirrorStatus::Mirroring {
        homeserver: mirror_homeserver().z32(),
    }
}

/// Read a file of `user` from the mirror homeserver, as any client would.
///
/// Private files need the access token of a session on the mirror.
async fn read_from_mirror(user: &PublicKey, path: &str, token: Option<&str>) -> Option<String> {
    let client = get_shared_testnet().await.sdk().unwrap().client().clone();
    let mut url = url::Url::parse(&format!("https://{}/", mirror_homeserver().z32())).unwrap();
    url.set_path(path);
    let mut request = client
        .request(Method::GET, &url)
        .header("pubky-host", user.z32());
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.unwrap();
    response
        .status()
        .is_success()
        .then_some(response.text().await.unwrap())
}

/// A session for reading everything of the signer's key on the mirror,
/// approved by the signer, the way the backup opens its own mirror session:
/// the grant is not tied to a homeserver, so the secret of the session it
/// opens on the key's own homeserver restores a session on the mirror as well.
async fn mirror_session_for(signer: &PubkySigner) -> PubkySession {
    let testnet = get_shared_testnet().await;
    let pubky_client = testnet.sdk().unwrap();
    let caps = Capabilities::builder().read("/").unwrap().finish();
    let flow = PubkyGrantAuthFlow::builder(
        &caps,
        AuthFlowKind::signin(),
        ClientId::new("integration-tests-mirror").unwrap(),
    )
    .client(pubky_client.client().clone())
    .relay(testnet.http_relay().local_link_url())
    .start()
    .unwrap();
    signer
        .approve_auth(flow.authorization_url().as_str())
        .await
        .unwrap();
    let own_session = flow.await_approval().await.unwrap();
    let secret = own_session
        .as_grant()
        .unwrap()
        .export_local_secret()
        .await
        .unwrap();
    own_session.signout().await.map_err(|(e, _)| e).unwrap();

    // The stored grant names the homeserver in its second field
    let mut fields: Vec<&str> = secret.splitn(3, ':').collect();
    let mirror = mirror_homeserver().z32();
    fields[1] = &mirror;
    restore_backup_session(&fields.join(":")).await.unwrap()
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

/// A key does not need public data to be backed up: all its data may be private.
#[tokio::test]
#[serial]
async fn test_key_with_only_private_data_can_be_backed_up() {
    let user = BackedUpUser::create_with_files(&[("/priv/secret.txt", "private data")]).await;

    // The homeserver has no public directory for this user at all
    let public_dir = PubkyResource::new(user.pubky.clone(), "/pub/").unwrap();
    let testnet = get_shared_testnet().await;
    assert!(testnet
        .sdk()
        .unwrap()
        .public_storage()
        .get(public_dir)
        .await
        .is_err());

    user.sign_in().await;
    user.sync_until_backed_up("/priv/secret.txt", Some("private data"))
        .await;

    user.manager.shutdown().await;
}

/// A key with no data yet can be added, and is backed up once it has some.
#[tokio::test]
#[serial]
async fn test_key_without_data_can_be_added() {
    let user = BackedUpUser::create_with_files(&[]).await;
    assert!(user
        .manager
        .get_key_state(&user.pubky)
        .unwrap()
        .error
        .is_none());

    user.session
        .storage()
        .put("/pub/first.txt", "first file")
        .await
        .unwrap();
    user.sync_until_backed_up("/pub/first.txt", Some("first file"))
        .await;

    user.manager.shutdown().await;
}

/// A key that no homeserver knows is refused up front, rather than added and
/// left failing on every sync.
#[tokio::test]
#[serial]
async fn test_key_without_homeserver_is_rejected() {
    let data_dir = TempDir::new().unwrap();
    let manager = start_manager(&data_dir).await;
    let unknown = Keypair::random().public_key();

    let result = manager.add_key(unknown.clone()).await;

    assert!(
        matches!(result, Err(OrchestratorError::HomeserverNotFound(_))),
        "Unexpected result: {result:?}"
    );
    assert!(manager.get_key_state(&unknown).is_none());
    // Nothing was stored, so the key isn't resumed on the next start either
    assert!(manager.list_pubky_directories().unwrap().is_empty());

    manager.shutdown().await;
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

/// A sign-in approved without read access to private data fails, rather than
/// leaving a key that shows as signed in but can't back up its private data.
#[tokio::test]
#[serial]
async fn test_sign_in_without_private_read_access_fails() {
    let user = BackedUpUser::create().await;
    let request = user.manager.start_sign_in(&user.pubky).await.unwrap();

    // Signer apps may approve fewer capabilities than were asked for, here none
    let nothing = Capabilities::builder().finish();
    user.signer
        .approve_auth_with_capabilities(request.authorization_url(), &nothing)
        .await
        .unwrap();

    let error = request.approved().await.unwrap_err();
    assert!(
        error.to_string().contains("not approved"),
        "Unexpected error: {error}"
    );
    assert!(matches!(
        user.auth_status(),
        AuthStatus::SignInFailed { .. }
    ));
    assert!(!user.session_secret_file().exists());

    user.manager.shutdown().await;
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

/// Approving a sign-in for a new key backs up the key that approved it, private
/// data included, without its pubky being entered.
#[tokio::test]
#[serial]
async fn test_sign_in_for_new_key_backs_up_approving_key() {
    let (signer, _session) = sign_up_with_files(&[
        ("/priv/secret.txt", "private data"),
        ("/pub/public.txt", "public data"),
    ])
    .await;
    let pubky = signer.public_key();
    let data_dir = TempDir::new().unwrap();
    let manager = start_manager(&data_dir).await;

    let request = manager.start_sign_in_for_new_key().await.unwrap();
    signer
        .approve_auth(request.authorization_url())
        .await
        .unwrap();

    assert_eq!(request.approved().await.unwrap(), pubky);
    assert_eq!(manager.get_keys(), vec![pubky.clone()]);
    assert_eq!(
        manager.get_key_state(&pubky).unwrap().auth,
        AuthStatus::SignedIn
    );
    let private_copy = manager
        .keys_dir()
        .join(pubky.z32())
        .join("data/priv/secret.txt");
    wait_until("backup of private data", || async {
        std::fs::read_to_string(&private_copy).ok().as_deref() == Some("private data")
    })
    .await;

    manager.shutdown().await;
}

/// A sign-in for a new key approved by a key that is already backed up signs
/// that key in, without adding it twice.
#[tokio::test]
#[serial]
async fn test_sign_in_for_new_key_signs_in_existing_key() {
    let user = BackedUpUser::create().await;

    let request = user.manager.start_sign_in_for_new_key().await.unwrap();
    user.signer
        .approve_auth(request.authorization_url())
        .await
        .unwrap();

    assert_eq!(request.approved().await.unwrap(), user.pubky);
    assert_eq!(user.manager.get_keys(), vec![user.pubky.clone()]);
    assert_eq!(user.auth_status(), AuthStatus::SignedIn);
    user.sync_until_backed_up("/priv/secret.txt", Some("private data"))
        .await;

    user.manager.shutdown().await;
}

/// A cancelled sign-in for a new key adds no key, even if approved late.
#[tokio::test]
#[serial]
async fn test_cancelled_sign_in_for_new_key_adds_no_key() {
    let (signer, _session) = sign_up_with_files(&[]).await;
    let data_dir = TempDir::new().unwrap();
    let manager = start_manager(&data_dir).await;
    let request = manager.start_sign_in_for_new_key().await.unwrap();
    let authorization_url = request.authorization_url().to_string();

    manager.cancel_sign_in_for_new_key();

    assert!(request.approved().await.is_err());
    // A late approval is never picked up, so the signer is left waiting
    let late_approval = signer.approve_auth(&authorization_url);
    assert!(tokio::time::timeout(Duration::from_secs(2), late_approval)
        .await
        .is_err());
    assert!(manager.get_keys().is_empty());

    manager.shutdown().await;
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
        restore_backup_session(&session_secret).await.is_err()
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
    let backup_session = restore_backup_session(&session_secret).await.unwrap();
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

/// A failure while syncing private data, other than a rejected session, leaves
/// the backup of public data and the key's status untouched.
#[tokio::test]
#[serial]
async fn test_private_sync_failure_does_not_fail_public_backup() {
    let user = BackedUpUser::create().await;
    user.sign_in().await;
    user.sync_until_backed_up("/priv/secret.txt", Some("private data"))
        .await;

    // Make the private cursor unreadable, so every private sync fails
    let private_cursor = user
        .manager
        .keys_dir()
        .join(user.pubky.z32())
        .join("state/private_cursor");
    std::fs::remove_file(&private_cursor).unwrap();
    std::fs::create_dir(&private_cursor).unwrap();

    user.session
        .storage()
        .put("/pub/later.txt", "public after failure")
        .await
        .unwrap();
    user.sync_until_backed_up("/pub/later.txt", Some("public after failure"))
        .await;
    user.wait_for_state("an idle sync", |state| state.status == KeyStatus::Idle)
        .await;

    let state = user.manager.get_key_state(&user.pubky).unwrap();
    assert!(state.error.is_none());
    assert_eq!(state.auth, AuthStatus::SignedIn);

    user.manager.shutdown().await;
}

/// Restore a session from its stored secret, as the backup does.
///
/// This takes the session over: the backup's own copy of it stops working.
async fn restore_backup_session(secret: &str) -> pubky::Result<PubkySession> {
    get_shared_testnet()
        .await
        .sdk()
        .unwrap()
        .restore_session(secret)
        .await
}

// --- Mirroring to a second homeserver ---

/// Mirroring copies what is backed up to a second homeserver, including
/// later changes and deletions, without moving the key's identity.
#[tokio::test]
#[serial]
async fn test_mirror_copies_backup_to_second_homeserver() {
    let user = BackedUpUser::create().await;
    assert_eq!(user.mirrored("/pub/public.txt").await, None);

    // Everything backed up so far is copied right away
    user.start_mirror().await;
    wait_until("mirror of existing data", || async {
        user.mirrored("/pub/public.txt").await.as_deref() == Some("public data")
    })
    .await;

    // Later changes follow, in both directions
    user.session
        .storage()
        .put("/pub/public.txt", "changed")
        .await
        .unwrap();
    user.session
        .storage()
        .put("/pub/later.txt", "written later")
        .await
        .unwrap();
    user.sync_until_mirrored("/pub/public.txt", Some("changed"))
        .await;
    user.sync_until_mirrored("/pub/later.txt", Some("written later"))
        .await;

    user.session
        .storage()
        .delete("/pub/later.txt")
        .await
        .unwrap();
    user.sync_until_mirrored("/pub/later.txt", None).await;

    // The mirror has caught up with the backup, and the key still lives where it did
    assert_eq!(
        user.stored_cursor("mirror_cursor"),
        user.stored_cursor("cursor")
    );
    let testnet = get_shared_testnet().await;
    let own_homeserver = testnet
        .sdk()
        .unwrap()
        .get_homeserver_of(&user.pubky)
        .await
        .unwrap();
    assert_eq!(
        own_homeserver.as_ref(),
        Some(&testnet.homeserver_app().public_key())
    );

    user.manager.shutdown().await;
}

/// Private data reaches the mirror once the key is signed in, since that is
/// when it is backed up.
#[tokio::test]
#[serial]
async fn test_mirror_includes_private_data_once_signed_in() {
    let user = BackedUpUser::create().await;
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/public.txt", Some("public data"))
        .await;
    assert_eq!(user.mirrored_private("/priv/secret.txt").await, None);

    user.sign_in().await;

    wait_until("mirror of private data", || async {
        user.mirrored_private("/priv/secret.txt").await.as_deref() == Some("private data")
    })
    .await;

    user.manager.shutdown().await;
}

/// The mirror is stored, so a restarted manager keeps copying to it.
#[tokio::test]
#[serial]
async fn test_mirror_survives_restart() {
    let mut user = BackedUpUser::create().await;
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/public.txt", Some("public data"))
        .await;
    user.manager.shutdown().await;

    user.session
        .storage()
        .put("/pub/after-restart.txt", "still mirrored")
        .await
        .unwrap();

    user.manager = start_manager(&user.data_dir).await;

    assert_eq!(user.mirror_status(), mirroring());
    user.sync_until_mirrored("/pub/after-restart.txt", Some("still mirrored"))
        .await;

    user.manager.shutdown().await;
}

/// Stopping a mirror ends the copying and the session on the mirror, but
/// keeps what was copied. A mirror can be set up again for the existing account.
#[tokio::test]
#[serial]
async fn test_stop_mirror_keeps_copied_data_and_ends_session() {
    let user = BackedUpUser::create().await;
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/public.txt", Some("public data"))
        .await;
    let secret = user.stored_mirror_secret().unwrap();

    user.manager.stop_mirror(&user.pubky).await.unwrap();

    assert_eq!(user.mirror_status(), MirrorStatus::NotMirrored);
    assert_eq!(user.stored_mirror_secret(), None);
    user.session
        .storage()
        .put("/pub/after-stop.txt", "not mirrored")
        .await
        .unwrap();
    user.sync_until_backed_up("/pub/after-stop.txt", Some("not mirrored"))
        .await;
    assert_eq!(user.mirrored("/pub/after-stop.txt").await, None);
    assert_eq!(
        user.mirrored("/pub/public.txt").await.as_deref(),
        Some("public data")
    );
    // The mirror no longer accepts the session
    wait_until("mirror session to end", || async {
        restore_backup_session(&secret).await.is_err()
    })
    .await;

    // Setting the mirror up again copies everything again
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/after-stop.txt", Some("not mirrored"))
        .await;

    user.manager.shutdown().await;
}

/// When the mirror stops accepting the session, copying pauses until the
/// owner approves again, then resumes where it stopped. The backup is unaffected.
#[tokio::test]
#[serial]
async fn test_rejected_mirror_session_pauses_mirroring() {
    let user = BackedUpUser::create().await;
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/public.txt", Some("public data"))
        .await;

    // End the backup's session behind its back
    let secret = user.stored_mirror_secret().unwrap();
    let mirror_session = restore_backup_session(&secret).await.unwrap();
    mirror_session.signout().await.map_err(|(e, _)| e).unwrap();

    user.session
        .storage()
        .put("/pub/while-expired.txt", "written while expired")
        .await
        .unwrap();
    user.manager.force_sync(&user.pubky).await.unwrap();

    user.wait_for_state("mirror session expiry", |state| {
        state.mirror
            == MirrorStatus::SessionExpired {
                homeserver: mirror_homeserver().z32(),
            }
    })
    .await;
    assert_eq!(user.stored_mirror_secret(), None);
    user.sync_until_backed_up("/pub/while-expired.txt", Some("written while expired"))
        .await;
    assert!(user
        .manager
        .get_key_state(&user.pubky)
        .unwrap()
        .error
        .is_none());
    assert_eq!(user.mirrored("/pub/while-expired.txt").await, None);

    // Approving again resumes where it stopped
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/while-expired.txt", Some("written while expired"))
        .await;

    user.manager.shutdown().await;
}

/// A mirror that becomes the key's own homeserver is not copied to any more:
/// every copy would be a new change to copy, without end.
#[tokio::test]
#[serial]
async fn test_mirroring_stops_when_mirror_becomes_own_homeserver() {
    let mut user = BackedUpUser::create().await;
    user.start_mirror().await;
    user.sync_until_mirrored("/pub/public.txt", Some("public data"))
        .await;

    // The owner moves to the mirror, and the backup is restarted so that it
    // resolves the key's homeserver afresh rather than from its cache
    user.manager.shutdown().await;
    user.signer
        .pkdns()
        .publish_homeserver_force(Some(mirror_homeserver()))
        .await
        .unwrap();
    user.manager = start_manager(&user.data_dir).await;
    user.manager.force_sync(&user.pubky).await.unwrap();

    user.wait_for_state("mirroring to stop", |state| {
        matches!(&state.mirror, MirrorStatus::Failing { message, .. } if message.contains("own homeserver"))
    })
    .await;

    user.manager.shutdown().await;
}

/// A key without an account on the mirror cannot be mirrored there.
#[tokio::test]
#[serial]
async fn test_mirror_needs_an_account_on_the_mirror() {
    let user = BackedUpUser::create_without_mirror_account().await;

    let request = user
        .manager
        .start_mirror(&user.pubky, mirror_homeserver())
        .await
        .unwrap();
    user.signer
        .approve_auth(request.authorization_url())
        .await
        .unwrap();

    let error = request.approved().await.unwrap_err();
    assert!(
        error.to_string().contains("no account"),
        "Unexpected error: {error}"
    );
    assert!(matches!(
        user.mirror_status(),
        MirrorStatus::SetupFailed { message, .. } if message.contains("no account")
    ));

    user.manager.shutdown().await;
}

/// A mirror request approved by another key sets nothing up.
#[tokio::test]
#[serial]
async fn test_mirror_approved_by_other_key_fails() {
    let user = BackedUpUser::create().await;
    let other = BackedUpUser::create().await;

    let request = user
        .manager
        .start_mirror(&user.pubky, mirror_homeserver())
        .await
        .unwrap();
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
        user.mirror_status(),
        MirrorStatus::SetupFailed { .. }
    ));
    assert_eq!(user.stored_mirror_secret(), None);

    user.manager.shutdown().await;
    other.manager.shutdown().await;
}

/// A key cannot be mirrored to the homeserver it already lives on.
#[tokio::test]
#[serial]
async fn test_key_cannot_be_mirrored_to_own_homeserver() {
    let user = BackedUpUser::create().await;
    let own_homeserver = get_shared_testnet().await.homeserver_app().public_key();

    let result = user
        .manager
        .start_mirror(&user.pubky, &own_homeserver)
        .await;

    assert!(
        matches!(result, Err(OrchestratorError::MirrorFailed(_))),
        "Unexpected result: {result:?}"
    );
    assert_eq!(user.mirror_status(), MirrorStatus::NotMirrored);

    user.manager.shutdown().await;
}

/// A cancelled mirror request can no longer be approved.
#[tokio::test]
#[serial]
async fn test_cancelled_mirror_request_cannot_be_approved() {
    let user = BackedUpUser::create().await;
    let request = user
        .manager
        .start_mirror(&user.pubky, mirror_homeserver())
        .await
        .unwrap();

    user.manager.cancel_mirror(&user.pubky).await.unwrap();

    assert_eq!(user.mirror_status(), MirrorStatus::NotMirrored);
    assert!(request.approved().await.is_err());

    user.manager.shutdown().await;
}
