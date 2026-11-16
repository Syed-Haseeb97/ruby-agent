//! 16 kHz Mono 16-bit PCM WAV Encoder for Gemini multimodal audio payloads.

/// Encodes raw 16-bit signed PCM samples at 16,000 Hz mono into standard RIFF WAVE bytes.
pub fn encode_wav_16k_mono(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let file_len = 36 + data_len;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);

    // 1. RIFF Chunk Descriptor
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&file_len.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");

    // 2. "fmt " Sub-chunk
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size = 16 for PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = 1 (linear PCM)
    bytes.extend_from_slice(&1u16.to_le_bytes()); // NumChannels = 1 (Mono)
    bytes.extend_from_slice(&16000u32.to_le_bytes()); // SampleRate = 16000
    bytes.extend_from_slice(&(16000u32 * 2).to_le_bytes()); // ByteRate = 32000 B/s
    bytes.extend_from_slice(&2u16.to_le_bytes()); // BlockAlign = 2 bytes (16-bit mono)
    bytes.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample = 16

    // 3. "data" Sub-chunk
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for &sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_wav_valid_riff_header() {
        let samples = vec![0i16, 100, -100, 300];
        let wav = encode_wav_16k_mono(&samples);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        let data_size = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(data_size, 8); // 4 samples * 2 bytes
        assert_eq!(wav.len(), 44 + 8);
    }
}
