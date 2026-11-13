//! OMMA — ancient ὄμμα, "eye": turns an image into structured text an AI agent
//! can reason about, locally, with measured data separated from interpretation.

pub mod analysis;
pub mod base64;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod input;
pub mod mcp;
pub mod output;
pub mod report;
pub mod vision;

/// Logging always goes to stderr: stdout belongs to the result (or, in MCP mode,
/// to the protocol).
pub fn init_logging(level: &str) {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_new(level)
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .try_init();
    });
}
