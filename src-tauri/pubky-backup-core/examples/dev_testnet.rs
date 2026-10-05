//! Local Pubky network for development: a DHT, a homeserver and the relays, all
//! on this machine with fixed ports.
//!
//! Lets you run backups, including signing in, without touching mainnet:
//!
//! ```bash
//! # 1. Start the network (needs Docker, for the homeserver's Postgres)
//! cargo run -p pubky-backup-core --example dev_testnet
//!
//! # 2. Create a user on it and approve sign-ins as that user
//! PUBKY_TESTNET=1 cargo run -p pubky-backup-core --example dev_signer -- signup <homeserver> user.key
//!
//! # 3. Point the backup at it
//! PUBKY_TESTNET=1 cargo tauri dev
//! ```
//!
//! All state is lost when the network stops.

use pubky_testnet::{docker_postgres::DockerPostgres, StaticTestnet};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // The static testnet takes its database from the environment
    let postgres = DockerPostgres::shared().await.connection_string()?;
    std::env::set_var("TEST_PUBKY_CONNECTION_STRING", postgres.as_str());

    let testnet = StaticTestnet::start().await?;

    println!(
        "Testnet running. Homeserver: {}",
        testnet.homeserver_app().public_key().z32()
    );
    println!("HTTP relay: {}", testnet.http_relay().local_link_url());
    println!("Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    Ok(())
}
