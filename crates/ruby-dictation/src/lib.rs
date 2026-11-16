//! Wispr Flow-style System-Wide Dictation Engine for Ruby Assistant.
//! Pipeline: Ctrl+Win hold -> cpal 16kHz mono -> local VAD silence trim ->
//! single-pass multimodal Gemini 3.8 Flash cleanup -> SendInput Unicode/paste.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum DictationError {
    #[error("Audio capture failed: {0}")]
    Capture(String),
    #[error("Gemini cleanup failed: {0}")]
    Model(String),
    #[error("Insertion failed: {0}")]
    Insertion(String),
    #[error("Dictation timed out")]
    Timeout,
}

/// Dedicated Dictation State Machine (separate from Orb emotions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum DictationState {
    Idle,
    Recording,
    Processing,
    Inserting,
    Error,
}

/// Measured latency budget breakdown for telemetry and logging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DictationLatencyBudget {
    pub vad_millis: u64,
    pub encode_millis: u64,
    pub network_model_millis: u64,
    pub insertion_millis: u64,
    pub total_millis: u64,
}

impl DictationLatencyBudget {
    pub fn is_within_target(&self) -> bool {
        self.total_millis <= 2000
    }
}

#[async_trait]
pub trait DictationEngine: Send + Sync {
    async fn start_recording(&self) -> Result<(), DictationError>;
    async fn finish_and_insert(
        &self,
        foreground_process: &str,
    ) -> Result<DictationLatencyBudget, DictationError>;
    fn cancel(&self);
    fn current_state(&self) -> DictationState;
}
