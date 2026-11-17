//! Milestone M2: Multi-turn Chat Client with Emotion Tagging and Gemini Integration.
//! Implements primary-to-fallback routing (gemini-3.8-flash -> gemini-3-flash-preview),
//! 60-second primary unhealthy caching, text-length-scaled timeouts,
//! optional third-party FreeLLMAPI fallback, and strict log privacy hygiene.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::dictation_client::is_daily_quota_exhausted;
use crate::GeminiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Model,
    Assistant,
    System,
}

#[derive(Debug)]
#[allow(dead_code)]
enum CallError {
    Timeout,
    RateLimit { status: u16, body: String },
    Api(String),
    Network(String),
}

/// Parses JSON response { "answer": "...", "emotion": "..." } with fallback.
pub fn parse_json_emotion(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim();
    let unwrapped = if let Some(stripped) = trimmed.strip_prefix("```json") {
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else if let Some(stripped) = trimmed.strip_prefix("```") {
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else {
        trimmed
    };

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(unwrapped) {
        if let Some(obj) = v.as_object() {
            let answer = obj.get("answer").and_then(|a| a.as_str()).unwrap_or("");
            let emotion = obj
                .get("emotion")
                .and_then(|e| e.as_str())
                .map(|s| s.to_ascii_lowercase());
            if !answer.is_empty() {
                return (answer.to_string(), emotion);
            }
        }
    }

    if let (Some(start), Some(end)) = (unwrapped.find('{'), unwrapped.rfind('}')) {
        if start < end {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&unwrapped[start..=end]) {
                if let Some(obj) = v.as_object() {
                    let answer = obj.get("answer").and_then(|a| a.as_str()).unwrap_or("");
                    let emotion = obj
                        .get("emotion")
                        .and_then(|e| e.as_str())
                        .map(|s| s.to_ascii_lowercase());
                    if !answer.is_empty() {
                        return (answer.to_string(), emotion);
                    }
                }
            }
        }
    }

    (unwrapped.to_string(), None)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnResponse {
    pub text: String,
    pub detected_emotion: Option<String>,
    pub served_backend: String,
}

#[async_trait]
pub trait ChatClient: Send + Sync {
    async fn send_message(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError>;

    fn last_served_backend(&self) -> String;
}

/// Dynamic timeout configuration scaling with prompt and context length.
#[derive(Debug, Clone, Copy)]
pub struct ChatTimeoutConfig {
    pub primary_base_ms: u64,
    pub primary_scale_per_char_ms: u64,
    pub primary_cap_ms: u64,
    pub fallback_base_ms: u64,
    pub fallback_scale_per_char_ms: u64,
    pub fallback_cap_ms: u64,
}

impl Default for ChatTimeoutConfig {
    fn default() -> Self {
        Self {
            primary_base_ms: 3000,
            primary_scale_per_char_ms: 2,
            primary_cap_ms: 12000,
            fallback_base_ms: 4500,
            fallback_scale_per_char_ms: 3,
            fallback_cap_ms: 15000,
        }
    }
}

impl ChatTimeoutConfig {
    pub fn compute_primary_timeout(&self, total_chars: usize) -> Duration {
        let scaled = self.primary_base_ms + (total_chars as u64 * self.primary_scale_per_char_ms);
        Duration::from_millis(scaled.min(self.primary_cap_ms))
    }

    pub fn compute_fallback_timeout(&self, total_chars: usize) -> Duration {
        let scaled = self.fallback_base_ms + (total_chars as u64 * self.fallback_scale_per_char_ms);
        Duration::from_millis(scaled.min(self.fallback_cap_ms))
    }
}

/// Multi-turn Gemini chat client with automatic fallback and log privacy.
pub struct GeminiChatClient {
    pub primary_model: String,
    pub fallback_model: String,
    pub api_key: Option<String>,
    pub freellmapi_api_key: Option<String>,
    pub freellmapi_base_url: String,
    pub allow_third_party_fallback: bool,
    pub system_prompt: String,
    pub base_url: Option<String>,
    pub client: reqwest::Client,
    pub timeout_config: ChatTimeoutConfig,
    pub primary_unhealthy_until: Arc<Mutex<Option<Instant>>>,
    pub google_quota_exhausted_until: Arc<Mutex<Option<Instant>>>,
    pub last_served_backend: Arc<Mutex<String>>,
}

impl GeminiChatClient {
    pub fn new(
        primary_model: impl Into<String>,
        fallback_model: impl Into<String>,
        api_key: Option<String>,
        allow_third_party: bool,
        system_prompt: impl Into<String>,
    ) -> Self {
        let freellmapi_api_key = std::env::var("FREELLMAPI_API_KEY").ok();
        let freellmapi_base_url = std::env::var("FREELLMAPI_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3001/v1".to_string());

        Self {
            primary_model: primary_model.into(),
            fallback_model: fallback_model.into(),
            api_key,
            freellmapi_api_key,
            freellmapi_base_url,
            allow_third_party_fallback: allow_third_party,
            system_prompt: system_prompt.into(),
            base_url: None,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_millis(1500))
                .build()
                .unwrap_or_default(),
            timeout_config: ChatTimeoutConfig::default(),
            primary_unhealthy_until: Arc::new(Mutex::new(None)),
            google_quota_exhausted_until: Arc::new(Mutex::new(None)),
            last_served_backend: Arc::new(Mutex::new("none".to_string())),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    fn is_primary_healthy(&self) -> bool {
        if let Ok(lock) = self.primary_unhealthy_until.lock() {
            if let Some(until) = *lock {
                return Instant::now() >= until;
            }
        }
        true
    }

    fn mark_primary_unhealthy(&self, duration: Duration) {
        if let Ok(mut lock) = self.primary_unhealthy_until.lock() {
            *lock = Some(Instant::now() + duration);
        }
    }

    fn is_google_quota_exhausted(&self) -> bool {
        if let Ok(lock) = self.google_quota_exhausted_until.lock() {
            if let Some(until) = *lock {
                return Instant::now() < until;
            }
        }
        false
    }

    fn mark_google_quota_exhausted(&self, duration: Duration) {
        if let Ok(mut lock) = self.google_quota_exhausted_until.lock() {
            *lock = Some(Instant::now() + duration);
        }
    }
}

impl Default for GeminiChatClient {
    fn default() -> Self {
        let key = std::env::var("GEMINI_API_KEY").ok();
        let allow_third_party = std::env::var("ALLOW_THIRD_PARTY_FALLBACK")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        let prompt = ruby_config::load_system_prompt();
        Self::new(
            "gemini-3.8-flash",
            "gemini-3-flash-preview",
            key,
            allow_third_party,
            prompt,
        )
    }
}

#[async_trait]
impl ChatClient for GeminiChatClient {
    async fn send_message(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError> {
        let total_chars =
            history.iter().map(|m| m.content.len()).sum::<usize>() + new_message.len();

        let api_key = match &self.api_key {
            Some(k) if !k.trim().is_empty() => k.clone(),
            _ => {
                // If Google key missing, check if FreeLLMAPI is available
                if self.allow_third_party_fallback && self.freellmapi_api_key.is_some() {
                    return self.call_freellmapi(history, new_message).await;
                }
                return Err(GeminiError::Auth(
                    "Missing GEMINI_API_KEY for chat client".into(),
                ));
            }
        };

        let google_quota_blocked = self.is_google_quota_exhausted();

        // 1. Primary Model Attempt (gemini-3.8-flash)
        if !google_quota_blocked && self.is_primary_healthy() {
            let timeout = self.timeout_config.compute_primary_timeout(total_chars);
            let start = Instant::now();
            match self
                .call_google_model(&self.primary_model, &api_key, history, new_message, timeout)
                .await
            {
                Ok(resp) => {
                    info!(
                        "Chat served by primary model '{}' in {:?}",
                        self.primary_model,
                        start.elapsed()
                    );
                    *self.last_served_backend.lock().unwrap() = self.primary_model.clone();
                    return Ok(ChatTurnResponse {
                        text: resp.0,
                        detected_emotion: resp.1,
                        served_backend: self.primary_model.clone(),
                    });
                }
                Err(e) => {
                    warn!(
                        "Primary model '{}' failed ({:?}); marking unhealthy for 60s",
                        self.primary_model, e
                    );
                    self.mark_primary_unhealthy(Duration::from_secs(60));
                    if let CallError::RateLimit { status, ref body } = e {
                        let status_code = reqwest::StatusCode::from_u16(status)
                            .unwrap_or(reqwest::StatusCode::TOO_MANY_REQUESTS);
                        if is_daily_quota_exhausted(status_code, body) {
                            self.mark_google_quota_exhausted(Duration::from_secs(1800));
                        }
                    }
                }
            }
        }

        // 2. Fallback Model Attempt (gemini-3-flash-preview)
        if !self.is_google_quota_exhausted() {
            let timeout = self.timeout_config.compute_fallback_timeout(total_chars);
            let start = Instant::now();
            match self
                .call_google_model(
                    &self.fallback_model,
                    &api_key,
                    history,
                    new_message,
                    timeout,
                )
                .await
            {
                Ok(resp) => {
                    info!(
                        "Chat served by fallback model '{}' in {:?}",
                        self.fallback_model,
                        start.elapsed()
                    );
                    *self.last_served_backend.lock().unwrap() = self.fallback_model.clone();
                    return Ok(ChatTurnResponse {
                        text: resp.0,
                        detected_emotion: resp.1,
                        served_backend: self.fallback_model.clone(),
                    });
                }
                Err(e) => {
                    warn!("Fallback model '{}' failed: {:?}", self.fallback_model, e);
                    if let CallError::RateLimit { status, ref body } = e {
                        let status_code = reqwest::StatusCode::from_u16(status)
                            .unwrap_or(reqwest::StatusCode::TOO_MANY_REQUESTS);
                        if is_daily_quota_exhausted(status_code, body) {
                            self.mark_google_quota_exhausted(Duration::from_secs(1800));
                        }
                    }
                }
            }
        }

        // 3. Third-party FreeLLMAPI Attempt
        if self.allow_third_party_fallback {
            return self.call_freellmapi(history, new_message).await;
        }

        Err(GeminiError::Api(
            "All configured Gemini models failed and third-party fallback is disabled".into(),
        ))
    }

    fn last_served_backend(&self) -> String {
        self.last_served_backend.lock().unwrap().clone()
    }
}

impl GeminiChatClient {
    async fn call_google_model(
        &self,
        model: &str,
        api_key: &str,
        history: &[ChatMessage],
        new_message: &str,
        timeout: Duration,
    ) -> Result<(String, Option<String>), CallError> {
        let base = self
            .base_url
            .clone()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
        let endpoint = format!("{}/v1beta/models/{}:generateContent", base, model);

        let mut contents = Vec::new();
        for msg in history {
            let role = match msg.role {
                ChatRole::User => "user",
                ChatRole::Model | ChatRole::Assistant => "model",
                ChatRole::System => "user",
            };
            contents.push(serde_json::json!({
                "role": role,
                "parts": [{"text": msg.content}]
            }));
        }
        contents.push(serde_json::json!({
            "role": "user",
            "parts": [{"text": new_message}]
        }));

        let payload = serde_json::json!({
            "system_instruction": {
                "parts": [{"text": self.system_prompt}]
            },
            "contents": contents,
            "generationConfig": {
                "temperature": 0.2,
                "maxOutputTokens": 900,
                "responseMimeType": "application/json"
            }
        });

        let resp = self
            .client
            .post(&endpoint)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .timeout(timeout)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    CallError::Timeout
                } else {
                    CallError::Network(e.to_string())
                }
            })?;

        let status = resp.status().as_u16();
        if status == 429 {
            let body = resp.text().await.unwrap_or_default();
            return Err(CallError::RateLimit { status, body });
        }
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(CallError::Api(format!("Status {}: {}", status, body)));
        }

        let json_val: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CallError::Api(e.to_string()))?;

        let candidate_text = json_val["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("");

        let (answer, emotion_str) = parse_json_emotion(candidate_text);
        Ok((answer, emotion_str))
    }

    async fn call_freellmapi(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError> {
        let key = self.freellmapi_api_key.as_deref().unwrap_or("auto");
        let endpoint = format!(
            "{}/chat/completions",
            self.freellmapi_base_url.trim_end_matches('/')
        );

        let mut messages = Vec::new();
        messages.push(serde_json::json!({
            "role": "system",
            "content": self.system_prompt
        }));
        for msg in history {
            let role = match msg.role {
                ChatRole::User => "user",
                ChatRole::Model | ChatRole::Assistant => "assistant",
                ChatRole::System => "system",
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": msg.content
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": new_message
        }));

        let payload = serde_json::json!({
            "model": "auto",
            "messages": messages,
            "temperature": 0.2
        });

        let resp = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", key))
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(10))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GeminiError::Network(e.to_string()))?;

        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(GeminiError::Api(format!("Status {}: {}", status, body)));
        }

        let json_val: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| GeminiError::Api(e.to_string()))?;

        let content = json_val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");

        let (answer, emotion_str) = parse_json_emotion(content);

        *self.last_served_backend.lock().unwrap() = "FreeLLMAPI".to_string();
        info!("Chat served via FreeLLMAPI");

        Ok(ChatTurnResponse {
            text: answer,
            detected_emotion: emotion_str,
            served_backend: "FreeLLMAPI".to_string(),
        })
    }
}

/// Mock chat client for offline tests.
pub struct MockChatClient {
    pub canned_text: String,
    pub canned_emotion: Option<String>,
    pub canned_backend: String,
}

impl MockChatClient {
    pub fn new(text: impl Into<String>, emotion: Option<impl Into<String>>) -> Self {
        Self {
            canned_text: text.into(),
            canned_emotion: emotion.map(|e| e.into()),
            canned_backend: "mock".to_string(),
        }
    }
}

#[async_trait]
impl ChatClient for MockChatClient {
    async fn send_message(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError> {
        let text = if history.is_empty() {
            format!("{}: {}", self.canned_text, new_message)
        } else {
            self.canned_text.clone()
        };
        Ok(ChatTurnResponse {
            text,
            detected_emotion: self.canned_emotion.clone(),
            served_backend: self.canned_backend.clone(),
        })
    }

    fn last_served_backend(&self) -> String {
        self.canned_backend.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_chat_client_responds_with_emotion() {
        let client = MockChatClient::new("Certainly! I am here to help.", Some("happy"));
        let history = vec![ChatMessage {
            role: ChatRole::User,
            content: "Can you assist me?".to_string(),
        }];
        let res = client.send_message(&history, "Thank you!").await.unwrap();
        assert_eq!(res.text, "Certainly! I am here to help.");
        assert_eq!(res.detected_emotion, Some("happy".to_string()));
        assert_eq!(res.served_backend, "mock");
    }

    #[test]
    fn test_timeout_scaling_chat() {
        let cfg = ChatTimeoutConfig::default();
        let t1 = cfg.compute_primary_timeout(100);
        let t2 = cfg.compute_primary_timeout(1000);
        assert!(t2 > t1);
        assert!(t2 <= Duration::from_millis(cfg.primary_cap_ms));
    }
}
