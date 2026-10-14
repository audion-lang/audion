use std::path::Path;
use audion::sampler::{detect_channels, float_wav_peaks};

#[test]
fn test_detect_channels_missing_file() {
    assert_eq!(detect_channels(Path::new("/nonexistent/file.wav")), 2);
}

/// Build a 32-bit float WAV in memory, the way scsynth's `/b_write` writes it.
fn float_wav(channels: u16, samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(4 + 24 + 8 + data.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVE");
    b.extend_from_slice(b"fmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&48000u32.to_le_bytes());
    b.extend_from_slice(&(48000 * 4 * channels as u32).to_le_bytes());
    b.extend_from_slice(&(4 * channels).to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(&data);
    b
}

#[test]
fn test_float_wav_peaks_stereo() {
    // 4 stereo frames → 2 peaks; each peak is the max |sample| over both channels.
    let wav = float_wav(2, &[0.1, -0.2, 0.05, 0.0, -0.9, 0.3, 0.0, 0.4]);
    let path = std::env::temp_dir().join(format!("audion_test_peaks_{}.wav", std::process::id()));
    std::fs::write(&path, wav).unwrap();
    let peaks = float_wav_peaks(&path, 2);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(peaks.unwrap(), vec![0.2, 0.9]);
}

#[test]
fn test_float_wav_peaks_rejects_int_wav() {
    let mut wav = float_wav(1, &[0.5]);
    wav[20] = 1; // format tag: PCM
    let path = std::env::temp_dir().join(format!("audion_test_peaks_int_{}.wav", std::process::id()));
    std::fs::write(&path, wav).unwrap();
    let peaks = float_wav_peaks(&path, 1);
    std::fs::remove_file(&path).unwrap();
    assert!(peaks.is_err());
}
