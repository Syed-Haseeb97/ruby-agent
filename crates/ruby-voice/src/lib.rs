//! Audio capture, lock-free ringbuffers, local VAD, and pluggable TtsEngine trait.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Audio device error: {0}")]
    Device(String),
    #[error("TTS synthesis error: {0}")]
    Synthesis(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub trait AudioSource: Send + Sync {
    fn start_recording(&mut self) -> Result<(), VoiceError>;
    fn stop_recording(&mut self) -> Result<Vec<u8>, VoiceError>;
}

/// Trims leading and trailing silence based on energy thresholds.
pub fn trim_silence(samples: &[i16], threshold: i16) -> &[i16] {
    let start = samples
        .iter()
        .position(|&s| s.abs() > threshold)
        .unwrap_or(0);
    let end = samples
        .iter()
        .rposition(|&s| s.abs() > threshold)
        .map(|idx| idx + 1)
        .unwrap_or(samples.len());

    if start >= end {
        &[]
    } else {
        &samples[start..end]
    }
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
