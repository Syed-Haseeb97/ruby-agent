//! Energy-based Voice Activity Detection (VAD).
//! Trims leading and trailing silence to minimize payload size and API latency.

/// Computes Root Mean Square (RMS) energy of a frame of 16-bit PCM samples.
pub fn frame_rms(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / frame.len() as f64).sqrt() as f32
}

/// Trims leading and trailing silence from 16 kHz mono audio samples.
/// Uses 20 ms frames (320 samples at 16 kHz), with 100 ms leading padding and
/// 160 ms trailing padding to prevent clipping consonants or phonemes.
pub fn trim_silence_vad(samples: &[i16], sample_rate: u32, threshold_rms: f32) -> &[i16] {
    let frame_size = (sample_rate as usize * 20) / 1000; // 20 ms frame (320 samples @ 16kHz)
    if samples.len() < frame_size {
        return samples;
    }

    let lead_padding = (sample_rate as usize * 100) / 1000; // 100 ms
    let tail_padding = (sample_rate as usize * 160) / 1000; // 160 ms

    let mut first_speech_frame = None;
    let mut last_speech_frame = None;

    let total_frames = samples.len() / frame_size;
    for i in 0..total_frames {
        let frame = &samples[i * frame_size..(i + 1) * frame_size];
        let rms = frame_rms(frame);
        if rms >= threshold_rms {
            if first_speech_frame.is_none() {
                first_speech_frame = Some(i);
            }
            last_speech_frame = Some(i);
        }
    }

    match (first_speech_frame, last_speech_frame) {
        (Some(start_idx), Some(end_idx)) => {
            let start_sample = (start_idx * frame_size).saturating_sub(lead_padding);
            let end_sample = ((end_idx + 1) * frame_size + tail_padding).min(samples.len());
            &samples[start_sample..end_sample]
        }
        _ => {
            // Entire clip is below threshold (pure silence/noise)
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_silence_vad_preserves_speech_with_padding() {
        let sample_rate = 16000;
        let _frame_size = 320;
        // 500ms silence (8000 samples) + 1000ms speech (16000 samples) + 500ms silence (8000 samples)
        let mut samples = vec![0i16; 8000];
        // Speech with amplitude 2000
        for i in 0..16000 {
            samples.push(((i % 50) as i16) * 100);
        }
        samples.extend(vec![0i16; 8000]);

        let trimmed = trim_silence_vad(&samples, sample_rate, 500.0);
        assert!(!trimmed.is_empty());
        assert!(trimmed.len() < samples.len());
        // Trimmed size should be roughly 1000ms + ~260ms padding = ~1260ms = ~20160 samples
        assert!(trimmed.len() >= 16000);
        assert!(trimmed.len() <= 22000);
    }

    #[test]
    fn test_pure_silence_returns_empty() {
        let samples = vec![5i16; 16000];
        let trimmed = trim_silence_vad(&samples, 16000, 300.0);
        assert!(trimmed.is_empty());
    }
}
