//! Homeserver discovery and pubky validation.
//!
//! This module handles the discovery of homeservers for pubky keys and validates
//! that a pubky has data available for backup.
//!
//! # Responsibilities
//!
//! - Homeserver discovery
//! - Validation that a pubky has backup-able data
//! - Timeout handling for network operations

use log::debug;
use pubky::{Pkdns, PubkyResource, PublicKey, PublicStorage};

use super::error::OrchestratorError;

/// Validate a pubky by discovering its homeserver and checking for data.
///
/// In developer mode, validation is skipped and the pubky itself is returned
/// as the "homeserver" placeholder.
///
/// # Arguments
///
/// * `pubky` - The public key to validate
/// * `timeout_secs` - Timeout for network operations
/// * `developer_mode` - Whether to skip validation (for testing)
///
/// # Returns
///
/// The homeserver's public key if validation succeeds.
///
/// # Errors
///
/// Returns `OrchestratorError::ValidationFailed` if:
/// - Homeserver discovery times out or fails
/// - No data is found for the pubky
pub async fn validate_pubky(
    pubky: &PublicKey,
    timeout_secs: u64,
    developer_mode: bool,
) -> Result<PublicKey, OrchestratorError> {
    // In developer mode, skip validation
    if developer_mode {
        debug!("Developer mode: skipping validation for {}", pubky);
        return Ok(pubky.clone()); // Return pubky as homeserver in dev mode
    }

    // Discover homeserver
    let homeserver = discover_homeserver(pubky, timeout_secs).await?;

    // Check that the pubky has data
    verify_pubky_has_data(pubky, timeout_secs).await?;

    Ok(homeserver)
}

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
/// Returns `OrchestratorError::ValidationFailed` if discovery times out or fails.
async fn discover_homeserver(
    pubky: &PublicKey,
    timeout_secs: u64,
) -> Result<PublicKey, OrchestratorError> {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        Pkdns::new()
            .map_err(|e| OrchestratorError::ValidationFailed(e.to_string()))?
            .get_homeserver_of(pubky)
            .await
            .ok_or_else(|| {
                OrchestratorError::ValidationFailed(format!(
                    "Could not discover homeserver for {}",
                    pubky
                ))
            })
    })
    .await
    .map_err(|_| {
        OrchestratorError::ValidationFailed("Homeserver discovery timed out".to_string())
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
async fn verify_pubky_has_data(
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
    use crate::DEV_MODE_PUBKY;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_validate_pubky_developer_mode_skips_validation() {
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // In developer mode, should return the pubky itself as "homeserver"
        let result = validate_pubky(&pubky, 30, true).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), pubky);
    }

    #[tokio::test]
    async fn test_validate_pubky_non_developer_mode_requires_real_homeserver() {
        let pubky = PublicKey::from_str(DEV_MODE_PUBKY).unwrap();

        // In non-developer mode with fake pubky, validation should fail
        // (no real homeserver exists for this test pubky)
        let result = validate_pubky(&pubky, 5, false).await;
        assert!(result.is_err());
    }
}
