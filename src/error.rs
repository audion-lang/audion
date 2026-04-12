// Copyright (C) 2025-2026 Aleksandr Bogdanov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//

use std::fmt;

#[derive(Debug)]
pub enum AudionError {
    LexError { msg: String, line: usize },
    ParseError { msg: String, line: usize },
    RuntimeError { msg: String },
    RuntimeErrorAt { msg: String, file: String, line: usize },
}

impl AudionError {
    /// Attach a source location to a RuntimeError. Other variants pass through unchanged.
    pub fn at_line(self, line: usize, file: &str) -> Self {
        match self {
            AudionError::RuntimeError { msg } => AudionError::RuntimeErrorAt { msg, line, file: file.to_string() },
            other => other,
        }
    }
}

impl fmt::Display for AudionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudionError::LexError { msg, line } => {
                write!(f, "error[line {}]: {}", line, msg)
            }
            AudionError::ParseError { msg, line } => {
                write!(f, "error[line {}]: {}", line, msg)
            }
            AudionError::RuntimeError { msg } => {
                write!(f, "runtime error: {}", msg)
            }
            AudionError::RuntimeErrorAt { msg, file, line } => {
                write!(f, "runtime error[{}:{}]: {}", file, line, msg)
            }
        }
    }
}

impl std::error::Error for AudionError {}

pub type Result<T> = std::result::Result<T, AudionError>;
