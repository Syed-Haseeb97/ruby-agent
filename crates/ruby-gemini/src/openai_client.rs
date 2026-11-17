//! OpenAI-compatible Dictation Client (e.g. FreeLLMAPI proxy).
//! Third, last-resort fallback backend behind DictationClient.

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::dictation_client::{
    calculate_audio_duration_secs, redact_sensitive_content, TimeoutConfig, DICTATION_SYSTEM_PROMPT,
};
use crate::key_storage::{get_freellmapi_api_key, get_freellmapi_base_url};
use crate::DictationClient;
use crate::GeminiError;

pub const DEFAULT_FREELLMAPI_MODEL: &str = "auto";
pub const FREELLMAPI_CONNECT_TIMEOUT: Duration = Duration::from_millis(1000);

/// OpenAI-compatible dictation client supporting:
/// Path 1: `chat/completions` with `input_audio` { data: base64, format: "wav" }
/// Path 2: `POST /audio/transcriptions` then text-only `chat/completions` cleanup when audio is rejected (4xx).
pub struct OpenAiCompatDictationClient {
    client: Client,
    base_url: String,
    explicit_api_key: Option<String>,
    model: String,
    last_served_backend: Arc<Mutex<Option<String>>>,
    last_served_model: Arc<Mutex<Option<String>>>,
    timeout_config: TimeoutConfig,
}

impl Default for OpenAiCompatDictationClient {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

impl OpenAiCompatDictationClient {
    pub fn new(api_key: Option<String>, base_url: Option<String>, model: Option<String>) -> Self {
        Self::with_config(api_key, base_url, model, TimeoutConfig::default())
    }

    pub fn with_config(
        api_key: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
        timeout_config: TimeoutConfig,
    ) -> Self {
        let client = Client::builder()
            .connect_timeout(FREELLMAPI_CONNECT_TIMEOUT)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let resolved_base_url = base_url
            .or_else(|| std::env::var("FREELLMAPI_BASE_URL").ok())
            .unwrap_or_else(get_freellmapi_base_url)
            .trim_end_matches('/')
            .to_string();

        let resolved_model = model
            .or_else(|| std::env::var("FREELLMAPI_MODEL").ok())
            .unwrap_or_else(|| DEFAULT_FREELLMAPI_MODEL.to_string());

        Self {
            client,
            base_url: resolved_base_url,
            explicit_api_key: api_key,
            model: resolved_model,
            last_served_backend: Arc::new(Mutex::new(None)),
            last_served_model: Arc::new(Mutex::new(None)),
            timeout_config,
        }
    }

    pub fn last_served_backend(&self) -> Option<String> {
        self.last_served_backend.lock().ok().and_then(|m| m.clone())
    }

    pub fn last_served_model(&self) -> Option<String> {
        self.last_served_model.lock().ok().and_then(|m| m.clone())
    }
}

#[async_trait]
impl DictationClient for OpenAiCompatDictationClient {
    fn last_served_backend(&self) -> Option<String> {
        self.last_served_backend.lock().ok().and_then(|m| m.clone())
    }

    async fn transcribe_and_cleanup(
        &self,
        wav_audio_bytes: &[u8],
        target_process_name: Option<&str>,
    ) -> Result<String, GeminiError> {
        let key = self
            .explicit_api_key
            .clone()
            .or_else(get_freellmapi_api_key)
            .ok_or_else(|| {
                GeminiError::Auth(
                    "FreeLLMAPI API key not found in keyring or FREELLMAPI_API_KEY".to_string(),
                )
            })?;

        if wav_audio_bytes.is_empty() {
            return Ok(String::new());
        }

        let audio_duration_secs = calculate_audio_duration_secs(wav_audio_bytes);
        let per_attempt_timeout = self
            .timeout_config
            .compute_fallback_timeout(audio_duration_secs);

        let app_context = match target_process_name {
            Some(proc) => format!("Active application: {}", proc),
            None => "Active application: generic text editor".to_string(),
        };

        let auth_header = format!("Bearer {}", key.trim());

        // ------------------------------------------------------------------
        // Path 1: chat/completions with input_audio { data: base64, format: "wav" }
        // ------------------------------------------------------------------
        let audio_b64 = BASE64.encode(wav_audio_bytes);
        let path1_url = format!("{}/chat/completions", self.base_url);

        let chat_audio_body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": DICTATION_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": audio_b64,
                                "format": "wav"
                            }
                        },
                        {
                            "type": "text",
                            "text": format!("{}\nTranscribe and clean up the speech in this audio clip.", app_context)
                        }
                    ]
                }
            ],
            "temperature": 0.1
        });

        debug!(
            "Attempting FreeLLMAPI Path 1 (chat/completions with input_audio) to {} (timeout: {:?})",
            path1_url, per_attempt_timeout
        );

        let send_path1 = self
            .client
            .post(&path1_url)
            .header("Authorization", &auth_header)
            .json(&chat_audio_body)
            .send();

        let path1_result = tokio::time::timeout(per_attempt_timeout, send_path1).await;

        match path1_result {
            Ok(Ok(response)) => {
                let status = response.status();
                let status_code = status.as_u16();
                if status.is_success() {
                    let res_json: serde_json::Value = response
                        .json()
                        .await
                        .map_err(|e| GeminiError::Serialization(e.to_string()))?;

                    let text = res_json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if let Ok(mut lock) = self.last_served_backend.lock() {
                        *lock = Some("freellmapi:chat_audio".to_string());
                    }
                    if let Ok(mut lock) = self.last_served_model.lock() {
                        *lock = Some(
                            res_json["model"]
                                .as_str()
                                .unwrap_or(&self.model)
                                .to_string(),
                        );
                    }

                    info!("FreeLLMAPI Path 1 (chat_audio) succeeded");
                    return Ok(text);
                } else if status_code == 401 || status_code == 403 {
                    // Authentication or permission configuration error: surface immediately
                    let err_body = response.text().await.unwrap_or_default();
                    let sanitized = redact_sensitive_content(&err_body, Some(&key));
                    error!(
                        "FreeLLMAPI authentication error (status {}): {}",
                        status, sanitized
                    );
                    return Err(GeminiError::Auth(format!(
                        "Status {}: {}",
                        status, sanitized
                    )));
                } else if status_code == 429 {
                    // 429 follows rate-limit path: surface immediately, do NOT fall back to Path 2
                    let err_body = response.text().await.unwrap_or_default();
                    let sanitized = redact_sensitive_content(&err_body, Some(&key));
                    warn!("FreeLLMAPI rate limit (status 429): {}", sanitized);
                    return Err(GeminiError::Api(format!(
                        "Status 429 Rate Limit: {}",
                        sanitized
                    )));
                } else if status_code == 400 || status_code == 415 || status_code == 422 {
                    // Audio-unsupported responses: 400 Bad Request, 415 Unsupported Media Type, 422 Unprocessable Entity
                    // Proxy rejects input_audio -> fall back to Path 2 (audio/transcriptions + chat)
                    let err_body = response.text().await.unwrap_or_default();
                    warn!(
                        "FreeLLMAPI Path 1 rejected audio with status {} ('{}'). Falling back to Path 2 (transcribe + chat)",
                        status, redact_sensitive_content(&err_body, Some(&key))
                    );
                } else {
                    // Other client error or 5xx server error: surface error directly without Path 2
                    let err_body = response.text().await.unwrap_or_default();
                    let sanitized = redact_sensitive_content(&err_body, Some(&key));
                    error!("FreeLLMAPI Path 1 error (status {}): {}", status, sanitized);
                    return Err(GeminiError::Api(format!(
                        "Status {}: {}",
                        status, sanitized
                    )));
                }
            }
            Ok(Err(e)) => {
                let sanitized = redact_sensitive_content(&e.to_string(), Some(&key));
                error!("FreeLLMAPI Path 1 network/connect error: {}", sanitized);
                return Err(GeminiError::Network(sanitized));
            }
            Err(_) => {
                error!(
                    "FreeLLMAPI Path 1 timed out after {:?}",
                    per_attempt_timeout
                );
                return Err(GeminiError::Network(format!(
                    "FreeLLMAPI request timed out after {:?}",
                    per_attempt_timeout
                )));
            }
        }

        // ------------------------------------------------------------------
        // Path 2: POST /v1/audio/transcriptions -> text-only chat cleanup
        // ------------------------------------------------------------------
        let transcribe_url = format!("{}/audio/transcriptions", self.base_url);
        debug!(
            "Attempting FreeLLMAPI Path 2 (audio/transcriptions) to {}",
            transcribe_url
        );

        let part = reqwest::multipart::Part::bytes(wav_audio_bytes.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| GeminiError::Api(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", self.model.clone());

        let send_transcribe = self
            .client
            .post(&transcribe_url)
            .header("Authorization", &auth_header)
            .multipart(form)
            .send();

        let transcribe_res = match tokio::time::timeout(per_attempt_timeout, send_transcribe).await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                let sanitized = redact_sensitive_content(&e.to_string(), Some(&key));
                error!(
                    "FreeLLMAPI Path 2 transcription network error: {}",
                    sanitized
                );
                return Err(GeminiError::Network(sanitized));
            }
            Err(_) => {
                return Err(GeminiError::Network(format!(
                    "FreeLLMAPI transcription request timed out after {:?}",
                    per_attempt_timeout
                )));
            }
        };

        let tr_status = transcribe_res.status();
        if !tr_status.is_success() {
            let err_body = transcribe_res.text().await.unwrap_or_default();
            let sanitized = redact_sensitive_content(&err_body, Some(&key));
            error!(
                "FreeLLMAPI transcription API error (status {}): {}",
                tr_status, sanitized
            );
            return Err(GeminiError::Api(format!(
                "Status {}: {}",
                tr_status, sanitized
            )));
        }

        let tr_json: serde_json::Value = transcribe_res
            .json()
            .await
            .map_err(|e| GeminiError::Serialization(e.to_string()))?;

        let raw_transcription = tr_json["text"].as_str().unwrap_or("").trim().to_string();
        if raw_transcription.is_empty() {
            if let Ok(mut lock) = self.last_served_backend.lock() {
                *lock = Some("freellmapi:transcribe_then_chat".to_string());
            }
            return Ok(String::new());
        }

        // Path 2b: Text-only chat cleanup
        debug!("FreeLLMAPI Path 2b text cleanup for raw transcription");
        let text_cleanup_body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": DICTATION_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": format!(
                        "{}\nRaw transcription:\n\"\"\"\n{}\n\"\"\"\nTranscribe and clean up according to instructions. Output ONLY the clean text.",
                        app_context, raw_transcription
                    )
                }
            ],
            "temperature": 0.1
        });

        let send_cleanup = self
            .client
            .post(&path1_url)
            .header("Authorization", &auth_header)
            .json(&text_cleanup_body)
            .send();

        let cleanup_res = match tokio::time::timeout(per_attempt_timeout, send_cleanup).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                let sanitized = redact_sensitive_content(&e.to_string(), Some(&key));
                error!("FreeLLMAPI Path 2 cleanup network error: {}", sanitized);
                return Err(GeminiError::Network(sanitized));
            }
            Err(_) => {
                return Err(GeminiError::Network(format!(
                    "FreeLLMAPI cleanup request timed out after {:?}",
                    per_attempt_timeout
                )));
            }
        };

        let cl_status = cleanup_res.status();
        if !cl_status.is_success() {
            let err_body = cleanup_res.text().await.unwrap_or_default();
            let sanitized = redact_sensitive_content(&err_body, Some(&key));
            error!(
                "FreeLLMAPI cleanup API error (status {}): {}",
                cl_status, sanitized
            );
            return Err(GeminiError::Api(format!(
                "Status {}: {}",
                cl_status, sanitized
            )));
        }

        let cl_json: serde_json::Value = cleanup_res
            .json()
            .await
            .map_err(|e| GeminiError::Serialization(e.to_string()))?;

        let clean_text = cl_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        if let Ok(mut lock) = self.last_served_backend.lock() {
            *lock = Some("freellmapi:transcribe_then_chat".to_string());
        }
        if let Ok(mut lock) = self.last_served_model.lock() {
            *lock = Some(cl_json["model"].as_str().unwrap_or(&self.model).to_string());
        }

        info!("FreeLLMAPI Path 2 (transcribe_then_chat) succeeded");
        Ok(clean_text)
    }
}
