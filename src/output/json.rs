//! JSON is a primary product, not a debug format: versioned, stable, and the
//! only thing written to stdout in JSON mode.

use crate::error::Result;
use crate::report::Report;

pub fn render(report: &Report, pretty: bool) -> Result<String> {
    report.to_json(pretty)
}
