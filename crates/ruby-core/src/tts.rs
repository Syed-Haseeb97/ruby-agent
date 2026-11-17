//! Milestone M2: TTS (Text-to-Speech) traits and voices.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("TTS synthesis failed: {0}")]
    SynthesisFailed(String),
    #[error("Audio output error: {0}")]
    AudioOutput(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum TtsVoice {
    Puck,
    Charon,
    Kore,
    Fenrir,
    Aoede,
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn synthesize(&self, text: &str, voice: Option<TtsVoice>) -> Result<Vec<u8>, TtsError>;
}

/// Mock TTS engine for offline testing.
pub struct MockTtsEngine {
    pub canned_wav: Vec<u8>,
}

impl MockTtsEngine {
    pub fn new(canned_wav: Vec<u8>) -> Self {
        Self { canned_wav }
    }
}

#[async_trait]
impl TtsEngine for MockTtsEngine {
    async fn synthesize(&self, _text: &str, _voice: Option<TtsVoice>) -> Result<Vec<u8>, TtsError> {
        Ok(self.canned_wav.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_tts_engine_returns_audio() {
        let canned = vec![0x52, 0x49, 0x46, 0x46]; // "RIFF"
        let engine = MockTtsEngine::new(canned.clone());
        let res = engine.synthesize("Hello world", Some(TtsVoice::Kore)).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), canned);
    }
}
