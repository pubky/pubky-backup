//! Homeserver discovery.
//!
//! [`discover_homeserver`] resolves a pubky's homeserver via PKDNS. It is the
//! one check a pubky must pass to be backed up: without a homeserver there is
//! nothing to back up from. A pubky does not need to have any data yet.

use pubky::{Pubky, PublicKey};

use super::error::OrchestratorError;

/// Discover the homeserver for a pubky using PKDNS.
///
/// # Arguments
///
/// * `pubky_client` - The Pubky client to resolve with
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
    pubky_client: &Pubky,
    pubky: &PublicKey,
    timeout_secs: u64,
) -> Result<PublicKey, OrchestratorError> {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        pubky_client
            .get_homeserver_of(pubky)
            .await
            .map_err(|e| OrchestratorError::HomeserverNotFound(e.to_string()))?
            .ok_or_else(|| {
                OrchestratorError::HomeserverNotFound("Could not discover homeserver".to_string())
            })
    })
    .await
    .map_err(|_| {
        OrchestratorError::HomeserverNotFound("Homeserver discovery timed out".to_string())
    })?
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
        let pubky_client = Pubky::new().unwrap();
        let result = discover_homeserver(&pubky_client, &pubky, 3).await;
        assert!(result.is_err());
    }
}
