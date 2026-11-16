//! Strongly-typed Gemini API client for Ruby Assistant.
//! Models: `gemini-3.8-flash` (generateContent, dictation, interactions), `gemini-3.8-live` (Live API).
//! Supports `thinkingConfig: { thinkingLevel: "low" }`.

pub mod dictation_client;
pub mod key_storage;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use dictation_client::{DictationClient, GeminiDictationClient, MockDictationClient};
pub use key_storage::{get_gemini_api_key, set_gemini_api_key};

pub const MODEL_CHAT_FLASH: &str = "gemini-3.8-flash";
pub const MODEL_LIVE_AUDIO: &str = "gemini-3.8-live";

#[derive(Debug, Error)]
pub enum GeminiError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: String, // "low", "medium", "high"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_text(
        &self,
        prompt: &str,
        system_instruction: Option<&str>,
    ) -> Result<String, GeminiError>;
    async fn clean_dictation_audio(
        &self,
        audio_data: &[u8],
        mime_type: &str,
        foreground_app: &str,
    ) -> Result<String, GeminiError>;
}

/// Mock client for offline CI and integration testing.
pub struct MockLlmClient {
    pub canned_response: String,
}

impl MockLlmClient {
    pub fn new(canned_response: impl Into<String>) -> Self {
        Self {
            canned_response: canned_response.into(),
        }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn generate_text(
        &self,
        _prompt: &str,
        _system_instruction: Option<&str>,
    ) -> Result<String, GeminiError> {
        Ok(self.canned_response.clone())
    }

    async fn clean_dictation_audio(
        &self,
        _audio_data: &[u8],
        _mime_type: &str,
        _foreground_app: &str,
    ) -> Result<String, GeminiError> {
        Ok(self.canned_response.clone())
    }
}
