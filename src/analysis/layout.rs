//! Layout: measured text regions from OCR, plus the spatial colour grid.
//!
//! This is not object detection. A region is a Tesseract text block with its
//! measured box, a position label derived from that box's centre, and its
//! reading order.
//!
//! A region is an *interpretation*, so a block has to look like text before it
//! becomes one: two or more alphanumeric characters at or above the configured
//! confidence. Photographs otherwise turn texture noise into dozens of
//! one-character "regions". Nothing is thrown away — every block stays in
//! `ocr.blocks`; the skipped ones are counted in a warning.

use crate::config::Config;
use crate::report::{GridHint, Layout, Region, Warning};

/// Nine-cell label from the box centre, in thirds of the analyzed frame.
pub fn position_label(center_x: f64, center_y: f64) -> &'static str {
    let column = if center_x < 1.0 / 3.0 {
        "left"
    } else if center_x < 2.0 / 3.0 {
        "center"
    } else {
        "right"
    };
    let row = if center_y < 1.0 / 3.0 {
        "top"
    } else if center_y < 2.0 / 3.0 {
        "middle"
    } else {
        "bottom"
    };
    match (row, column) {
        ("top", "left") => "top-left",
        ("top", "center") => "top-center",
        ("top", "right") => "top-right",
        ("middle", "left") => "middle-left",
        ("middle", "center") => "center",
        ("middle", "right") => "middle-right",
        ("bottom", "left") => "bottom-left",
        ("bottom", "center") => "bottom-center",
        _ => "bottom-right",
    }
}

/// Is this block credible enough to be called a text region?
pub fn is_text_region(text: &str, confidence: Option<f64>, min_confidence: f64) -> bool {
    let alphanumeric = text.chars().filter(|c| c.is_alphanumeric()).count();
    alphanumeric >= 2 && confidence.is_some_and(|value| value >= min_confidence)
}

pub struct Outcome {
    pub layout: Layout,
    pub warning: Option<Warning>,
}

pub fn analyze(ocr: Option<&crate::report::Ocr>, config: &Config) -> Outcome {
    let min_confidence = config.ocr.min_confidence;
    let mut regions = Vec::new();
    let mut skipped = 0usize;

    if let Some(ocr) = ocr {
        for block in &ocr.blocks {
            if !is_text_region(&block.text, block.confidence, min_confidence) {
                skipped += 1;
                continue;
            }
            let center_x = block.bbox.normalized.x + block.bbox.normalized.width / 2.0;
            let center_y = block.bbox.normalized.y + block.bbox.normalized.height / 2.0;
            regions.push(Region {
                id: block.id,
                kind: "text",
                position: position_label(center_x, center_y).to_string(),
                bbox: block.bbox.clone(),
                content: block.text.clone(),
                confidence: block.confidence,
                reading_order: regions.len() as u32,
                block: block.id,
            });
        }
    }

    let warning = (skipped > 0).then(|| Warning {
        code: "layout_low_confidence_blocks",
        message: format!(
            "{skipped} measured text block(s) were not credible enough to become regions \
             (fewer than 2 alphanumeric characters or confidence below {min_confidence}); \
             ocr.blocks still holds them"
        ),
    });

    Outcome {
        layout: Layout {
            regions,
            grid: GridHint {
                width: config.colors.grid_width,
                height: config.colors.grid_height,
                included: true,
            },
        },
        warning,
    }
}
