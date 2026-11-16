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
    is_recording: bool,
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
        }
    }
}

impl MockAudioSource {
    pub fn new(samples: Vec<i16>) -> Self {
        Self {
            samples,
            is_recording: false,
        }
    }
}

impl AudioSource for MockAudioSource {
    fn start_recording(&mut self) -> Result<(), VoiceError> {
        self.is_recording = true;
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<Vec<i16>, VoiceError> {
        self.is_recording = false;
        Ok(self.samples.clone())
    }
}
