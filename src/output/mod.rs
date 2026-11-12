//! Output rendering: one report, two presentations.

pub mod json;
pub mod text;

use crate::config::OutputFormat;
use crate::error::Result;
use crate::report::Report;

pub fn render(report: &Report, format: OutputFormat, pretty: bool) -> Result<String> {
    match format {
        OutputFormat::Json => json::render(report, pretty),
        OutputFormat::Text => Ok(text::render(report)),
    }
}
