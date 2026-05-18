//! Base64 ↔ PCM16 audio codec helpers.

use base64::engine::general_purpose;
use base64::Engine;

/// Decode a base64 string into a `Vec<i16>` of little-endian PCM samples.
pub fn decode_base64_pcm16(b64: &str) -> Result<Vec<i16>, base64::DecodeError> {
    let bytes = general_purpose::STANDARD.decode(b64)?;
    let mut samples = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Ok(samples)
}

/// Encode a slice of PCM16 samples as a base64 string.
pub fn encode_base64_pcm16(samples: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    general_purpose::STANDARD.encode(bytes)
}

/// Encode raw PCM16 wire bytes (already little-endian) as base64.
pub fn encode_base64_pcm16_bytes(bytes: &[u8]) -> String {
    general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_pcm16() {
        let original: Vec<i16> = vec![0, 1, -1, 32_767, -32_768, 12_345];
        let encoded = encode_base64_pcm16(&original);
        let decoded = decode_base64_pcm16(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trip_pcm16_bytes() {
        let samples: Vec<i16> = vec![10, -10, 20, -20];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        let encoded = encode_base64_pcm16_bytes(&bytes);
        let decoded = decode_base64_pcm16(&encoded).unwrap();

        assert_eq!(decoded, samples);
    }
}
