//! Wake word detection abstraction (Phase M6 / F3).
//! Default implementation is disabled (`DisabledWakeWordDetector`) to ensure
//! zero unauthorized background audio processing or listening.

use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeWordState {
    Disabled,
    Idle,
    Listening,
    Triggered,
}

/// Interface for streaming wake word detection engines.
#[async_trait]
pub trait WakeWordDetector: Send + Sync {
    /// Returns whether the wake word detector is currently enabled.
    fn is_enabled(&self) -> bool;

    /// Current operational state of the wake word detector.
    fn state(&self) -> WakeWordState;

    /// Feeds an audio chunk (16kHz mono f32 samples) into the detector.
    /// Returns true if the wake word was detected in this chunk.
    fn process_audio_chunk(&mut self, samples: &[f32]) -> bool;

    /// Resets detector state (e.g. after a triggered event or on session abort).
    fn reset(&mut self);
}

/// Default production detector that remains permanently disabled (fail-safe).
#[derive(Debug, Default, Clone)]
pub struct DisabledWakeWordDetector;

#[async_trait]
impl WakeWordDetector for DisabledWakeWordDetector {
    fn is_enabled(&self) -> bool {
        false
    }

    fn state(&self) -> WakeWordState {
        WakeWordState::Disabled
    }

    fn process_audio_chunk(&mut self, _samples: &[f32]) -> bool {
        // Permanently disabled: Never triggers and discards samples
        false
    }

    fn reset(&mut self) {}
}

/// Mock wake word detector for offline unit testing and state verification.
#[derive(Debug, Clone)]
pub struct MockWakeWordDetector {
    pub enabled: bool,
    pub state: WakeWordState,
    pub trigger_on_next_chunk: bool,
    pub processed_samples_count: usize,
}

impl Default for MockWakeWordDetector {
    fn default() -> Self {
        Self {
            enabled: false,
            state: WakeWordState::Disabled,
            trigger_on_next_chunk: false,
            processed_samples_count: 0,
        }
    }
}

impl MockWakeWordDetector {
    pub fn new_enabled() -> Self {
        Self {
            enabled: true,
            state: WakeWordState::Listening,
            trigger_on_next_chunk: false,
            processed_samples_count: 0,
        }
    }
}

#[async_trait]
impl WakeWordDetector for MockWakeWordDetector {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn state(&self) -> WakeWordState {
        self.state
    }

    fn process_audio_chunk(&mut self, samples: &[f32]) -> bool {
        if !self.enabled {
            return false;
        }
        self.processed_samples_count += samples.len();
        if self.trigger_on_next_chunk {
            self.state = WakeWordState::Triggered;
            self.trigger_on_next_chunk = false;
            true
        } else {
            self.state = WakeWordState::Listening;
            false
        }
    }

    fn reset(&mut self) {
        if self.enabled {
            self.state = WakeWordState::Listening;
        } else {
            self.state = WakeWordState::Disabled;
        }
        self.trigger_on_next_chunk = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_wake_word_detector_is_off_by_default() {
        let mut detector = DisabledWakeWordDetector;
        assert!(!detector.is_enabled());
        assert_eq!(detector.state(), WakeWordState::Disabled);

        let dummy_chunk = vec![0.0f32; 1600];
        let triggered = detector.process_audio_chunk(&dummy_chunk);
        assert!(!triggered);
        assert_eq!(detector.state(), WakeWordState::Disabled);
    }

    #[test]
    fn test_mock_wake_word_detector_trigger_and_reset() {
        let mut detector = MockWakeWordDetector::new_enabled();
        assert!(detector.is_enabled());
        assert_eq!(detector.state(), WakeWordState::Listening);

        let dummy_chunk = vec![0.1f32; 512];
        assert!(!detector.process_audio_chunk(&dummy_chunk));
        assert_eq!(detector.processed_samples_count, 512);

        // Arm trigger
        detector.trigger_on_next_chunk = true;
        assert!(detector.process_audio_chunk(&dummy_chunk));
        assert_eq!(detector.state(), WakeWordState::Triggered);

        // Reset returns to listening
        detector.reset();
        assert_eq!(detector.state(), WakeWordState::Listening);
    }
}
