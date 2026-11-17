//! Low-level Telegram Bot API client using outbound HTTPS only (getUpdates long-polling).
//! Never opens an inbound port, never requires a webhook or public URL.

use crate::types::RemoteError;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::{debug, error};

const DEFAULT_TELEGRAM_BASE_URL: &str = "https://api.telegram.org";

pub struct TelegramClient {
    client: Client,
    bot_token: String,
    base_url: String,
}

impl TelegramClient {
    pub fn new(bot_token: String) -> Self {
        Self::with_base_url(bot_token, DEFAULT_TELEGRAM_BASE_URL.to_string())
    }

    pub fn with_base_url(bot_token: String, base_url: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45)) // Long-polling timeout margin
            .build()
            .unwrap_or_default();

        Self {
            client,
            bot_token,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn bot_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.bot_token, method)
    }

    /// Fetches updates using long polling via getUpdates.
    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, RemoteError> {
        let mut query = vec![
            ("timeout", timeout_secs.to_string()),
            ("allowed_updates", "[\"message\"]".to_string()),
        ];
        if let Some(off) = offset {
            query.push(("offset", off.to_string()));
        }

        let url = self.bot_url("getUpdates");
        debug!(
            "Polling Telegram getUpdates (offset: {:?}, timeout: {}s)",
            offset, timeout_secs
        );

        let res = self
            .client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| RemoteError::Network(e.to_string()))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            error!(
                "Telegram getUpdates failed with status {}: [REDACTED_BODY]",
                status
            );
            return Err(RemoteError::Api(format!("Status {}: {}", status, body)));
        }

        let json_val = res
            .json::<serde_json::Value>()
            .await
            .map_err(|e| RemoteError::Api(e.to_string()))?;

        Ok(json_val)
    }

    /// Sends a text message to a chat id.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<serde_json::Value, RemoteError> {
        let url = self.bot_url("sendMessage");
        let body = json!({
            "chat_id": chat_id,
            "text": text,
        });

        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RemoteError::Network(e.to_string()))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            error!(
                "Telegram sendMessage failed with status {}: [REDACTED_BODY]",
                status
            );
            return Err(RemoteError::Api(format!("Status {}: {}", status, body)));
        }

        let json_val = res
            .json::<serde_json::Value>()
            .await
            .map_err(|e| RemoteError::Api(e.to_string()))?;

        Ok(json_val)
    }

    /// Sends a binary document/file attachment to a chat id.
    pub async fn send_document(
        &self,
        chat_id: i64,
        filename: &str,
        data: &[u8],
        caption: Option<&str>,
    ) -> Result<serde_json::Value, RemoteError> {
        let url = self.bot_url("sendDocument");

        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| RemoteError::Api(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", part);

        if let Some(cap) = caption {
            form = form.text("caption", cap.to_string());
        }

        let res = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| RemoteError::Network(e.to_string()))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            error!(
                "Telegram sendDocument failed with status {}: [REDACTED_BODY]",
                status
            );
            return Err(RemoteError::Api(format!("Status {}: {}", status, body)));
        }

        let json_val = res
            .json::<serde_json::Value>()
            .await
            .map_err(|e| RemoteError::Api(e.to_string()))?;

        Ok(json_val)
    }
}
