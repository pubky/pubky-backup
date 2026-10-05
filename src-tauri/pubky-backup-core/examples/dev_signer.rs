//! Stand-in for a signer app (e.g. Pubky Ring) during development.
//!
//! Lets you try out signing in to a backup without a phone:
//!
//! ```bash
//! # Create a throwaway user with some public and private data
//! cargo run -p pubky-backup-core --example dev_signer -- signup <homeserver> user.key
//!
//! # Approve the sign-in link that the backup shows for that user
//! cargo run -p pubky-backup-core --example dev_signer -- approve 'pubkyauth://...' user.key
//! ```
//!
//! Set `PUBKY_TESTNET=1` to act on a local testnet (see the `dev_testnet`
//! example) instead of mainnet.
//!
//! The key file holds the user's secret key unencrypted, so only use it for
//! throwaway test users.
#![allow(deprecated)] // Cookie auth is deprecated by the SDK, but it is what we use

use std::path::Path;

use pubky::{Keypair, Pubky, PublicKey};
use pubky_backup_core::is_testnet_mode;

const USAGE: &str = "Usage:
  dev_signer signup <homeserver> <key-file>
  dev_signer approve <pubkyauth-url> <key-file>";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        ["signup", homeserver, key_file] => signup(homeserver, Path::new(key_file)).await,
        ["approve", url, key_file] => approve(url, Path::new(key_file)).await,
        _ => Err(USAGE.into()),
    }
}

/// A client for mainnet, or for a local testnet in testnet mode.
fn pubky_client() -> pubky::Result<Pubky> {
    if is_testnet_mode() {
        Pubky::testnet()
    } else {
        Pubky::new()
    }
}

/// Create a user on `homeserver`, give it some data to back up and save its key.
async fn signup(homeserver: &str, key_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let homeserver: PublicKey = homeserver.parse()?;
    let keypair = Keypair::random();
    keypair.write_secret_key_file(key_file)?;

    let session = pubky_client()?
        .signer(keypair.clone())
        .signup_cookie(&homeserver, None)
        .await?;
    let storage = session.storage();
    storage
        .put("/pub/backup-test/hello.txt", "public data")
        .await?;
    storage
        .put("/priv/backup-test/secret.txt", "private data")
        .await?;

    println!("{}", keypair.public_key().z32());
    Ok(())
}

/// Approve a sign-in request with the key in `key_file`, as its owner would.
async fn approve(url: &str, key_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let keypair = Keypair::from_secret_key_file(key_file)?;
    pubky_client()?.signer(keypair).approve_auth(url).await?;

    println!("Approved");
    Ok(())
}
