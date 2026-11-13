//! Configuration: built-in defaults < config file < environment < CLI.
//!
//! Config file location: `--config PATH`, else `$OMMA_CONFIG`, else
//! `$XDG_CONFIG_HOME/omma/config.toml`, else `$HOME/.config/omma/config.toml`.
//! If `$HOME` cannot be resolved and no explicit path was given, omitted
//! configuration is reported (not fatal) and defaults are used.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434/v1/chat/completions";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub ocr: Ocr,
    pub colors: Colors,
    pub layout: Layout,
    pub vision: Vision,
    pub mcp: Mcp,
    pub logging: Logging,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub default_format: OutputFormat,
    pub max_file_size_mb: u64,
    pub max_pixels: u64,
    pub max_dimension: u32,
    pub max_alloc_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "text" => Some(OutputFormat::Text),
            "json" => Some(OutputFormat::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Ocr {
    pub enabled: bool,
    pub languages: Vec<String>,
    pub timeout_seconds: u64,
    pub upscale_small_crops: bool,
    pub small_crop_height: u32,
    pub upscale_factor: u32,
    /// Below this mean word confidence, OCR output is flagged as unreliable and
    /// blocks are not promoted to layout regions. Measurements are never
    /// discarded — only flagged.
    pub min_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub enabled: bool,
    pub max_colors: usize,
    pub grid_width: u32,
    pub grid_height: u32,
    pub quantization_bits: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Vision {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub max_tokens: u32,
    pub api_key: Option<String>,
    /// A non-loopback endpoint is a remote upload; it needs this explicit yes.
    pub allow_remote: bool,
    /// Longest side of the image actually uploaded; larger images are downscaled.
    pub max_image_dimension: u32,
    /// Ceiling for the encoded PNG sent to the model.
    pub max_request_mb: u64,
}

impl Vision {
    pub fn max_request_bytes(&self) -> u64 {
        self.max_request_mb.saturating_mul(1024 * 1024)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Mcp {
    pub allowed_directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Logging {
    pub level: String,
}

impl Default for General {
    fn default() -> Self {
        General {
            default_format: OutputFormat::Text,
            max_file_size_mb: 32,
            max_pixels: 40_000_000,
            max_dimension: 16_384,
            max_alloc_mb: 256,
        }
    }
}

impl Default for Ocr {
    fn default() -> Self {
        Ocr {
            enabled: true,
            languages: vec!["eng".to_string()],
            timeout_seconds: 30,
            upscale_small_crops: true,
            small_crop_height: 120,
            upscale_factor: 4,
            min_confidence: 40.0,
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            enabled: true,
            max_colors: 6,
            grid_width: 64,
            grid_height: 36,
            quantization_bits: 5,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout { enabled: true }
    }
}

impl Default for Vision {
    fn default() -> Self {
        Vision {
            enabled: false,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            model: String::new(),
            timeout_seconds: 120,
            max_tokens: 400,
            api_key: None,
            allow_remote: false,
            max_image_dimension: 1280,
            max_request_mb: 6,
        }
    }
}

impl Default for Logging {
    fn default() -> Self {
        Logging {
            level: "warn".to_string(),
        }
    }
}

/// `$XDG_CONFIG_HOME/omma/config.toml`, else `%APPDATA%\omma\config.toml` on
/// Windows, else `$HOME/.config/omma/config.toml`.
pub fn default_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir).join("omma").join("config.toml"));
    }
    if cfg!(windows)
        && let Some(appdata) = std::env::var_os("APPDATA")
        && !appdata.is_empty()
    {
        return Some(PathBuf::from(appdata).join("omma").join("config.toml"));
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("omma")
            .join("config.toml"),
    )
}

impl Config {
    /// Load defaults, then the config file (if any), then environment overrides.
    pub fn load(explicit: Option<&Path>) -> Result<(Config, Option<PathBuf>)> {
        let env_path = std::env::var_os("OMMA_CONFIG").map(PathBuf::from);
        let path = explicit
            .map(Path::to_path_buf)
            .or(env_path)
            .or_else(default_path);

        let mut config = Config::default();
        let mut used = None;
        if let Some(path) = path {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    config = toml::from_str(&text).map_err(|err| Error::Config {
                        path: path.display().to_string(),
                        message: err.to_string(),
                    })?;
                    used = Some(path);
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    // An explicit path that does not exist is a mistake; a
                    // discovered one is simply absent.
                    if explicit.is_some() || std::env::var_os("OMMA_CONFIG").is_some() {
                        return Err(Error::Config {
                            path: path.display().to_string(),
                            message: "file does not exist".to_string(),
                        });
                    }
                }
                Err(err) => {
                    return Err(Error::Config {
                        path: path.display().to_string(),
                        message: err.to_string(),
                    });
                }
            }
        }

        config.apply_env()?;
        config.validate(used.as_deref())?;
        Ok((config, used))
    }

    fn apply_env(&mut self) -> Result<()> {
        if let Some(raw) = env("OMMA_FORMAT") {
            self.general.default_format =
                OutputFormat::parse(&raw).ok_or_else(|| Error::Config {
                    path: "OMMA_FORMAT".to_string(),
                    message: format!("expected text or json, got {raw:?}"),
                })?;
        }
        if let Some(raw) = env("OMMA_MAX_FILE_SIZE_MB") {
            self.general.max_file_size_mb = parse_env(&raw, "OMMA_MAX_FILE_SIZE_MB")?;
        }
        if let Some(raw) = env("OMMA_MAX_PIXELS") {
            self.general.max_pixels = parse_env(&raw, "OMMA_MAX_PIXELS")?;
        }
        if let Some(raw) = env("OMMA_MAX_DIMENSION") {
            self.general.max_dimension = parse_env(&raw, "OMMA_MAX_DIMENSION")?;
        }
        if let Some(raw) = env("OMMA_OCR_ENABLED") {
            self.ocr.enabled = parse_bool(&raw, "OMMA_OCR_ENABLED")?;
        }
        if let Some(raw) = env("OMMA_OCR_LANGUAGES") {
            self.ocr.languages = split_languages(&raw);
        }
        if let Some(raw) = env("OMMA_OCR_TIMEOUT_SECONDS") {
            self.ocr.timeout_seconds = parse_env(&raw, "OMMA_OCR_TIMEOUT_SECONDS")?;
        }
        if let Some(raw) = env("OMMA_OCR_MIN_CONFIDENCE") {
            self.ocr.min_confidence = parse_env(&raw, "OMMA_OCR_MIN_CONFIDENCE")?;
        }
        if let Some(raw) = env("OMMA_COLORS_ENABLED") {
            self.colors.enabled = parse_bool(&raw, "OMMA_COLORS_ENABLED")?;
        }
        if let Some(raw) = env("OMMA_MAX_COLORS") {
            self.colors.max_colors = parse_env(&raw, "OMMA_MAX_COLORS")?;
        }
        if let Some(raw) = env("OMMA_LAYOUT_ENABLED") {
            self.layout.enabled = parse_bool(&raw, "OMMA_LAYOUT_ENABLED")?;
        }
        if let Some(raw) = env("OMMA_VISION_ENABLED") {
            self.vision.enabled = parse_bool(&raw, "OMMA_VISION_ENABLED")?;
        }
        if let Some(raw) = env("OMMA_VISION_ENDPOINT") {
            self.vision.endpoint = raw;
        }
        if let Some(raw) = env("OMMA_VISION_MODEL") {
            self.vision.model = raw;
        }
        if let Some(raw) = env("OMMA_VISION_TIMEOUT_SECONDS") {
            self.vision.timeout_seconds = parse_env(&raw, "OMMA_VISION_TIMEOUT_SECONDS")?;
        }
        if let Some(raw) = env("OMMA_VISION_MAX_TOKENS") {
            self.vision.max_tokens = parse_env(&raw, "OMMA_VISION_MAX_TOKENS")?;
        }
        if let Some(raw) = env("OMMA_VISION_API_KEY") {
            self.vision.api_key = Some(raw);
        }
        if let Some(raw) = env("OMMA_VISION_ALLOW_REMOTE") {
            self.vision.allow_remote = parse_bool(&raw, "OMMA_VISION_ALLOW_REMOTE")?;
        }
        if let Some(raw) = env("OMMA_VISION_MAX_DIMENSION") {
            self.vision.max_image_dimension = parse_env(&raw, "OMMA_VISION_MAX_DIMENSION")?;
        }
        if let Some(raw) = env("OMMA_VISION_MAX_REQUEST_MB") {
            self.vision.max_request_mb = parse_env(&raw, "OMMA_VISION_MAX_REQUEST_MB")?;
        }
        if let Some(raw) = env("OMMA_LOG_LEVEL") {
            self.logging.level = raw;
        }
        Ok(())
    }

    pub fn validate(&self, path: Option<&Path>) -> Result<()> {
        let fail = |message: String| Error::Config {
            path: path
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "defaults".to_string()),
            message,
        };
        if self.general.max_file_size_mb == 0 {
            return Err(fail(
                "general.max_file_size_mb must be greater than 0".into(),
            ));
        }
        if self.general.max_pixels == 0 {
            return Err(fail("general.max_pixels must be greater than 0".into()));
        }
        if self.general.max_dimension == 0 {
            return Err(fail("general.max_dimension must be greater than 0".into()));
        }
        if self.general.max_alloc_mb == 0 {
            return Err(fail("general.max_alloc_mb must be greater than 0".into()));
        }
        if self.ocr.languages.iter().any(|lang| lang.trim().is_empty()) {
            return Err(fail("ocr.languages must not contain an empty entry".into()));
        }
        if self.ocr.timeout_seconds == 0 {
            return Err(fail("ocr.timeout_seconds must be greater than 0".into()));
        }
        if self.ocr.upscale_factor == 0 || self.ocr.upscale_factor > 16 {
            return Err(fail("ocr.upscale_factor must be between 1 and 16".into()));
        }
        if !(0.0..=100.0).contains(&self.ocr.min_confidence) {
            return Err(fail("ocr.min_confidence must be between 0 and 100".into()));
        }
        if self.colors.max_colors == 0 || self.colors.max_colors > 64 {
            return Err(fail("colors.max_colors must be between 1 and 64".into()));
        }
        if self.colors.grid_width == 0 || self.colors.grid_width > 256 {
            return Err(fail("colors.grid_width must be between 1 and 256".into()));
        }
        if self.colors.grid_height == 0 || self.colors.grid_height > 256 {
            return Err(fail("colors.grid_height must be between 1 and 256".into()));
        }
        if !(1..=8).contains(&self.colors.quantization_bits) {
            return Err(fail(
                "colors.quantization_bits must be between 1 and 8".into(),
            ));
        }
        if self.vision.timeout_seconds == 0 {
            return Err(fail("vision.timeout_seconds must be greater than 0".into()));
        }
        if self.vision.max_image_dimension == 0 || self.vision.max_image_dimension > 16384 {
            return Err(fail(
                "vision.max_image_dimension must be between 1 and 16384".into(),
            ));
        }
        if self.vision.max_request_mb == 0 {
            return Err(fail("vision.max_request_mb must be greater than 0".into()));
        }
        if self.vision.enabled && self.vision.model.trim().is_empty() {
            return Err(fail(
                "vision.model is required when vision.enabled = true".into(),
            ));
        }
        // Fail early and locally rather than after an image has been read.
        crate::vision::check_endpoint(&self.vision)?;
        Ok(())
    }

    pub fn max_file_size_bytes(&self) -> u64 {
        self.general.max_file_size_mb.saturating_mul(1024 * 1024)
    }

    pub fn max_alloc_bytes(&self) -> u64 {
        self.general.max_alloc_mb.saturating_mul(1024 * 1024)
    }

    pub fn ocr_languages(&self) -> String {
        self.ocr.languages.join("+")
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn parse_env<T: std::str::FromStr>(raw: &str, key: &str) -> Result<T> {
    raw.trim().parse().map_err(|_| Error::Config {
        path: key.to_string(),
        message: format!("cannot parse {raw:?}"),
    })
}

fn parse_bool(raw: &str, key: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(Error::Config {
            path: key.to_string(),
            message: format!("expected a boolean, got {other:?}"),
        }),
    }
}

/// `eng` or `eng,deu` or `eng+deu`.
pub fn split_languages(raw: &str) -> Vec<String> {
    raw.split([',', '+'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
