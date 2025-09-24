use anyhow::anyhow;
use log::warn;

/// Retry utility function for reqwest HTTP operations
pub async fn retry_with_backoff<F, Fut>(operation: F) -> Result<reqwest::Response, anyhow::Error>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, anyhow::Error>>,
{
    let mut last_error = None;
    const MAX_RETRIES: u32 = 3;

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
                    } else {
                        return Err(anyhow!(
                            "Rate limited after maximum retries for {}",
                            response.url().as_str()
                        ));
                    }
                }

                return Ok(response);
            }
            Err(e) => {
                let error_msg = format!("{}", e);

                // Extract URL from error message for better logging
                let url_info = if let Some(start) = error_msg.find("https://") {
                    let url_part = &error_msg[start..];
                    if let Some(end) = url_part.find(' ') {
                        &url_part[..end]
                    } else {
                        url_part
                    }
                } else if let Some(start) = error_msg.find("pubky://") {
                    let url_part = &error_msg[start..];
                    if let Some(end) = url_part.find(' ') {
                        &url_part[..end]
                    } else {
                        url_part
                    }
                } else {
                    "unknown URL"
                };

                // Handle HTTP transport errors - retry with standard delay
                if error_msg.contains("HTTP transport error")
                    || error_msg.contains("error sending request")
                    || error_msg.to_lowercase().contains("connection")
                    || error_msg.to_lowercase().contains("network")
                {
                    if attempt < MAX_RETRIES {
                        warn!(
                            "Request to {} failed on attempt {} with transport error: {}, retrying...",
                            url_info, attempt, e
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        last_error = Some(e);
                        continue;
                    }
                }

                // For other errors, don't retry
                return Err(e);
            }
        }
    }

    // Return the last error if we exhausted retries
    Err(last_error.unwrap_or_else(|| anyhow!("Unexpected error in retry loop")))
}
