//! The engine: one analysis path shared by the CLI and the MCP server.

pub mod colors;
pub mod layout;
pub mod ocr;

use std::time::Instant;

use crate::config::Config;
use crate::error::Result;
use crate::input;
use crate::report::{ImageMeta, Processing, Report, SCHEMA_VERSION, Stages, Tool, Warning};
use crate::vision;

#[derive(Debug, Clone)]
pub struct Options {
    pub colors: bool,
    pub ocr: bool,
    pub layout: bool,
    pub ask: Option<String>,
    pub region: Option<String>,
    /// Directories a path must resolve inside; empty means no restriction.
    pub allowed_directories: Vec<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            colors: true,
            ocr: true,
            layout: true,
            ask: None,
            region: None,
            allowed_directories: Vec::new(),
        }
    }
}

pub fn run(source: &str, config: &Config, options: &Options) -> Result<Report> {
    let started = Instant::now();
    let mut warnings = Vec::new();

    if !options.allowed_directories.is_empty() {
        crate::input::enforce_allowed_roots(source, &options.allowed_directories)?;
    }

    let bytes = input::read_bytes(source, config.max_file_size_bytes())?;
    let (bytes, file_size) = bytes;

    let decode_started = Instant::now();
    let mut decoded = input::decode(source, bytes, file_size, config)?;
    if let Some(spec) = &options.region {
        let (image, region) = input::crop(decoded.image, spec)?;
        decoded.image = image;
        decoded.region = Some(region);
    }
    let decode_ms = decode_started.elapsed().as_millis() as u64;

    let mut stages = Stages {
        decode: decode_ms,
        ..Stages::default()
    };

    let width = decoded.width();
    let height = decoded.height();
    let image_meta = ImageMeta {
        filename: decoded.filename.clone(),
        format: decoded.format_name().to_string(),
        width: decoded.source_width,
        height: decoded.source_height,
        aspect_ratio: round4(
            f64::from(decoded.source_width) / f64::from(decoded.source_height.max(1)),
        ),
        color_type: color_type_name(decoded.image.color()),
        has_alpha: decoded.image.color().has_alpha(),
        pixel_count: u64::from(decoded.source_width) * u64::from(decoded.source_height),
        file_size_bytes: decoded.file_size,
        animated: decoded.animated,
        analyzed_width: width,
        analyzed_height: height,
    };

    let colors_out = if options.colors {
        let started = Instant::now();
        let result = colors::analyze(&decoded.image, config);
        stages.colors = started.elapsed().as_millis() as u64;
        Some(result)
    } else {
        None
    };

    let ocr_out = if options.ocr {
        let started = Instant::now();
        let outcome = ocr::analyze(&decoded.image, config);
        stages.ocr = started.elapsed().as_millis() as u64;
        warnings.extend(outcome.warnings);
        outcome.ocr
    } else {
        None
    };

    let layout_out = if options.layout {
        let started = Instant::now();
        let outcome = layout::analyze(ocr_out.as_ref(), config);
        stages.layout = started.elapsed().as_millis() as u64;
        if let Some(warning) = outcome.warning {
            warnings.push(warning);
        }
        if ocr_out.is_none() {
            warnings.push(Warning {
                code: "layout_no_text_regions",
                message: "layout regions come from OCR; none were produced".to_string(),
            });
        }
        Some(outcome.layout)
    } else {
        None
    };

    let vision_out = match &options.ask {
        Some(question) => {
            let started = Instant::now();
            let png = decoded.png_bytes()?;
            let result = vision::ask(&png, question, &config.vision);
            stages.vision = started.elapsed().as_millis() as u64;
            match result {
                Ok(mut out) => {
                    out.duration_ms = started.elapsed().as_millis() as u64;
                    Some(out)
                }
                Err(err) => {
                    warnings.push(Warning {
                        code: "vision_failed",
                        message: err.to_string(),
                    });
                    None
                }
            }
        }
        None => None,
    };

    let (colors_out, grid_out) = match colors_out {
        Some(result) => (Some(result.colors), Some(result.grid)),
        None => (None, None),
    };

    Ok(Report {
        schema_version: SCHEMA_VERSION,
        tool: Tool {
            name: "omma",
            version: env!("CARGO_PKG_VERSION"),
        },
        image: image_meta,
        region: decoded.region.clone(),
        ocr: ocr_out,
        colors: colors_out,
        grid: grid_out,
        layout: layout_out,
        vision: vision_out,
        warnings,
        processing: Processing {
            duration_ms: started.elapsed().as_millis() as u64,
            stages,
        },
    })
}

fn color_type_name(color: image::ColorType) -> String {
    use image::ColorType;
    match color {
        ColorType::L8 => "l8",
        ColorType::La8 => "la8",
        ColorType::Rgb8 => "rgb8",
        ColorType::Rgba8 => "rgba8",
        ColorType::L16 => "l16",
        ColorType::La16 => "la16",
        ColorType::Rgb16 => "rgb16",
        ColorType::Rgba16 => "rgba16",
        ColorType::Rgb32F => "rgb32f",
        ColorType::Rgba32F => "rgba32f",
        other => {
            return format!("{other:?}").to_lowercase();
        }
    }
    .to_string()
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
