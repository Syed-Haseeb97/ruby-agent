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

/// Mock AudioSource providing deterministic synthetic audio for offline tests and CI.
pub struct MockAudioSource {
    samples: Vec<i16>,
    pub is_recording: bool,
    pub is_committed: bool,
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
        }
    }
}

impl MockAudioSource {
    pub fn new(samples: Vec<i16>) -> Self {
        Self {
            samples,
            is_recording: false,
            is_committed: false,
        }
    }
}

impl AudioSource for MockAudioSource {
    fn start_recording(&mut self) -> Result<(), VoiceError> {
        self.is_recording = true;
        self.is_committed = false;
        Ok(())
    }

    fn commit_recording(&mut self) -> Result<(), VoiceError> {
        self.is_committed = true;
        Ok(())
    }

    fn discard_recording(&mut self) -> Result<(), VoiceError> {
        self.is_recording = false;
        self.is_committed = false;
        self.samples.clear();
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<Vec<i16>, VoiceError> {
        self.is_recording = false;
        self.is_committed = false;
        Ok(self.samples.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vad::trim_silence_vad;

    #[test]
    fn test_pre_roll_preserves_speech_within_50ms_of_keydown() {
        // Speech starts at 30ms (sample 480 @ 16kHz)
        // Samples 0..480: silence
        // Samples 480..8000: active speech
        let mut audio_fixture = vec![0i16; 480];
        for i in 0..7520 {
            audio_fixture.push(((i % 40) as i16 * 100) + 1000);
        }

        let mut mock_source = MockAudioSource::new(audio_fixture.clone());

        // 1. Key-down arrives -> start pre-roll recording into ring buffer immediately
        mock_source.start_recording().unwrap();
        assert!(mock_source.is_recording);
        assert!(!mock_source.is_committed);

        // 2. 250ms threshold passes -> commit recording
        mock_source.commit_recording().unwrap();
        assert!(mock_source.is_committed);

        // 3. User finishes talking -> stop recording
        let recorded = mock_source.stop_recording().unwrap();
        assert_eq!(recorded.len(), 8000);

        // 4. VAD trims silence with 100ms leading padding:
        // Because speech began at sample 480 (30ms) and lead_padding is 100ms (1600 samples),
        // (start_idx * 320).saturating_sub(1600) evaluates to 0.
        // Therefore, sample 0 of the pre-roll is retained and speech is not clipped.
        let trimmed = trim_silence_vad(&recorded, 16000, 300.0);
        assert!(!trimmed.is_empty(), "Trimmed audio should not be empty");
        assert_eq!(
            trimmed.as_ptr(),
            recorded.as_ptr(),
            "Audio starting within 50ms of key-down must retain start without clipping"
        );
    }

    #[test]
    fn test_pre_roll_discard_on_early_release() {
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
