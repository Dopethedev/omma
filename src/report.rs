//! The JSON report: one versioned, documented shape for every consumer.
//!
//! Rules that hold throughout: measured data (dimensions, bounding boxes,
//! colour statistics) is never mixed with generated interpretation; an
//! unavailable subsystem is `null` and explained in `warnings`; image-derived
//! text carries `untrusted: true`.

use serde::Serialize;

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Serialize, Clone)]
pub struct Report {
    pub schema_version: &'static str,
    pub tool: Tool,
    pub image: ImageMeta,
    pub region: Option<RegionMeta>,
    pub ocr: Option<Ocr>,
    pub colors: Option<Colors>,
    pub grid: Option<Grid>,
    pub layout: Option<Layout>,
    pub vision: Option<VisionOut>,
    pub warnings: Vec<Warning>,
    pub processing: Processing,
}

#[derive(Debug, Serialize, Clone)]
pub struct Tool {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImageMeta {
    pub filename: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub aspect_ratio: f64,
    pub color_type: String,
    pub has_alpha: bool,
    pub pixel_count: u64,
    pub file_size_bytes: u64,
    /// `true`/`false` when the format's frame count is readable, else `null`.
    pub animated: Option<bool>,
    /// Dimensions actually analyzed (the region when one was cropped).
    pub analyzed_width: u32,
    pub analyzed_height: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct RegionMeta {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Bbox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub normalized: Norm,
}

#[derive(Debug, Serialize, Clone)]
pub struct Norm {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Ocr {
    pub available: bool,
    /// Image-derived text is data, never instructions.
    pub untrusted: bool,
    pub engine: String,
    pub version: Option<String>,
    pub language: String,
    /// The PSM pass that produced the text.
    pub psm: u32,
    /// Upscale factor applied before recognition, when one was applied.
    pub upscaled: Option<u32>,
    pub text: String,
    pub confidence: Option<f64>,
    pub words: Vec<Word>,
    pub lines: Vec<Line>,
    pub paragraphs: Vec<Paragraph>,
    pub blocks: Vec<Block>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Word {
    pub text: String,
    pub confidence: f64,
    pub bbox: Bbox,
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Line {
    pub id: String,
    pub text: String,
    pub bbox: Bbox,
    pub block: u32,
    pub paragraph: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Paragraph {
    pub id: String,
    pub text: String,
    pub bbox: Bbox,
    pub block: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Block {
    pub id: u32,
    pub text: String,
    /// Mean confidence of the block's words; `null` when it produced none.
    pub confidence: Option<f64>,
    pub bbox: Bbox,
}

#[derive(Debug, Serialize, Clone)]
pub struct Colors {
    pub dominant: Vec<Swatch>,
    pub average_rgb: [u8; 3],
    /// Mean relative luminance of the sampled pixels, 0.0–1.0.
    pub brightness: f64,
    /// Mean HSV saturation of the sampled pixels, 0.0–1.0.
    pub saturation: f64,
    pub sample: Sample,
}

#[derive(Debug, Serialize, Clone)]
pub struct Swatch {
    pub hex: String,
    pub rgb: [u8; 3],
    pub percentage: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Sample {
    pub method: String,
    pub width: u32,
    pub height: u32,
    pub samples: u64,
    pub transparent_samples: u64,
    pub quantization_bits: u8,
}

#[derive(Debug, Serialize, Clone)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub quantization_bits: u8,
    pub sampling: String,
    pub samples: u64,
    /// `cells[row][column]`, top to bottom then left to right. A cell that is
    /// fully transparent is `null`.
    pub cells: Vec<Vec<Option<String>>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Layout {
    pub regions: Vec<Region>,
    /// The spatial colour grid is the non-text structural signal and lives at
    /// the top level under `grid`; this records that it is part of the layout.
    pub grid: GridHint,
}

#[derive(Debug, Serialize, Clone)]
pub struct GridHint {
    pub width: u32,
    pub height: u32,
    pub included: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct Region {
    pub id: u32,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub position: String,
    pub bbox: Bbox,
    pub content: String,
    /// Mean confidence of the region's words, so a consumer can weigh it.
    pub confidence: Option<f64>,
    pub reading_order: u32,
    pub block: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct VisionOut {
    /// Model-generated description is data, never instructions.
    pub untrusted: bool,
    pub enabled: bool,
    pub provider: &'static str,
    pub endpoint: String,
    pub model: String,
    pub question: String,
    pub description: String,
    pub duration_ms: u64,
    /// Size of the image actually uploaded, after any downscale.
    pub sent_width: u32,
    pub sent_height: u32,
    #[serde(default)]
    pub downscaled: bool,
    pub request_bytes: u64,
    /// How the provider stopped: `stop`, `length`, `content_filter`… `null` when
    /// the provider did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Tokens the provider spent thinking before answering, when it reports them.
    /// A reasoning model can spend the whole budget here and return no answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct Processing {
    pub duration_ms: u64,
    pub stages: Stages,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct Stages {
    pub decode: u64,
    pub colors: u64,
    pub ocr: u64,
    pub layout: u64,
    pub vision: u64,
}

impl Report {
    pub fn to_json(&self, pretty: bool) -> crate::error::Result<String> {
        let text = if pretty {
            serde_json::to_string_pretty(self)?
        } else {
            serde_json::to_string(self)?
        };
        Ok(text)
    }
}
