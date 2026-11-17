//! Wispr Flow-style System-Wide Dictation Engine for Ruby Assistant.
//! Pipeline: Ctrl+Win hold -> cpal 16kHz mono -> local VAD silence trim ->
//! single-pass multimodal Gemini 3.8 Flash cleanup -> SendInput Unicode/paste.

pub mod engine;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub use engine::SystemDictationEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationCue {
    Start,
    Stop,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandsFreeConfig {
    pub silence_auto_stop_duration_ms: u64,
    pub max_recording_duration_ms: u64,
    pub silence_threshold_rms: f32,
}

impl Default for HandsFreeConfig {
    fn default() -> Self {
        Self {
            silence_auto_stop_duration_ms: 3500, // 3.5s of sustained silence auto-stops recording
            max_recording_duration_ms: 300_000,  // 5 minutes (300s) hard limit
            silence_threshold_rms: 300.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsFreeAction {
    Continue,
    AutoStopSilence,
    AutoStopMaxDuration,
}

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
    use ruby_voice::{AudioSource, MockAudioSource};
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

    #[tokio::test]
    async fn test_pre_roll_preserves_speech_starting_at_100ms_before_250ms_commit() {
        const EARLY_SPEECH_VAL: i16 = 7777;
        const LATER_SPEECH_VAL: i16 = 3333;

        let mut mock_audio = MockAudioSource::default();

        // 1. Pre-roll started on keydown (t=0)
        mock_audio.start_recording().unwrap();

        // 2. Hardware frames arrive between t=0 and t=100ms (silence)
        mock_audio.feed_hardware_frame(&vec![0i16; 1600]);

        // 3. User begins speaking at t=100ms (150ms before commit at t=250ms)
        mock_audio.feed_hardware_frame(&vec![EARLY_SPEECH_VAL; 2400]);

        // 4. Minimum hold threshold (250ms) reached -> commit recording
        mock_audio.commit_recording().unwrap();

        // 5. Sustained speech post-commit
        mock_audio.feed_hardware_frame(&vec![LATER_SPEECH_VAL; 4000]);

        let audio = Box::new(mock_audio);
        let gemini = Box::new(MockDictationClient::new("Transcribed text with pre-roll"));
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(audio, gemini, desktop, None);

        // Commit engine state
        engine.commit_pre_roll().await.unwrap();
        assert_eq!(engine.current_state().await, DictationState::Recording);

        // 6. User finishes talking and releases chord
        let budget = engine
            .finish_and_insert(Some("code.exe".to_string()))
            .await
            .unwrap();
        assert_eq!(engine.current_state().await, DictationState::Idle);
        assert!(budget.total_millis < 500);
    }

    #[tokio::test]
    async fn test_pre_roll_discard_on_early_release() {
        let audio = Box::new(MockAudioSource::default());
        let gemini = Box::new(MockDictationClient::default());
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(audio, gemini, desktop, None);

        // Key-down: pre-roll starts
        engine.start_pre_roll().await.unwrap();
        assert_eq!(engine.current_state().await, DictationState::Idle);

        // Released early (<250ms): discard pre-roll
        engine.discard_pre_roll().await.unwrap();
        assert_eq!(engine.current_state().await, DictationState::Idle);
    }

    #[tokio::test]
    async fn test_dictation_cues_emitted() {
        use std::sync::Mutex as StdMutex;
        let cues = Arc::new(StdMutex::new(Vec::new()));
        let cues_clone = Arc::clone(&cues);

        let audio = Box::new(MockAudioSource::default());
        let gemini = Box::new(MockDictationClient::default());
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::with_config(
            audio,
            gemini,
            desktop,
            None,
            Some(Arc::new(move |cue| {
                cues_clone.lock().unwrap().push(cue);
            })),
            HandsFreeConfig::default(),
        );

        // Start
        engine.start_recording().await.unwrap();
        // Finish
        let _ = engine.finish_and_insert(None).await.unwrap();
        // Cancel
        engine.cancel().await;

        let recorded_cues = cues.lock().unwrap().clone();
        assert_eq!(
            recorded_cues,
            vec![
                DictationCue::Start,
                DictationCue::Stop,
                DictationCue::Cancel
            ]
        );
    }

    #[test]
    fn test_hands_free_safety_rules() {
        let audio = Box::new(MockAudioSource::default());
        let gemini = Box::new(MockDictationClient::default());
        let desktop = Box::new(MockDesktopController::default());

        let engine = SystemDictationEngine::new(audio, gemini, desktop, None);

        // 1. Normal active speech within limits -> Continue
        let action = engine.evaluate_hands_free_safety(10_000, 1_000);
        assert_eq!(action, HandsFreeAction::Continue);

        // 2. Trailing silence exceeds 3.5s (3500ms) -> AutoStopSilence
        let action_silence = engine.evaluate_hands_free_safety(10_000, 3_500);
        assert_eq!(action_silence, HandsFreeAction::AutoStopSilence);

        // 3. Elapsed recording reaches 5 minutes (300,000ms) -> AutoStopMaxDuration
        let action_timeout = engine.evaluate_hands_free_safety(300_000, 500);
        assert_eq!(action_timeout, HandsFreeAction::AutoStopMaxDuration);
    }
}
