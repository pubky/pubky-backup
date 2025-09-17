use reqwest;
use serde_json::Value;

pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn get(&self, url: &str) -> Result<String, reqwest::Error> {
        let response = self.client.get(url).send().await?;
        let text = response.text().await?;
        Ok(text)
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

    pub async fn get_json(&self, url: &str) -> Result<Value, reqwest::Error> {
        let response = self.client.get(url).send().await?;
        let json = response.json::<Value>().await?;
        Ok(json)
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
