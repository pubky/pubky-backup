//! Resource fetching for backup synchronization.
//!
//! This module handles fetching resource data from Pubky homeservers during
//! the backup process. It includes retry logic, error handling, and support
//! for developer mode (offline/no-network).
//!
//! # Responsibilities
//!
//! - Fetching resource data from homeservers
//! - Retry logic with exponential backoff
//! - Handling 404 responses gracefully
//! - Returning empty data in developer mode (no network calls)

use std::sync::atomic::{AtomicBool, Ordering};

use log::{debug, info};
use pubky::{Pubky, PubkyResource};

use super::error::SyncError;
use super::scope::SyncScope;
use super::session::is_session_rejection;
use crate::is_developer_mode;
use crate::utils::retry_with_backoff;

/// Fetch data from a PubkyResource URL.
///
/// In developer mode, returns empty data instead of making real network calls.
/// Handles 404 responses by returning empty data rather than an error.
///
/// # Arguments
///
/// * `pubky_client` - The Pubky client for making requests
/// * `scope` - The scope the resource belongs to, which decides how it is read
/// * `resource` - The resource to fetch
///
/// # Returns
///
/// The resource data as bytes. Returns an empty vector for 404 responses
/// or in developer mode.
///
/// # Errors
///
/// Returns `SyncError::SessionRejected` if the homeserver does not accept the
/// session of a private scope.
/// Returns `SyncError::Internal` if the fetch fails for other non-404 reasons.
pub(super) async fn fetch_resource_data(
    pubky_client: &Pubky,
    scope: SyncScope<'_>,
    resource: &PubkyResource,
) -> Result<Vec<u8>, SyncError> {
    if is_developer_mode() {
        return Ok(Vec::new());
    }

    let public_storage = pubky_client.public_storage();
    // The retry helper only carries error messages, so note a rejection on the side
    let session_rejected = AtomicBool::new(false);
    let response = match retry_with_backoff(|| async {
        let result = match scope {
            SyncScope::Public => public_storage.get(resource).await,
            SyncScope::Private(session) => session.storage().get(&resource.path).await,
        };
        result.map_err(|e| {
            if matches!(scope, SyncScope::Private(_)) && is_session_rejection(&e) {
                session_rejected.store(true, Ordering::Relaxed);
            }
            format!("{}", e)
        })
    })
    .await
    {
        Ok(response) => response,
        Err(_) if session_rejected.load(Ordering::Relaxed) => {
            return Err(SyncError::SessionRejected);
        }
        Err(e) => {
            // Handle 404s gracefully - resource was deleted between event and fetch
            if e.contains("404") || e.to_lowercase().contains("not found") {
                info!("404 response: Returning empty data for {}", resource);
                return Ok(Vec::new());
            }
            return Err(SyncError::Internal(format!(
                "Failed to fetch data for {}: {}",
                resource, e
            )));
        }
    };

    let data = response.bytes().await.map_err(|e| {
        SyncError::Internal(format!(
            "Failed to read response bytes for {}: {}",
            resource, e
        ))
    })?;

    let data_vec = data.to_vec();
    debug!("Successfully fetched data: {} bytes", data_vec.len());
    Ok(data_vec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_PUBKY;
    use pubky::PublicKey;
    use std::str::FromStr;

    fn enable_developer_mode() {
        std::env::set_var("PUBKY_DEVELOPER_MODE", "1");
    }

    #[tokio::test]
    async fn test_fetch_resource_data_developer_mode_returns_empty() {
        enable_developer_mode();

        let pubky_client = Pubky::testnet().expect("Failed to create testnet client");
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let resource = PubkyResource::new(pubky, "/pub/profile.json").unwrap();

        let data = fetch_resource_data(&pubky_client, SyncScope::Public, &resource)
            .await
            .unwrap();

        // Developer mode returns empty data (no network calls)
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_resource_data_developer_mode_posts_returns_empty() {
        enable_developer_mode();

        let pubky_client = Pubky::testnet().expect("Failed to create testnet client");
        let pubky = PublicKey::from_str(TEST_PUBKY).unwrap();
        let resource = PubkyResource::new(pubky, "/pub/posts/123").unwrap();

        let data = fetch_resource_data(&pubky_client, SyncScope::Public, &resource)
            .await
            .unwrap();

        // Developer mode returns empty data (no network calls)
        assert!(data.is_empty());
    }
}
