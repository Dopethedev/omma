//! One error type for the whole tool, and the stable exit-code contract.
//!
//! Optional backends (OCR, vision) usually degrade to a warning instead of an
//! error; `Error::Ocr` / `Error::Vision` are reserved for the case where the
//! caller asked for exactly that backend and got nothing from it.

use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no such file: {0}")]
    NotFound(PathBuf),

    #[error("permission denied: {0}")]
    Permission(PathBuf),

    #[error("not a regular file: {0}")]
    NotRegularFile(PathBuf),

    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("input is empty")]
    EmptyInput,

    #[error("input too large: {size} bytes exceeds the {limit} byte limit")]
    TooLarge { size: u64, limit: u64 },

    #[error("unsupported image format: {0}")]
    UnsupportedFormat(String),

    #[error(
        "image too large to analyze: {width}x{height} exceeds the configured limits \
         ({max_pixels} pixels, {max_dimension} px per side)"
    )]
    Dimensions {
        width: u32,
        height: u32,
        max_pixels: u64,
        max_dimension: u32,
    },

    #[error("cannot decode image: {0}")]
    Decode(String),

    #[error("invalid region {region:?}: {reason}")]
    Region { region: String, reason: String },

    #[error("invalid configuration in {path}: {message}")]
    Config { path: String, message: String },

    #[error("{0}")]
    Usage(String),

    #[error("ocr backend failed: {0}")]
    Ocr(String),

    #[error("vision backend failed: {0}")]
    Vision(String),

    #[error("cannot write output {path}: {source}")]
    Output {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("serialization failed: {0}")]
    Serialize(String),

    #[error("mcp server failed: {0}")]
    Mcp(String),
}

impl Error {
    /// Stable exit-code contract, documented in `docs/cli.md`.
    ///
    /// 0 success (including partial results with warnings) · 1 input, decoding
    /// or filesystem failure · 2 invalid arguments or configuration · 3
    /// requested processing backend failure · 4 output or serialization
    /// failure · 5 MCP server startup failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Usage(_) | Error::Config { .. } | Error::Region { .. } => 2,
            Error::Ocr(_) | Error::Vision(_) => 3,
            Error::Output { .. } | Error::Serialize(_) => 4,
            Error::Mcp(_) => 5,
            Error::NotFound(_)
            | Error::Permission(_)
            | Error::NotRegularFile(_)
            | Error::Io { .. }
            | Error::EmptyInput
            | Error::TooLarge { .. }
            | Error::UnsupportedFormat(_)
            | Error::Dimensions { .. }
            | Error::Decode(_) => 1,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialize(err.to_string())
    }
}
