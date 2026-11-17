//! Audio Capture implementation using cpal, lock-free ringbuffers, and a mock source.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rtrb::RingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::VoiceError;

pub trait AudioSource: Send + Sync {
    fn start_recording(&mut self) -> Result<(), VoiceError>;
    fn commit_recording(&mut self) -> Result<(), VoiceError> {
        Ok(())
    }
    fn discard_recording(&mut self) -> Result<(), VoiceError> {
        let _ = self.stop_recording();
        Ok(())
    }
    fn stop_recording(&mut self) -> Result<Vec<i16>, VoiceError>;
}

/// Live hardware audio capture via cpal with lock-free ring buffer and 16 kHz mono resampling.
/// The underlying cpal Stream is created, operated, and dropped entirely on its dedicated thread.
pub struct CpalAudioCapture {
    is_recording: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<Result<Vec<i16>, VoiceError>>>,
}

impl Default for CpalAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalAudioCapture {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }
}

impl AudioSource for CpalAudioCapture {
    fn start_recording(&mut self) -> Result<(), VoiceError> {
        let is_recording = Arc::new(AtomicBool::new(true));
        self.is_recording = Arc::clone(&is_recording);
        let is_rec_worker = Arc::clone(&is_recording);

        let (init_tx, init_rx) = crossbeam_channel::bounded::<Result<(), VoiceError>>(1);

        let worker_handle = std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    let err = VoiceError::Device("No default audio input device found".to_string());
                    let _ = init_tx.send(Err(err));
                    return Err(VoiceError::Device("No default input device".to_string()));
                }
            };

            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let err = VoiceError::Device(format!("Default input config error: {:?}", e));
                    let _ = init_tx.send(Err(err));
                    return Err(VoiceError::Device("Config error".to_string()));
                }
            };

            let sample_rate = config.sample_rate().0;
            let channels = config.channels() as usize;

            let ring_capacity = (sample_rate as usize * channels).max(16000) * 5;
            let (mut producer, mut consumer) = RingBuffer::<f32>::new(ring_capacity);

            let err_fn = |err| {
                eprintln!("cpal audio stream error: {:?}", err);
            };

            let stream_res = match config.sample_format() {
                SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        for &sample in data {
                            let _ = producer.push(sample);
                        }
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        for &sample in data {
                            let _ = producer.push(sample as f32 / 32768.0);
                        }
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::U16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &_| {
                        for &sample in data {
                            let _ = producer.push((sample as f32 - 32768.0) / 32768.0);
                        }
                    },
                    err_fn,
                    None,
                ),
                _ => {
                    let err = VoiceError::Device("Unsupported sample format".to_string());
                    let _ = init_tx.send(Err(err));
                    return Err(VoiceError::Device("Format error".to_string()));
                }
            };

            let stream = match stream_res {
                Ok(s) => s,
                Err(e) => {
                    let err = VoiceError::Device(format!("Failed to build input stream: {:?}", e));
                    let _ = init_tx.send(Err(err));
                    return Err(VoiceError::Device("Build error".to_string()));
                }
            };

            if let Err(e) = stream.play() {
                let err = VoiceError::Device(format!("Failed to play stream: {:?}", e));
                let _ = init_tx.send(Err(err));
                return Err(VoiceError::Device("Play error".to_string()));
            }

            // Stream started successfully
            let _ = init_tx.send(Ok(()));

            let mut raw_mono = Vec::new();
            let mut channel_idx = 0;

            while is_rec_worker.load(Ordering::SeqCst) || !consumer.is_empty() {
                while let Ok(sample) = consumer.pop() {
                    if channel_idx == 0 {
                        raw_mono.push(sample);
                    }
                    channel_idx = (channel_idx + 1) % channels;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }

            drop(stream);

            // Resample to 16,000 Hz if needed
            let samples: Vec<i16> = if sample_rate == 16000 {
                raw_mono
                    .into_iter()
                    .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect()
            } else {
                let ratio = sample_rate as f64 / 16000.0;
                let target_len = (raw_mono.len() as f64 / ratio) as usize;
                let mut out = Vec::with_capacity(target_len);

                for i in 0..target_len {
                    let src_idx = (i as f64 * ratio) as usize;
                    if src_idx < raw_mono.len() {
                        let s = raw_mono[src_idx].clamp(-1.0, 1.0);
                        out.push((s * 32767.0) as i16);
                    }
                }
                out
            };

            Ok(samples)
        });

        match init_rx.recv() {
            Ok(Ok(())) => {
                self.worker_handle = Some(worker_handle);
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(VoiceError::Device(
                "Audio thread initialization failed".to_string(),
            )),
        }
    }

    fn stop_recording(&mut self) -> Result<Vec<i16>, VoiceError> {
        self.is_recording.store(false, Ordering::SeqCst);

        if let Some(handle) = self.worker_handle.take() {
            match handle.join() {
                Ok(res) => res,
                Err(_) => Err(VoiceError::Device(
                    "Recording worker thread panicked".to_string(),
                )),
            }
        } else {
            Ok(Vec::new())
        }
    }
}

/// Observable call tracker for mock audio tests.
#[derive(Debug, Default, Clone)]
pub struct MockAudioTracker {
    pub is_recording: bool,
    pub is_committed: bool,
    pub start_call_count: usize,
    pub commit_call_count: usize,
    pub discard_call_count: usize,
    pub stop_call_count: usize,
}

/// Mock AudioSource providing deterministic synthetic audio for offline tests and CI.
pub struct MockAudioSource {
    samples: Vec<i16>,
    pub is_recording: bool,
    pub is_committed: bool,
    pub start_call_count: usize,
    pub commit_call_count: usize,
    pub discard_call_count: usize,
    pub stop_call_count: usize,
    tracker: Option<std::sync::Arc<std::sync::Mutex<MockAudioTracker>>>,
    recorded_buffer: Vec<i16>,
}

impl Default for MockAudioSource {
    fn default() -> Self {
        // 1 second of synthetic 16 kHz tone (250 Hz)
        let mut samples = Vec::with_capacity(16000);
        for i in 0..16000 {
            let angle = (i as f32) * 250.0 * std::f32::consts::TAU / 16000.0;
            samples.push((angle.sin() * 2500.0) as i16);
        }
        Self {
            samples,
            is_recording: false,
            is_committed: false,
            start_call_count: 0,
            commit_call_count: 0,
            discard_call_count: 0,
            stop_call_count: 0,
            tracker: None,
            recorded_buffer: Vec::new(),
        }
    }
}

impl MockAudioSource {
    pub fn new(samples: Vec<i16>) -> Self {
        Self {
            samples,
            is_recording: false,
            is_committed: false,
            start_call_count: 0,
            commit_call_count: 0,
            discard_call_count: 0,
            stop_call_count: 0,
            tracker: None,
            recorded_buffer: Vec::new(),
        }
    }

    pub fn with_tracker(
        samples: Vec<i16>,
        tracker: std::sync::Arc<std::sync::Mutex<MockAudioTracker>>,
    ) -> Self {
        Self {
            samples,
            is_recording: false,
            is_committed: false,
            start_call_count: 0,
            commit_call_count: 0,
            discard_call_count: 0,
            stop_call_count: 0,
            tracker: Some(tracker),
            recorded_buffer: Vec::new(),
        }
    }

    /// Feeds incoming hardware audio frames into the source as time elapses.
    /// Only frames arriving while is_recording == true are captured into the buffer.
    pub fn feed_hardware_frame(&mut self, frame: &[i16]) {
        if self.is_recording {
            self.recorded_buffer.extend_from_slice(frame);
        }
    }
}

impl AudioSource for MockAudioSource {
    fn start_recording(&mut self) -> Result<(), VoiceError> {
        self.is_recording = true;
        self.is_committed = false;
        self.start_call_count += 1;
        self.recorded_buffer.clear();
        if let Some(ref t) = self.tracker {
            if let Ok(mut lock) = t.lock() {
                lock.is_recording = true;
                lock.is_committed = false;
                lock.start_call_count += 1;
            }
        }
        Ok(())
    }

    fn commit_recording(&mut self) -> Result<(), VoiceError> {
        self.is_committed = true;
        self.commit_call_count += 1;
        if let Some(ref t) = self.tracker {
            if let Ok(mut lock) = t.lock() {
                lock.is_committed = true;
                lock.commit_call_count += 1;
            }
        }
        Ok(())
    }

    fn discard_recording(&mut self) -> Result<(), VoiceError> {
        self.is_recording = false;
        self.is_committed = false;
        self.discard_call_count += 1;
        self.recorded_buffer.clear();
        self.samples.clear();
        if let Some(ref t) = self.tracker {
            if let Ok(mut lock) = t.lock() {
                lock.is_recording = false;
                lock.is_committed = false;
                lock.discard_call_count += 1;
            }
        }
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<Vec<i16>, VoiceError> {
        self.is_recording = false;
        self.is_committed = false;
        self.stop_call_count += 1;
        if let Some(ref t) = self.tracker {
            if let Ok(mut lock) = t.lock() {
                lock.is_recording = false;
                lock.is_committed = false;
                lock.stop_call_count += 1;
            }
        }
        if !self.recorded_buffer.is_empty() {
            Ok(std::mem::take(&mut self.recorded_buffer))
        } else {
            Ok(self.samples.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_level_pre_roll_timeline_preserves_speech_starting_at_100ms_before_250ms_commit() {
        // Realistic hardware timeline:
        // t = 0ms: Key-down event -> start_recording() opens pre-roll buffer.
        // t = 0..100ms: 100ms of ambient silence (1600 samples @ 16kHz).
        // t = 100..250ms: User starts speaking early (150ms = 2400 samples of distinct speech).
        // t = 250ms: Hold qualification reached -> commit_recording() called.
        // t = 250..500ms: Sustained speech post-commit (250ms = 4000 samples).
        // t = 500ms: User finishes speaking -> stop_recording().
        const EARLY_SPEECH_VAL: i16 = 7777;
        const LATER_SPEECH_VAL: i16 = 3333;

        let mut mock_source = MockAudioSource::default();

        // 1. t = 0ms: Key-down arrives -> pre-roll buffering begins
        mock_source.start_recording().unwrap();
        assert!(mock_source.is_recording);
        assert!(!mock_source.is_committed);

        // 2. t = 0..100ms: Silence frames arrive from hardware
        let silence_100ms = vec![0i16; 1600];
        mock_source.feed_hardware_frame(&silence_100ms);

        // 3. t = 100..250ms: User begins speaking BEFORE the 250ms hold threshold!
        let early_speech_150ms = vec![EARLY_SPEECH_VAL; 2400];
        mock_source.feed_hardware_frame(&early_speech_150ms);

        // 4. t = 250ms: Hold qualification elapses -> commit recording
        mock_source.commit_recording().unwrap();
        assert!(mock_source.is_committed);

        // 5. t = 250..500ms: Continued speech frames arrive
        let later_speech_250ms = vec![LATER_SPEECH_VAL; 4000];
        mock_source.feed_hardware_frame(&later_speech_250ms);

        // 6. t = 500ms: Stop recording
        let recorded = mock_source.stop_recording().unwrap();
        assert_eq!(
            recorded.len(),
            8000,
            "Buffer must contain all 8000 frames from t=0 to t=500ms"
        );

        // Assert: Early speech that started at t=100ms is retained in the buffer
        assert!(
            recorded.contains(&EARLY_SPEECH_VAL),
            "Pre-roll buffer MUST contain the speech starting at t=100ms"
        );
        assert_eq!(
            recorded[1600], EARLY_SPEECH_VAL,
            "Sample at index 1600 (t=100ms) must be the start of speech"
        );
        assert_eq!(
            recorded[3999], EARLY_SPEECH_VAL,
            "Sample at index 3999 (t=250ms) must be the end of pre-commit speech"
        );
        assert_eq!(
            recorded[4000], LATER_SPEECH_VAL,
            "Sample at index 4000 (t=250ms) must be post-commit speech"
        );
    }

    #[test]
    fn test_mock_level_old_behavior_without_preroll_fails_to_capture_early_speech() {
        // Models the OLD behavior where recording started ONLY at commit (t=250ms):
        const EARLY_SPEECH_VAL: i16 = 7777;
        const LATER_SPEECH_VAL: i16 = 3333;

        let mut old_mock_source = MockAudioSource::default();

        // 1. t = 0ms: Key-down arrives, but OLD behavior does NOT start recording!
        // (start_recording is omitted at t=0)
        assert!(!old_mock_source.is_recording);

        // 2. t = 0..100ms: Silence arrives from hardware (dropped)
        let silence_100ms = vec![0i16; 1600];
        old_mock_source.feed_hardware_frame(&silence_100ms);

        // 3. t = 100..250ms: Speech arrives from hardware (DROPPED by old behavior!)
        let early_speech_150ms = vec![EARLY_SPEECH_VAL; 2400];
        old_mock_source.feed_hardware_frame(&early_speech_150ms);

        // 4. t = 250ms: In old behavior, recording only starts now on commit!
        old_mock_source.start_recording().unwrap();
        old_mock_source.commit_recording().unwrap();

        // 5. t = 250..500ms: Continued speech arrives
        let later_speech_250ms = vec![LATER_SPEECH_VAL; 4000];
        old_mock_source.feed_hardware_frame(&later_speech_250ms);

        // 6. t = 500ms: Stop recording
        let recorded = old_mock_source.stop_recording().unwrap();

        // PROOF: Under old behavior, buffer has ONLY 4000 samples, and early speech is completely lost!
        assert_eq!(recorded.len(), 4000);
        assert!(
            !recorded.contains(&EARLY_SPEECH_VAL),
            "Old behavior PROVABLY loses all speech spoken between t=100ms and t=250ms"
        );
    }

    #[test]
    fn test_mock_level_pre_roll_discard_on_early_release() {
        let mut mock_source = MockAudioSource::default();
        mock_source.start_recording().unwrap();
        assert!(mock_source.is_recording);

        mock_source.discard_recording().unwrap();
        assert!(!mock_source.is_recording);
        assert!(!mock_source.is_committed);
        let remaining = mock_source.stop_recording().unwrap();
        assert!(
            remaining.is_empty(),
            "Discarded pre-roll should clear audio buffer"
        );
    }
}
