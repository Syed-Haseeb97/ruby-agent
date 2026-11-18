//! Gemini Vision Computer Use Client with Primary -> Fallback Routing and FreeLLMAPI Protection.
//! Implements multimodal reasoning over screen captures.
//!
//! Enforces:
//! 1. Primary: gemini-3.8-flash (fast, high-resolution visual grounding)
//! 2. Fallback: gemini-3-flash-preview
//! 3. Third-party FreeLLMAPI fallback:
//!    - OFF unless allow_third_party_fallback == true
//!    - Per-task explicit confirmation requirement stating screenshots leave the machine
//!    - If not explicitly confirmed, returns Err and refuses third-party call.

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

use super::action::ComputerAction;
use super::agent_loop::{ComputerUseModelClient, StepRecord};
use super::executor::ScreenCapture;

pub struct GeminiVisionComputerUseModelClient {
    pub primary_model: String,
    pub fallback_model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub allow_third_party_fallback: bool,
    pub freellmapi_base_url: String,
    pub freellmapi_api_key: Option<String>,
    pub third_party_export_confirmed: Arc<AtomicBool>,
    pub client: reqwest::Client,
}

impl GeminiVisionComputerUseModelClient {
    pub fn new(
        primary_model: impl Into<String>,
        fallback_model: impl Into<String>,
        api_key: Option<String>,
        allow_third_party: bool,
    ) -> Self {
        let freellmapi_api_key = std::env::var("FREELLMAPI_API_KEY").ok();
        let freellmapi_base_url = std::env::var("FREELLMAPI_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3001/v1".to_string());

        Self {
            primary_model: primary_model.into(),
            fallback_model: fallback_model.into(),
            api_key,
            base_url: None,
            allow_third_party_fallback: allow_third_party,
            freellmapi_base_url,
            freellmapi_api_key,
            third_party_export_confirmed: Arc::new(AtomicBool::new(false)),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn confirm_third_party_export(&self) {
        self.third_party_export_confirmed
            .store(true, Ordering::SeqCst);
    }

    pub fn parse_action_response(response_text: &str) -> Result<ComputerAction, String> {
        let trimmed = response_text.trim();
        let clean = if let Some(stripped) = trimmed.strip_prefix("```json") {
            stripped.strip_suffix("```").unwrap_or(stripped).trim()
        } else if let Some(stripped) = trimmed.strip_prefix("```") {
            stripped.strip_suffix("```").unwrap_or(stripped).trim()
        } else {
            trimmed
        };

        if let Ok(act) = serde_json::from_str::<ComputerAction>(clean) {
            return Ok(act);
        }

        // Try extracting JSON object {...}
        if let (Some(s), Some(e)) = (clean.find('{'), clean.rfind('}')) {
            if s < e {
                if let Ok(act) = serde_json::from_str::<ComputerAction>(&clean[s..=e]) {
                    return Ok(act);
                }
            }
        }

        Err(format!(
            "Could not parse ComputerAction from response: {trimmed}"
        ))
    }
}

#[async_trait]
impl ComputerUseModelClient for GeminiVisionComputerUseModelClient {
    async fn propose_action(
        &self,
        goal: &str,
        step: usize,
        step_history: &[StepRecord],
        screen: &ScreenCapture,
    ) -> Result<ComputerAction, String> {
        let b64_jpeg = BASE64.encode(&screen.jpeg_bytes);

        let system_instruction = format!(
            "You are Ruby's autonomous Computer Use agent operating on a {w}x{h} display. \
            Goal: \"{goal}\". Current step: {step}. \
            Return ONLY a valid single JSON action representing the next action. \
            Supported actions: click(x, y, button), double_click(x, y), move(x, y), drag(start_x, start_y, end_x, end_y), \
            type_text(text, press_enter), press_key(key), hotkey(keys), wait(ms), finish(result), fail(reason).",
            w = screen.width,
            h = screen.height,
        );

        let history_summary = if step_history.is_empty() {
            "No previous steps taken.".to_string()
        } else {
            let last_3: Vec<String> = step_history
                .iter()
                .rev()
                .take(3)
                .map(|s| format!("Step {}: {} -> {}", s.step, s.action_type, s.status))
                .collect();
            format!("Recent steps:\n{}", last_3.join("\n"))
        };

        let payload = json!({
            "contents": [
                {
                    "parts": [
                        { "text": format!("{system_instruction}\n\n{history_summary}\n\nWhat is your next action?") },
                        {
                            "inline_data": {
                                "mime_type": "image/jpeg",
                                "data": b64_jpeg
                            }
                        }
                    ]
                }
            ],
            "generationConfig": {
                "response_mime_type": "application/json"
            }
        });

        // 1. Try Primary model (gemini-3.8-flash)
        if let Some(key) = &self.api_key {
            let base = self
                .base_url
                .clone()
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
            let url = format!(
                "{base}/models/{}:generateContent?key={key}",
                self.primary_model
            );

            let primary_res = self.client.post(&url).json(&payload).send().await;
            match primary_res {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(text) =
                            body["candidates"][0]["content"]["parts"][0]["text"].as_str()
                        {
                            return Self::parse_action_response(text);
                        }
                    }
                }
                Ok(resp) => {
                    warn!(
                        "Primary model ({}) returned error status {}",
                        self.primary_model,
                        resp.status()
                    );
                }
                Err(e) => {
                    warn!(
                        "Primary model ({}) network error: {}",
                        self.primary_model, e
                    );
                }
            }

            // 2. Try Fallback model (gemini-3-flash-preview)
            info!("Falling back to secondary model: {}", self.fallback_model);
            let fallback_url = format!(
                "{base}/models/{}:generateContent?key={key}",
                self.fallback_model
            );
            let fallback_res = self.client.post(&fallback_url).json(&payload).send().await;
            match fallback_res {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(text) =
                            body["candidates"][0]["content"]["parts"][0]["text"].as_str()
                        {
                            return Self::parse_action_response(text);
                        }
                    }
                }
                Ok(resp) => {
                    warn!(
                        "Fallback model ({}) returned error status {}",
                        self.fallback_model,
                        resp.status()
                    );
                }
                Err(e) => {
                    warn!(
                        "Fallback model ({}) network error: {}",
                        self.fallback_model, e
                    );
                }
            }
        }

        // 3. Third-party FreeLLMAPI Fallback
        if self.allow_third_party_fallback {
            if !self.third_party_export_confirmed.load(Ordering::SeqCst) {
                return Err(
                    "Third-party FreeLLMAPI fallback requires explicit per-task confirmation: \
                    screenshots and screen content will leave your machine to third-party endpoint."
                        .to_string(),
                );
            }

            info!(
                "Routing Computer Use vision request to third-party FreeLLMAPI: {}",
                self.freellmapi_base_url
            );
            let openai_payload = json!({
                "model": "auto",
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "text", "text": system_instruction },
                            { "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{b64_jpeg}") } }
                        ]
                    }
                ],
                "response_format": { "type": "json_object" }
            });

            let mut req = self
                .client
                .post(format!("{}/chat/completions", self.freellmapi_base_url))
                .json(&openai_payload);

            if let Some(k) = &self.freellmapi_api_key {
                req = req.header("Authorization", format!("Bearer {k}"));
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(text) = body["choices"][0]["message"]["content"].as_str() {
                            return Self::parse_action_response(text);
                        }
                    }
                    Err("Failed to parse FreeLLMAPI response".into())
                }
                Ok(resp) => Err(format!(
                    "FreeLLMAPI returned error status: {}",
                    resp.status()
                )),
                Err(e) => Err(format!("FreeLLMAPI network error: {e}")),
            }
        } else {
            Err(
                "All primary and secondary Gemini models failed, and allow_third_party_fallback is false"
                    .to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_action_json_clean_and_fenced() {
        let raw_json = r#"{"action":"click","x":500,"y":300,"button":"Left"}"#;
        let act = GeminiVisionComputerUseModelClient::parse_action_response(raw_json).unwrap();
        assert_eq!(act.action_type_str(), "click");

        let fenced = format!("```json\n{raw_json}\n```");
        let act2 = GeminiVisionComputerUseModelClient::parse_action_response(&fenced).unwrap();
        assert_eq!(act2.action_type_str(), "click");
    }

    #[tokio::test]
    async fn test_third_party_fallback_blocked_without_explicit_confirmation() {
        let client = GeminiVisionComputerUseModelClient::new(
            "gemini-3.8-flash",
            "gemini-3-flash-preview",
            None, // No Gemini key, triggering fallback immediately
            true, // allow_third_party_fallback is true
        );

        let screen = ScreenCapture {
            width: 1920,
            height: 1080,
            jpeg_bytes: vec![0xFF, 0xD8, 0xFF, 0xE0],
        };

        let res = client.propose_action("Test goal", 1, &[], &screen).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("requires explicit per-task confirmation"));
        assert!(err.contains("screenshots and screen content will leave your machine"));
    }

    #[tokio::test]
    async fn test_third_party_fallback_refused_when_allow_fallback_is_false() {
        let client = GeminiVisionComputerUseModelClient::new(
            "gemini-3.8-flash",
            "gemini-3-flash-preview",
            None,
            false, // allow_third_party_fallback is false
        );

        let screen = ScreenCapture {
            width: 1920,
            height: 1080,
            jpeg_bytes: vec![0xFF, 0xD8, 0xFF, 0xE0],
        };

        let res = client.propose_action("Test goal", 1, &[], &screen).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("allow_third_party_fallback is false"));
    }
}
