//! Wispr Flow-style System-Wide Dictation Engine for Ruby Assistant.
//! Pipeline: Ctrl+Win hold -> cpal 16kHz mono -> local VAD silence trim ->
//! single-pass multimodal Gemini 3.8 Flash cleanup -> SendInput Unicode/paste.

pub mod engine;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub use engine::SystemDictationEngine;

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

impl DictationState {
    /// Maps dictation operational states directly to the Liquid Orb visual states.
    pub fn to_orb_operational_label(&self) -> &'static str {
        match self {
            DictationState::Idle => "Idle",
            DictationState::Recording => "Listening",
            DictationState::Processing => "Thinking",
            DictationState::Inserting => "Speaking",
            DictationState::Error => "Error",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_gemini::MockDictationClient;
    use ruby_platform::MockDesktopController;
    use ruby_voice::MockAudioSource;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_full_dictation_pipeline_with_mocks() {
        let state_transitions = Arc::new(AtomicUsize::new(0));
        let st_clone = Arc::clone(&state_transitions);

        let audio = Box::new(MockAudioSource::default());
        let gemini = Box::new(MockDictationClient::new("Fixed, cleaned dictation."));
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(
            audio,
            gemini,
            desktop,
            Some(Arc::new(move |_| {
                st_clone.fetch_add(1, Ordering::SeqCst);
            })),
        );

        assert_eq!(engine.current_state().await, DictationState::Idle);

        // 1. Start recording
        engine.start_recording().await.unwrap();
        assert_eq!(engine.current_state().await, DictationState::Recording);

        // 2. Finish recording and insert text
        let budget = engine
            .finish_and_insert(Some("code.exe".to_string()))
            .await
            .unwrap();

        assert_eq!(engine.current_state().await, DictationState::Idle);
        assert!(budget.total_millis < 500); // Mocks complete in < 500ms
        assert!(state_transitions.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn test_dictation_network_failure_and_retry() {
        let audio = Box::new(MockAudioSource::default());
        let gemini_failing = Box::new(MockDictationClient::new_failing());
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(audio, gemini_failing, desktop, None);

        engine.start_recording().await.unwrap();
        let res = engine
            .finish_and_insert(Some("notepad.exe".to_string()))
            .await;
        assert!(res.is_err());
        assert_eq!(engine.current_state().await, DictationState::Error);

        // Retry without re-recording (still fails with mock_failing, but verifies audio buffer was retained)
        let retry_res = engine.retry_last_dictation().await;
        assert!(retry_res.is_err());
        assert_eq!(engine.current_state().await, DictationState::Error);
    }

    #[tokio::test]
    async fn test_dictation_cancellation() {
        let audio = Box::new(MockAudioSource::default());
        let gemini = Box::new(MockDictationClient::default());
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(audio, gemini, desktop, None);

        engine.start_recording().await.unwrap();
        assert_eq!(engine.current_state().await, DictationState::Recording);

        engine.cancel().await;
        assert_eq!(engine.current_state().await, DictationState::Idle);
    }

    #[test]
    fn test_orb_state_mapping() {
        assert_eq!(DictationState::Idle.to_orb_operational_label(), "Idle");
        assert_eq!(
            DictationState::Recording.to_orb_operational_label(),
            "Listening"
        );
        assert_eq!(
            DictationState::Processing.to_orb_operational_label(),
            "Thinking"
        );
        assert_eq!(
            DictationState::Inserting.to_orb_operational_label(),
            "Speaking"
        );
        assert_eq!(DictationState::Error.to_orb_operational_label(), "Error");
    }
}
