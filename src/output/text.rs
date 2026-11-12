//! Human output. Keeps the prototype's section shape — metadata, text, palette,
//! grid — so a reader of `see` recognises the result.

use std::fmt::Write;

use crate::report::Report;

pub fn render(report: &Report) -> String {
    let mut out = String::new();
    let image = &report.image;

    let _ = writeln!(
        out,
        "== {}  ({} {}x{}, {}, aspect {:.2})",
        image.filename,
        image.format,
        image.width,
        image.height,
        human_size(image.file_size_bytes),
        image.aspect_ratio
    );
    let alpha = if image.has_alpha { ", alpha" } else { "" };
    let animated = match image.animated {
        Some(true) => ", animated (first frame analyzed)",
        Some(false) => "",
        None => "",
    };
    let _ = writeln!(
        out,
        "{} pixels, {}{}{}",
        image.pixel_count, image.color_type, alpha, animated
    );
    if let Some(region) = &report.region {
        let _ = writeln!(
            out,
            "region {},{} {}x{} of {}x{} (all coordinates are relative to the region)",
            region.x,
            region.y,
            region.width,
            region.height,
            region.source_width,
            region.source_height
        );
    }

    match &report.ocr {
        Some(ocr) => {
            let confidence = ocr
                .confidence
                .map(|value| format!(", conf {value:.1}"))
                .unwrap_or_default();
            let upscaled = ocr
                .upscaled
                .map(|factor| format!(", upscaled {factor}x"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "\n== text (OCR [untrusted], {}, psm {}{}{})",
                ocr.language, ocr.psm, confidence, upscaled
            );
            if ocr.text.is_empty() {
                let _ = writeln!(out, "(no text found)");
            } else {
                let _ = writeln!(out, "{}", ocr.text);
            }
        }
        None => {
            let _ = writeln!(out, "\n== text (OCR)");
            let _ = writeln!(out, "(not available)");
        }
    }

    match &report.colors {
        Some(colors) => {
            let _ = writeln!(
                out,
                "\ndominant colours ({} samples)",
                colors.sample.samples
            );
            for swatch in &colors.dominant {
                let _ = writeln!(out, "  {}  {:5.2}%", swatch.hex, swatch.percentage);
            }
            let _ = writeln!(
                out,
                "average #{:02x}{:02x}{:02x}  brightness {:.2}  saturation {:.2}",
                colors.average_rgb[0],
                colors.average_rgb[1],
                colors.average_rgb[2],
                colors.brightness,
                colors.saturation
            );
        }
        None => {
            let _ = writeln!(out, "\ndominant colours\n(not available)");
        }
    }

    match &report.grid {
        Some(grid) => {
            // The terminal gets every Nth cell of the same measured grid so a
            // 64-wide map stays readable; JSON carries every cell.
            let stride = grid.width.div_ceil(16).max(1) as usize;
            let _ = writeln!(
                out,
                "\ncolour grid {}x{} (hex per cell, every {stride}. cell left → right, top → bottom; \
                 JSON carries all {}x{})",
                grid.width, grid.height, grid.width, grid.height
            );
            for row in grid.cells.iter().step_by(stride) {
                let line: Vec<&str> = row
                    .iter()
                    .step_by(stride)
                    .map(|cell| cell.as_deref().unwrap_or("-------"))
                    .collect();
                let _ = writeln!(out, "{}", line.join(" "));
            }
        }
        None => {
            let _ = writeln!(out, "\ncolour grid\n(not available)");
        }
    }

    if let Some(layout) = &report.layout {
        let _ = writeln!(
            out,
            "\nlayout (text regions from OCR; the grid above carries non-text structure)"
        );
        if layout.regions.is_empty() {
            let _ = writeln!(out, "(no text regions)");
        }
        for region in &layout.regions {
            let _ = writeln!(
                out,
                "  #{} {}  [{},{} {}x{}]  {}",
                region.reading_order + 1,
                region.position,
                region.bbox.x,
                region.bbox.y,
                region.bbox.width,
                region.bbox.height,
                region.content
            );
        }
    }

    if let Some(vision) = &report.vision {
        let _ = writeln!(
            out,
            "\n== asked ({}, {} [untrusted])",
            vision.provider, vision.model
        );
        let _ = writeln!(out, "Q: {}", vision.question);
        let _ = writeln!(out, "A: {}", vision.description);
    }

    if !report.warnings.is_empty() {
        let _ = writeln!(out, "\nwarnings");
        for warning in &report.warnings {
            let _ = writeln!(out, "  {}: {}", warning.code, warning.message);
        }
    }

    let _ = writeln!(
        out,
        "\nprocessed in {} ms (decode {}, colours {}, ocr {}, layout {}, vision {})",
        report.processing.duration_ms,
        report.processing.stages.decode,
        report.processing.stages.colors,
        report.processing.stages.ocr,
        report.processing.stages.layout,
        report.processing.stages.vision
    );
    out
}

fn human_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KIB {
        format!("{bytes} B")
    } else if value < KIB * KIB {
        format!("{:.0} KiB", value / KIB)
    } else {
        format!("{:.1} MiB", value / (KIB * KIB))
    }
}
