use std::path::Path;
use audion::sampler::detect_channels;

#[test]
fn test_detect_channels_missing_file() {
    assert_eq!(detect_channels(Path::new("/nonexistent/file.wav")), 2);
}
