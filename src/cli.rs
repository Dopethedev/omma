//! Command-line surface: the documented `omma IMAGE` form and the `analyze`
//! subcommand are the same analysis with the same flags.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::{Config, OutputFormat};
use crate::error::{Error, Result};

pub const DEFAULT_QUESTION: &str = "Describe this image.";

const EXIT_HELP: &str = "\
Exit codes:
  0  success, including partial results that carry warnings
  1  input, decoding or filesystem failure
  2  invalid arguments or configuration
  3  a requested processing backend failed
  4  output or serialization failure
  5  MCP server startup failure";

#[derive(Debug, Parser)]
#[command(
    name = "omma",
    version,
    about = "Turn an image into structured text an AI agent can reason about",
    after_help = EXIT_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub analyze: AnalyzeArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Analyze an image (same as passing an image directly).
    Analyze(AnalyzeArgs),
    /// Report versions, backends, configuration and limits.
    Doctor(DoctorArgs),
    /// Model Context Protocol server.
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum McpAction {
    /// Serve MCP over stdio.
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Configuration file to use.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Configuration file to use.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
pub struct AnalyzeArgs {
    /// Image file to analyze, or `-` to read from stdin.
    #[arg(value_name = "IMAGE")]
    pub image: Option<String>,

    /// Emit JSON (alias for --format json).
    #[arg(long)]
    pub json: bool,

    /// Output format: text or json.
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// Pretty-print JSON.
    #[arg(long)]
    pub pretty: bool,

    /// Analyze only this crop: X,Y,W,H in source-image pixels.
    #[arg(long, value_name = "X,Y,W,H")]
    pub region: Option<String>,

    /// Ask a local vision model one question about the image.
    #[arg(long, value_name = "QUESTION")]
    pub ask: Option<String>,

    /// Run vision analysis with the default question.
    #[arg(long)]
    pub vision: bool,

    /// Vision model name to use.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// OCR language(s), repeatable or comma separated (default: eng).
    #[arg(long, value_name = "LANG", value_delimiter = ',')]
    pub language: Vec<String>,

    /// Disable OCR.
    #[arg(long)]
    pub no_ocr: bool,

    /// Run OCR only (plus image metadata).
    #[arg(long)]
    pub ocr: bool,

    /// Run colour analysis only (plus image metadata).
    #[arg(long)]
    pub colors: bool,

    /// Run layout analysis only (plus image metadata).
    #[arg(long)]
    pub layout: bool,

    /// Write the result to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Number of dominant colours to report (1-64).
    #[arg(long, value_name = "N")]
    pub max_colors: Option<usize>,

    /// Configuration file to use.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub ocr: bool,
    pub colors: bool,
    pub layout: bool,
    /// True when the caller named the subsystems explicitly.
    pub explicit: bool,
}

impl AnalyzeArgs {
    pub fn format(&self, default: OutputFormat) -> Result<OutputFormat> {
        let named = match &self.format {
            Some(raw) => Some(OutputFormat::parse(raw).ok_or_else(|| {
                Error::Usage(format!("unknown --format {raw:?}; expected text or json"))
            })?),
            None => None,
        };
        if self.json {
            if named == Some(OutputFormat::Text) {
                return Err(Error::Usage(
                    "--json and --format text contradict each other".to_string(),
                ));
            }
            return Ok(OutputFormat::Json);
        }
        Ok(named.unwrap_or(default))
    }

    pub fn selection(&self, config: &Config) -> Result<Selection> {
        if self.no_ocr && self.ocr {
            return Err(Error::Usage(
                "--ocr and --no-ocr contradict each other".to_string(),
            ));
        }
        let explicit = self.ocr || self.colors || self.layout;
        let (ocr, colors, layout) = if explicit {
            (self.ocr && !self.no_ocr, self.colors, self.layout)
        } else {
            (
                config.ocr.enabled && !self.no_ocr,
                config.colors.enabled,
                config.layout.enabled,
            )
        };
        Ok(Selection {
            ocr,
            colors,
            layout,
            explicit,
        })
    }

    /// `None` means "do not run vision".
    pub fn question(&self) -> Option<String> {
        if let Some(ask) = &self.ask {
            return Some(ask.clone());
        }
        if self.vision {
            return Some(DEFAULT_QUESTION.to_string());
        }
        None
    }

    pub fn languages(&self) -> Option<Vec<String>> {
        if self.language.is_empty() {
            return None;
        }
        let mut languages = Vec::new();
        for raw in &self.language {
            languages.extend(crate::config::split_languages(raw));
        }
        (!languages.is_empty()).then_some(languages)
    }
}
