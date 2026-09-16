// Adapted from vibelang-dsp (https://github.com/trusch/vibelang), MIT OR Apache-2.0.
// Trimmed to the variants audion's graph builder + encoder actually raise.

use std::fmt;

#[derive(Debug)]
pub enum SynthDefError {
    NoActiveBuilder,
    EncodingError(String),
    ValidationError(String),
}

impl fmt::Display for SynthDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynthDefError::NoActiveBuilder => write!(f, "no active graph builder in scope"),
            SynthDefError::EncodingError(s) => write!(f, "encoding error: {}", s),
            SynthDefError::ValidationError(s) => write!(f, "validation error: {}", s),
        }
    }
}

impl std::error::Error for SynthDefError {}

pub type Result<T> = std::result::Result<T, SynthDefError>;
