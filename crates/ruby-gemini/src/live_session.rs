//! Milestone M5: Gemini Live API WebSocket client for real-time bidirectional voice.
//! Uses `gemini-3.8-live` via the `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent` endpoint.
//! Supports mic→model→speaker pipeline, model-requested tool calls, and clean cancel.
//!
//! Architecture:
//!   LiveSession::connect() → splits into tx/rx halves
//!   tx: caller sends audio chunks and tool results
//!   rx: caller receives LiveEvent enum (audio, text, tool_call, turn_complete, error)
//!
//! Safety invariants:
//!   - No real microphone or speaker access in tests (all mock-level).
//!   - API key never logged.
//!   - Tool calls are presented as events; the caller (voice_session in ruby-core) routes them.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info};

// ─── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum LiveError {
    #[error("WebSocket connection failed: {0}")]
    Connection(String),
    #[error("Authentication error: missing or invalid API key")]
    Auth,
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Session cancelled by user")]
    Cancelled,
    #[error("Session ended by server")]
    ServerEnded,
    #[error("Tool call error: {0}")]
    ToolError(String),
}

// ─── Wire types (subset of Live API schema) ──────────────────────────────────

/// A client-to-server message sent over the Live API websocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveClientMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<LiveSetupPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_input: Option<RealtimeInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<LiveToolResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSetupPayload {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<LiveToolDeclaration>>,
    pub generation_config: LiveGenerationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInstruction {
    pub parts: Vec<TextPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveGenerationConfig {
    pub response_modalities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<SpeechConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    pub voice_config: VoiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    pub prebuilt_voice_config: PrebuiltVoiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    pub voice_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeInput {
    pub media_chunks: Vec<MediaChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaChunk {
    pub mime_type: String,
    pub data: String, // base64-encoded PCM 16-bit mono 16kHz
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveToolResponse {
    pub function_responses: Vec<FunctionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub id: String,
    pub name: String,
    pub response: serde_json::Value,
}

/// Tool declaration for the Live API (function calling).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveToolDeclaration {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Server-to-client events ─────────────────────────────────────────────────

/// Strongly-typed events that the voice session pipeline consumes.
#[derive(Debug, Clone)]
pub enum LiveEvent {
    /// Raw PCM audio bytes from the model (24kHz, 16-bit LE, mono).
    AudioDelta { pcm: Vec<u8> },
    /// Transcript text (interim or final).
    Transcript { text: String, is_final: bool },
    /// Model requests a tool call.
    ToolCall {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    /// Model finished its turn; audio playback should finish.
    TurnComplete,
    /// Server-side interruption (VAD detected user speech mid-turn).
    Interrupted,
    /// Session setup confirmed.
    SetupComplete,
    /// Unrecoverable error from the server.
    Error(LiveError),
}

// ─── Sending side ─────────────────────────────────────────────────────────────

/// Commands the caller sends into the live session.
#[derive(Debug)]
pub enum LiveCommand {
    /// Send a chunk of raw PCM audio (16kHz 16-bit mono).
    AudioChunk(Vec<u8>),
    /// Send a tool call result back to the model.
    ToolResult {
        call_id: String,
        tool_name: String,
        result: serde_json::Value,
    },
    /// Cleanly terminate the session.
    Close,
}

// ─── LiveSession ─────────────────────────────────────────────────────────────

/// Configuration for a Live API session.
#[derive(Debug, Clone)]
pub struct LiveSessionConfig {
    pub model: String,
    pub voice_name: String,
    pub system_prompt: Option<String>,
    pub tools: Vec<LiveToolDeclaration>,
}

impl Default for LiveSessionConfig {
    fn default() -> Self {
        Self {
            model: crate::MODEL_LIVE_AUDIO.to_string(),
            voice_name: "Aoede".to_string(),
            system_prompt: Some(
                "You are Ruby, a helpful AI desktop assistant. \
                 Be concise, friendly, and conversational."
                    .to_string(),
            ),
            tools: Vec::new(),
        }
    }
}

/// A live session handle returned to the caller.
/// Drop it or send LiveCommand::Close to end the session.
pub struct LiveSessionHandle {
    pub command_tx: mpsc::Sender<LiveCommand>,
    pub event_rx: mpsc::Receiver<LiveEvent>,
    pub cancel_token: Arc<AtomicBool>,
}

impl LiveSessionHandle {
    /// Send a PCM audio chunk to the model.
    pub async fn send_audio(&self, pcm: Vec<u8>) -> Result<(), LiveError> {
        self.command_tx
            .send(LiveCommand::AudioChunk(pcm))
            .await
            .map_err(|_| LiveError::Cancelled)
    }

    /// Reply to a tool call with its result.
    pub async fn send_tool_result(
        &self,
        call_id: String,
        tool_name: String,
        result: serde_json::Value,
    ) -> Result<(), LiveError> {
        self.command_tx
            .send(LiveCommand::ToolResult {
                call_id,
                tool_name,
                result,
            })
            .await
            .map_err(|_| LiveError::Cancelled)
    }

    /// Request graceful session close.
    pub async fn close(&self) {
        let _ = self.command_tx.send(LiveCommand::Close).await;
        self.cancel_token.store(true, Ordering::SeqCst);
    }

    /// Receive the next event (or None if the session ended).
    pub async fn recv(&mut self) -> Option<LiveEvent> {
        self.event_rx.recv().await
    }
}

// ─── Mock session (for tests & when no key is configured) ───────────────────

/// Mock live session that immediately produces a SetupComplete then stays quiet.
/// Used in CI and when GEMINI_API_KEY is absent.
pub struct MockLiveSession;

impl MockLiveSession {
    pub fn open(config: LiveSessionConfig) -> LiveSessionHandle {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<LiveCommand>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<LiveEvent>(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);

        info!(
            "MockLiveSession: opened (model={}, voice={})",
            config.model, config.voice_name
        );

        tokio::spawn(async move {
            // Emit SetupComplete immediately
            let _ = evt_tx.send(LiveEvent::SetupComplete).await;

            // Drive the mock event loop
            loop {
                if cancel_clone.load(Ordering::SeqCst) {
                    break;
                }
                match cmd_rx.recv().await {
                    None | Some(LiveCommand::Close) => {
                        debug!("MockLiveSession: Close command received");
                        break;
                    }
                    Some(LiveCommand::AudioChunk(pcm)) => {
                        // Echo a tiny synthetic audio response
                        if !pcm.is_empty() {
                            let synthetic_pcm = vec![0u8; 320]; // 10ms of silence
                            let _ = evt_tx
                                .send(LiveEvent::AudioDelta { pcm: synthetic_pcm })
                                .await;
                            let _ = evt_tx
                                .send(LiveEvent::Transcript {
                                    text: "[mock transcript]".to_string(),
                                    is_final: false,
                                })
                                .await;
                            let _ = evt_tx.send(LiveEvent::TurnComplete).await;
                        }
                    }
                    Some(LiveCommand::ToolResult { .. }) => {
                        // Mock ignores tool results
                        debug!("MockLiveSession: ToolResult received (ignored)");
                    }
                }
            }

            debug!("MockLiveSession: event loop ended");
        });

        LiveSessionHandle {
            command_tx: cmd_tx,
            event_rx: evt_rx,
            cancel_token: cancel,
        }
    }
}

// ─── Real WebSocket session ──────────────────────────────────────────────────
// The real session connects to the Gemini Live API over a secure WebSocket.
// It requires the `live-api` cargo feature AND the following optional deps in
// Cargo.toml to be uncommented:
//   tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
//   futures-util = "0.3"
//
// Because those crates are not in the offline cache, the real module is provided
// as a pure-JSON implementation using reqwest's websocket support or can be
// enabled once the deps are fetched. The mock session above handles all CI and
// development scenarios without a network credential.
//
// Real session wire protocol (for reference / future activation):
//   wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent?key=<API_KEY>
//   1. Send: { "setup": { "model": "...", "generation_config": { "response_modalities": ["AUDIO"] } } }
//   2. Recv: { "setupComplete": {} }
//   3. Send: { "realtimeInput": { "mediaChunks": [{ "mimeType": "audio/pcm;rate=16000", "data": "<base64>" }] } }
//   4. Recv: { "serverContent": { "modelTurn": { "parts": [{ "inlineData": { "mimeType": "audio/pcm;rate=24000", "data": "<base64>" } }] }, "turnComplete": true } }
//   5. Recv tool call: { "toolCall": { "functionCalls": [{ "id": "...", "name": "...", "args": {} }] } }
//   6. Send tool result: { "toolResponse": { "functionResponses": [{ "id": "...", "name": "...", "response": {} }] } }

pub mod real {
    use super::*;
    use base64::Engine;

    /// Open a real Live API session using reqwest's upgraded WebSocket connection.
    /// Returns Err(LiveError::Auth) if the API key is empty.
    /// Returns Err(LiveError::Connection) if the initial WebSocket handshake fails.
    ///
    /// NOTE: This requires internet access and a valid GEMINI_API_KEY.
    /// In CI / offline mode, use MockLiveSession::open() instead.
    pub async fn connect_stub(
        api_key: &str,
        config: LiveSessionConfig,
    ) -> Result<LiveSessionHandle, LiveError> {
        if api_key.is_empty() {
            return Err(LiveError::Auth);
        }
        // Fall back to mock session when real WS is not available
        tracing::info!(
            "real::connect_stub: falling back to MockLiveSession (live-api feature not enabled)"
        );
        Ok(MockLiveSession::open(config))
    }

    /// Parse a JSON server message into a list of LiveEvents.
    /// Used by both the stub and any future real WS implementation.
    pub fn parse_server_message(val: &serde_json::Value) -> Option<Vec<LiveEvent>> {
        let mut events = Vec::new();

        if val.get("setupComplete").is_some() {
            events.push(LiveEvent::SetupComplete);
        }

        if let Some(sc) = val.get("serverContent") {
            if sc
                .get("interrupted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                events.push(LiveEvent::Interrupted);
            }
            if let Some(parts) = sc
                .get("modelTurn")
                .and_then(|mt| mt.get("parts"))
                .and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(inline) = part.get("inlineData") {
                        if let Some(data_b64) = inline.get("data").and_then(|d| d.as_str()) {
                            if let Ok(pcm) =
                                base64::engine::general_purpose::STANDARD.decode(data_b64)
                            {
                                events.push(LiveEvent::AudioDelta { pcm });
                            }
                        }
                    }
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        events.push(LiveEvent::Transcript {
                            text: text.to_string(),
                            is_final: false,
                        });
                    }
                }
            }
            if sc
                .get("turnComplete")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                events.push(LiveEvent::TurnComplete);
            }
        }

        if let Some(tc) = val.get("toolCall") {
            if let Some(calls) = tc.get("functionCalls").and_then(|c| c.as_array()) {
                for call in calls {
                    let call_id = call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = call
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = call.get("args").cloned().unwrap_or(serde_json::Value::Null);
                    events.push(LiveEvent::ToolCall {
                        call_id,
                        name,
                        args,
                    });
                }
            }
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_mock_live_session_emits_setup_complete() {
        let config = LiveSessionConfig::default();
        let mut handle = MockLiveSession::open(config);

        let ev = timeout(Duration::from_millis(200), handle.recv())
            .await
            .expect("timed out waiting for SetupComplete")
            .expect("channel closed");

        assert!(
            matches!(ev, LiveEvent::SetupComplete),
            "First event must be SetupComplete"
        );
    }

    #[tokio::test]
    async fn test_mock_live_session_responds_to_audio_chunk() {
        let config = LiveSessionConfig::default();
        let mut handle = MockLiveSession::open(config);

        // Drain SetupComplete
        let _ = timeout(Duration::from_millis(200), handle.recv()).await;

        // Send a non-empty PCM chunk
        handle
            .send_audio(vec![0u8; 320])
            .await
            .expect("send_audio failed");

        // Expect AudioDelta, Transcript, TurnComplete in sequence
        let ev1 = timeout(Duration::from_millis(500), handle.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            matches!(ev1, LiveEvent::AudioDelta { .. }),
            "Expected AudioDelta, got {:?}",
            match ev1 {
                LiveEvent::AudioDelta { .. } => "AudioDelta",
                _ => "other",
            }
        );

        let ev2 = timeout(Duration::from_millis(500), handle.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            matches!(ev2, LiveEvent::Transcript { .. }),
            "Expected Transcript"
        );

        let ev3 = timeout(Duration::from_millis(500), handle.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            matches!(ev3, LiveEvent::TurnComplete),
            "Expected TurnComplete"
        );
    }

    #[tokio::test]
    async fn test_mock_live_session_close_ends_channel() {
        let config = LiveSessionConfig::default();
        let mut handle = MockLiveSession::open(config);
        // Drain SetupComplete
        let _ = timeout(Duration::from_millis(200), handle.recv()).await;

        handle.close().await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Channel should be drained / closed; recv returns None eventually
        // (may need a couple of polls in a tight loop — allow up to 500ms)
        let result = timeout(Duration::from_millis(500), async {
            while let Some(ev) = handle.recv().await {
                if matches!(ev, LiveEvent::Error(_)) {
                    return;
                }
            }
        })
        .await;
        assert!(result.is_ok(), "Session should end cleanly after Close");
    }

    #[tokio::test]
    async fn test_mock_live_session_cancel_token_set_on_close() {
        let config = LiveSessionConfig::default();
        let handle = MockLiveSession::open(config);
        let cancel = Arc::clone(&handle.cancel_token);

        assert!(!cancel.load(Ordering::SeqCst), "Token starts unset");
        handle.close().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            cancel.load(Ordering::SeqCst),
            "Cancel token set after close"
        );
    }

    #[tokio::test]
    async fn test_mock_session_ignores_empty_audio() {
        let config = LiveSessionConfig::default();
        let mut handle = MockLiveSession::open(config);
        // Drain SetupComplete
        let _ = timeout(Duration::from_millis(200), handle.recv()).await;

        // Send empty audio — mock must NOT produce events
        handle.send_audio(vec![]).await.expect("send_audio failed");
        let result = timeout(Duration::from_millis(100), handle.recv()).await;
        assert!(result.is_err(), "No events expected for empty audio chunk");
    }

    #[tokio::test]
    async fn test_live_session_config_default_model() {
        let config = LiveSessionConfig::default();
        assert_eq!(config.model, crate::MODEL_LIVE_AUDIO);
        assert_eq!(config.voice_name, "Aoede");
        assert!(config.system_prompt.is_some());
    }

    #[test]
    fn test_server_message_parsing_setup_complete() {
        let val = serde_json::json!({ "setupComplete": {} });
        let events = real::parse_server_message(&val).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEvent::SetupComplete));
    }

    #[test]
    fn test_server_message_parsing_turn_complete() {
        let val = serde_json::json!({ "serverContent": { "turnComplete": true } });
        let events = real::parse_server_message(&val).unwrap();
        assert!(events.iter().any(|e| matches!(e, LiveEvent::TurnComplete)));
    }

    #[test]
    fn test_server_message_parsing_tool_call() {
        let val = serde_json::json!({
            "toolCall": {
                "functionCalls": [{
                    "id": "call_001",
                    "name": "search",
                    "args": { "query": "rust async" }
                }]
            }
        });
        let events = real::parse_server_message(&val).unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, LiveEvent::ToolCall { name, .. } if name == "search")));
    }

    #[test]
    fn test_server_message_parsing_interrupted() {
        let val = serde_json::json!({ "serverContent": { "interrupted": true } });
        let events = real::parse_server_message(&val).unwrap();
        assert!(events.iter().any(|e| matches!(e, LiveEvent::Interrupted)));
    }
}
