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

/// Deterministic noise in -1..1 (tiny LCG), so tests don't need a rand crate.
fn noise(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// `secs` of near-silence with a short decaying noise hit at each time in `hits`.
fn hits_track(sr: f64, secs: f64, hits: &[f64]) -> Vec<f32> {
    let mut seed = 7;
    let mut x: Vec<f32> = (0..(sr * secs) as usize).map(|_| noise(&mut seed) * 0.001).collect();
    for &h in hits {
        let start = (h * sr) as usize;
        for i in 0..(0.08 * sr) as usize {
            if start + i < x.len() {
                x[start + i] += noise(&mut seed) * 0.8 * (-(i as f32) / (0.015 * sr as f32)).exp();
            }
        }
    }
    x
}

#[test]
fn test_detect_beats_steady_hits() {
    use audion::sampler::detect_beats;
    let sr = 44100.0;
    // 4 s, a hit every 0.5 s → 8 beats at 120 bpm
    let hits: Vec<f64> = (0..8).map(|i| i as f64 * 0.5).collect();
    let b = detect_beats(&hits_track(sr, 4.0, &hits), sr);
    assert_eq!(b.beats, 8, "{:?}", b);
    assert!((b.bpm - 120.0).abs() < 0.01, "{:?}", b);
    assert_eq!(b.onsets.len(), 8, "{:?}", b.onsets);
    for (o, h) in b.onsets.iter().zip(&hits) {
        let err_ms = (*o as f64 / sr - h).abs() * 1000.0;
        assert!(err_ms < 5.0, "onset {} vs hit {} s: {} ms off", o, h, err_ms);
    }
}

#[test]
fn test_detect_beats_offbeat_pattern() {
    use audion::sampler::detect_beats;
    let sr = 44100.0;
    // 2 bars at 100 bpm (beat 0.6 s, 4.8 s), hits on beats 1, 2, 2.5, 3, 4 of each bar
    let mut hits = Vec::new();
    for bar in 0..2 {
        for b in [0.0, 1.0, 1.5, 2.0, 3.0] {
            hits.push((bar as f64 * 4.0 + b) * 0.6);
        }
    }
    let b = detect_beats(&hits_track(sr, 4.8, &hits), sr);
    assert_eq!(b.beats, 8, "{:?}", b);
    assert_eq!(b.onsets.len(), hits.len());
}

#[test]
fn test_detect_beats_not_beaty() {
    use audion::sampler::detect_beats;
    let sr = 44100.0;
    // a steady noise drone: no hits to speak of
    let mut seed = 3;
    let x: Vec<f32> = (0..(sr * 3.0) as usize).map(|_| noise(&mut seed) * 0.3).collect();
    let b = detect_beats(&x, sr);
    assert_eq!(b.beats, 0, "{:?}", b);
}
