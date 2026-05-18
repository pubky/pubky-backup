use log::warn;
use pubky::PublicKey;
use std::str::FromStr;

/// Parse a z32-encoded pubky string into a [`PublicKey`].
///
/// Convenience wrapper around `PublicKey::from_str`.
pub fn parse_pubky(pubky_str: &str) -> Result<PublicKey, pubky::pkarr::errors::PublicKeyError> {
    let stripped = pubky_str.strip_prefix("pubky").unwrap_or(pubky_str);
    PublicKey::from_str(stripped)
}

/// Maximum number of retry attempts for transient network errors
const MAX_RETRIES: u32 = 3;

/// Extract URL from an error message for logging context.
/// Looks for "https://" or "pubky://" URLs in the error string.
fn extract_url_from_error(error: &str) -> &str {
    for prefix in ["https://", "pubky://"] {
        if let Some(start) = error.find(prefix) {
            let url_part = &error[start..];
            return match url_part.find(' ') {
                Some(end) => &url_part[..end],
                None => url_part,
            };
        }
    }
    "unknown URL"
}

/// Check if an error is a transient network error that should be retried.
fn is_retryable_error(error: &str) -> bool {
    let error_lower = error.to_lowercase();
    error.contains("HTTP transport error")
        || error.contains("error sending request")
        || error_lower.contains("connection")
        || error_lower.contains("network")
}

/// Retry utility function for reqwest HTTP operations
pub async fn retry_with_backoff<F, Fut>(operation: F) -> Result<reqwest::Response, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, String>>,
{
    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        match operation().await {
            Ok(response) => {
                // Handle rate limiting (429) - use exponential backoff
                if response.status() == 429 {
                    if attempt < MAX_RETRIES {
                        let sleep_time = 2 * attempt;
                        warn!(
                            "Request to {} rate limited on attempt {}, sleeping for {} seconds",
                            response.url().as_str(),
                            attempt,
                            sleep_time
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(sleep_time.into()))
                            .await;
                        continue;
                    }
                    return Err(format!(
                        "Rate limited after maximum retries for {}",
                        response.url().as_str()
                    ));
                }

                if response.status().is_success() {
                    return Ok(response);
                }
                return Err(format!(
                    "Request to {} failed with status: {}",
                    response.url().as_str(),
                    response.status()
                ));
            }
            Err(e) => {
                if is_retryable_error(&e) && attempt < MAX_RETRIES {
                    let url_info = extract_url_from_error(&e);
                    warn!(
                        "Request to {} failed on attempt {} with transport error: {}, retrying...",
                        url_info, attempt, e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    last_error = Some(e);
                    continue;
                }

                // For other errors, don't retry
                return Err(e);
            }
        }
    }

    // Return the last error if we exhausted retries
    Err(last_error.unwrap_or_else(|| "Unexpected error in retry loop".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url_from_error_https() {
        assert_eq!(
            extract_url_from_error("Failed at https://example.com/path more text"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_extract_url_from_error_https_no_trailing() {
        assert_eq!(
            extract_url_from_error("Error: https://example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_extract_url_from_error_pubky() {
        assert_eq!(
            extract_url_from_error("Error with pubky://abc123/data"),
            "pubky://abc123/data"
        );
    }

    #[test]
    fn test_extract_url_from_error_unknown() {
        assert_eq!(extract_url_from_error("No URL here"), "unknown URL");
    }

    #[test]
    fn test_extract_url_from_error_prefers_https_over_pubky() {
        // https appears first in the search order
        assert_eq!(
            extract_url_from_error("Found https://foo.com then pubky://bar"),
            "https://foo.com"
        );
    }

    #[test]
    fn test_is_retryable_error_http_transport() {
        assert!(is_retryable_error("HTTP transport error: timeout"));
    }

    #[test]
    fn test_is_retryable_error_sending_request() {
        assert!(is_retryable_error("error sending request to server"));
    }

    #[test]
    fn test_is_retryable_error_connection_case_insensitive() {
        assert!(is_retryable_error("Connection refused"));
        assert!(is_retryable_error("connection reset by peer"));
        assert!(is_retryable_error("CONNECTION_FAILED"));
    }

    #[test]
    fn test_is_retryable_error_network_case_insensitive() {
        assert!(is_retryable_error("Network unreachable"));
        assert!(is_retryable_error("network timeout"));
        assert!(is_retryable_error("NETWORK_ERROR"));
    }

    #[test]
    fn test_is_retryable_error_non_retryable() {
        assert!(!is_retryable_error("404 Not Found"));
        assert!(!is_retryable_error("Invalid request body"));
        assert!(!is_retryable_error("Unauthorized"));
    }
}
