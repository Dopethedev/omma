//! `omma doctor`: what is available, what is configured, what the limits are.
//! Never prints secrets: the vision API key is reported as present or absent.

use std::path::Path;

use crate::analysis::ocr;
use crate::config::Config;

pub fn render(config: &Config, config_path: Option<&Path>) -> String {
    let mut out = String::new();
    out.push_str(&format!("omma {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "build: {} {}, rust edition 2024, msrv {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_RUST_VERSION")
    ));

    out.push_str(&format!(
        "config: {}\n",
        match config_path {
            Some(path) => path.display().to_string(),
            None => format!(
                "{} (not found; defaults in use)",
                crate::config::default_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(
                        || "unresolved: neither XDG_CONFIG_HOME nor HOME is set".to_string()
                    )
            ),
        }
    ));

    match ocr::backend() {
        Some(backend) => {
            out.push_str(&format!(
                "ocr: tesseract {} at {}; languages {}\n",
                backend.version.as_deref().unwrap_or("(version unknown)"),
                backend.path.display(),
                if backend.languages.is_empty() {
                    "none".to_string()
                } else {
                    backend.languages.join(", ")
                }
            ));
        }
        None => out.push_str("ocr: tesseract not found (OCR will be skipped)\n"),
    }
    out.push_str(&format!(
        "ocr configured: enabled={} languages={} timeout={}s upscale_small_crops={} (<{}px, {}x)\n",
        config.ocr.enabled,
        config.ocr.languages.join("+"),
        config.ocr.timeout_seconds,
        config.ocr.upscale_small_crops,
        config.ocr.small_crop_height,
        config.ocr.upscale_factor
    ));

    let vision_state = if config.vision.enabled {
        "enabled"
    } else {
        "disabled"
    };
    out.push_str(&format!(
        "vision: {} endpoint={} model={} timeout={}s max_tokens={} api_key={} remote={} effort={}\n",
        vision_state,
        config.vision.endpoint,
        if config.vision.model.is_empty() {
            "(none)"
        } else {
            config.vision.model.as_str()
        },
        config.vision.timeout_seconds,
        config.vision.max_tokens,
        // Presence and origin only: the value is a credential and is never read
        // here, let alone printed.
        config
            .vision
            .api_key_source()
            .unwrap_or_else(|| "none".to_string()),
        if config.vision.allow_remote {
            "allowed (https only)"
        } else {
            "loopback only"
        },
        config
            .vision
            .reasoning_effort
            .as_deref()
            .unwrap_or("default")
    ));
    if config.vision.enabled {
        match crate::vision::check_endpoint(&config.vision) {
            Ok(()) => out.push_str("vision endpoint: accepted\n"),
            Err(err) => out.push_str(&format!("vision endpoint: rejected: {err}\n")),
        }
    }

    out.push_str(&format!(
        "limits: file {} MB, pixels {}, dimension {} px, decode allocation {} MB\n",
        config.general.max_file_size_mb,
        config.general.max_pixels,
        config.general.max_dimension,
        config.general.max_alloc_mb
    ));
    out.push_str(&format!(
        "colours: enabled={} max_colors={} grid={}x{} quantization={} bits\n",
        config.colors.enabled,
        config.colors.max_colors,
        config.colors.grid_width,
        config.colors.grid_height,
        config.colors.quantization_bits
    ));
    out.push_str(&format!(
        "layout: enabled={}\nmcp: stdio transport, allowed directories: {}\n",
        config.layout.enabled,
        if config.mcp.allowed_directories.is_empty() {
            "(any path the server can read — see docs/mcp.md)".to_string()
        } else {
            config
                .mcp
                .allowed_directories
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));
    out.push_str(&format!("logging: level={}\n", config.logging.level));
    out
}
