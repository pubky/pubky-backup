//! Homeserver discovery and pubky validation.
//!
//! This module provides two independent operations:
//!
//! - [`discover_homeserver`] - Resolves a pubky's homeserver via PKDNS
//! - [`verify_pubky_has_data`] - Checks that a pubky has backup-able data at `/pub/`

use pubky::{Pkdns, PubkyResource, PublicKey, PublicStorage};

use super::error::OrchestratorError;

/// Discover the homeserver for a pubky using PKDNS.
///
/// # Arguments
///
/// * `pubky` - The public key to discover the homeserver for
/// * `timeout_secs` - Timeout for the discovery operation
///
/// # Returns
///
/// The homeserver's public key.
///
/// # Errors
///
/// Returns `OrchestratorError::HomeserverNotFound` if discovery times out or fails.
pub async fn discover_homeserver(
    pubky: &PublicKey,
    timeout_secs: u64,
) -> Result<PublicKey, OrchestratorError> {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        Pkdns::new()
            .map_err(|e| OrchestratorError::HomeserverNotFound(e.to_string()))?
            .get_homeserver_of(pubky)
            .await
            .ok_or_else(|| {
                OrchestratorError::HomeserverNotFound("Could not discover homeserver".to_string())
            })
    })
    .await
    .map_err(|_| {
        OrchestratorError::HomeserverNotFound("Homeserver discovery timed out".to_string())
    })?
}

/// Verify that a pubky has data available for backup.
///
/// Checks if the pubky's `/pub/` directory exists and is accessible.
///
/// # Arguments
///
/// * `pubky` - The public key to check
/// * `timeout_secs` - Timeout for the check operation
///
/// # Errors
///
/// Returns `OrchestratorError::ValidationFailed` if:
/// - The check times out
/// - No data is found at the pubky's `/pub/` path
pub async fn verify_pubky_has_data(
    pubky: &PublicKey,
    timeout_secs: u64,
) -> Result<(), OrchestratorError> {
    let pubky_storage = PublicStorage::new()
        .map_err(|e| OrchestratorError::ValidationFailed(format!("Storage error: {}", e)))?;

    let path = PubkyResource::new(pubky.clone(), "/pub/").map_err(|e| {
        OrchestratorError::ValidationFailed(format!("Invalid resource path: {}", e))
    })?;

    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        pubky_storage.get(path),
    )
    .await
    .map_err(|_| OrchestratorError::ValidationFailed("Data check timed out".to_string()))?
    .map_err(|e| OrchestratorError::ValidationFailed(format!("No data found for pubky: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_discover_homeserver_timeout() {
        // A valid z-base-32 key that has no homeserver registered
        let pubky =
            PublicKey::from_str("yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy").unwrap();
        let result = discover_homeserver(&pubky, 3).await;
        assert!(result.is_err());
    }
}
