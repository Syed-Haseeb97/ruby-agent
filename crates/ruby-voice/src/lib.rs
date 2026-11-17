//! Audio capture, lock-free ringbuffers, local VAD, and pluggable TtsEngine trait.

pub mod capture;
pub mod vad;
pub mod wav;

use async_trait::async_trait;
use thiserror::Error;

pub use capture::{AudioSource, CpalAudioCapture, MockAudioSource, MockAudioTracker};
pub use vad::{frame_rms, trim_silence_vad};
pub use wav::encode_wav_16k_mono;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Audio device error: {0}")]
    Device(String),
    #[error("TTS synthesis error: {0}")]
    Synthesis(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>, VoiceError>;
}

/// Mock TTS Engine for tests.
pub struct MockTtsEngine;

#[async_trait]
impl TtsEngine for MockTtsEngine {
    async fn synthesize(&self, _text: &str, _voice: &str) -> Result<Vec<u8>, VoiceError> {
        Ok(vec![0x52, 0x49, 0x46, 0x46]) // Mock RIFF header
    }
}
