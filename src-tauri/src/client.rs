use log::warn;
use reqwest;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct EventInfo {
    pub operation: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<String>,
    pub cursor: String,
}

impl EventsResponse {
    /// Parse events response from API
    pub fn from_response(response: &str) -> Result<Self, String> {
        let lines: Vec<&str> = response.trim().split('\n').collect();

        if lines.is_empty() {
            return Err("Empty response".to_string());
        }

        let mut events = Vec::new();
        let mut cursor = String::new();

        for line in lines {
            let line = line.trim();
            if line.starts_with("PUT pubky://") || line.starts_with("DEL pubky://") {
                events.push(line.to_string());
            } else if line.starts_with("cursor: ") {
                cursor = line.strip_prefix("cursor: ").unwrap_or("").to_string();
            } else {
                warn!("Unhandled event: {}", line);
            }
        }

        Ok(EventsResponse { events, cursor })
    }

    /// Get parsed events as EventInfo structs
    pub fn events(&self) -> Vec<EventInfo> {
        self.events
            .iter()
            .filter_map(|event| Self::parse_event(event))
            .collect()
    }

    fn parse_event(event: &str) -> Option<EventInfo> {
        let event = event.trim();

        if let Some(url) = event.strip_prefix("PUT ") {
            Some(EventInfo {
                operation: "PUT".to_string(),
                url: url.to_string(),
            })
        } else if let Some(url) = event.strip_prefix("DEL ") {
            Some(EventInfo {
                operation: "DEL".to_string(),
                url: url.to_string(),
            })
        } else {
            None
        }
    }
}

// This is required for making `/events/ requests until the SDK adds them 
pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_with_header(
        &self,
        url: &str,
        header_name: &str,
        header_value: &str,
    ) -> Result<String, reqwest::Error> {
        let response = self
            .client
            .get(url)
            .header(header_name, header_value)
            .send()
            .await?;
        let text = response.text().await?;
        Ok(text)
    }
}

/// Fetch event list from given cursor
/// This fetches all events for all pubkys currently
pub async fn fetch_events(cursor: &str) -> Result<EventsResponse, String> {
    let client = HttpClient::new();

    let limit = 10;
    let pubky_url = format!(
        "https://homeserver.staging.pubky.app/events/?limit={}&cursor={}",
        limit, cursor
    );
    let pubky_host = "b3p9kmimbq8irxe8hwwg85qbe34r3i6f3fcqw9jo61wsh13eftio";

    match client
        .get_with_header(&pubky_url, "Pubky-Host", pubky_host)
        .await
    {
        Ok(response) => EventsResponse::from_response(&response),
        Err(e) => Err(format!("HTTP request failed: {}", e)),
    }
}
