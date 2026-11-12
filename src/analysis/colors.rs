//! Deterministic colour analysis: palette, statistics and the spatial grid.
//!
//! One sampling method serves both products. The Python prototype sampled a
//! 64x36 frame through ffmpeg's default (bicubic) scaler and bucketed each
//! channel by dropping the low 3 bits; this keeps the same shape of algorithm
//! with the alpha handling spelled out instead of flattened to black: a pixel
//! below the alpha threshold is absent, and a partly transparent pixel is
//! composited over white.

use image::DynamicImage;
use image::imageops::FilterType;

use crate::config::Config;
use crate::report::{Colors, Grid, Sample, Swatch};

/// The palette always reads this frame, whatever the grid dimensions are.
pub const PALETTE_SAMPLE_W: u32 = 64;
pub const PALETTE_SAMPLE_H: u32 = 36;

/// Below this alpha a pixel is treated as absent, not as black.
pub const ALPHA_THRESHOLD: u8 = 8;

pub const METHOD: &str = "bicubic (CatmullRom) downsample to the sample grid; \
     alpha below 8/255 discarded; partial alpha composited over white";

pub struct Sampled {
    /// `None` where the source pixel was transparent.
    pub cells: Vec<Option<[u8; 3]>>,
    pub dropped: u64,
}

/// Downsample to `width x height` and flatten to opaqueness-aware RGB.
pub fn sample(image: &DynamicImage, width: u32, height: u32) -> Sampled {
    let scaled = image.resize_exact(width, height, FilterType::CatmullRom);
    let rgba = scaled.to_rgba8();
    let mut cells = Vec::with_capacity((width as usize) * (height as usize));
    let mut dropped = 0u64;
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < ALPHA_THRESHOLD {
            dropped += 1;
            cells.push(None);
            continue;
        }
        if a == 255 {
            cells.push(Some([r, g, b]));
            continue;
        }
        // Composite over white so a faint pixel cannot masquerade as a dark one.
        let alpha = u32::from(a);
        let blend = |channel: u8| -> u8 {
            ((u32::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8
        };
        cells.push(Some([blend(r), blend(g), blend(b)]));
    }
    Sampled { cells, dropped }
}

/// Keep the top `bits` bits of each channel (the prototype's `(v >> 3) << 3`).
pub fn quantize(rgb: [u8; 3], bits: u8) -> [u8; 3] {
    let shift = 8u8.saturating_sub(bits);
    [
        (rgb[0] >> shift) << shift,
        (rgb[1] >> shift) << shift,
        (rgb[2] >> shift) << shift,
    ]
}

pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

pub struct ColorsResult {
    pub colors: Colors,
    pub grid: Grid,
}

pub fn analyze(image: &DynamicImage, config: &Config) -> ColorsResult {
    let bits = config.colors.quantization_bits;
    let (grid_w, grid_h) = (config.colors.grid_width, config.colors.grid_height);

    let grid_sample = sample(image, grid_w, grid_h);
    let palette_sample = if grid_w == PALETTE_SAMPLE_W && grid_h == PALETTE_SAMPLE_H {
        None
    } else {
        Some(sample(image, PALETTE_SAMPLE_W, PALETTE_SAMPLE_H))
    };
    let palette_source = palette_sample.as_ref().unwrap_or(&grid_sample);

    let opaque: Vec<[u8; 3]> = palette_source.cells.iter().flatten().copied().collect();
    let total = opaque.len() as u64;

    let mut counters: std::collections::BTreeMap<[u8; 3], u64> = std::collections::BTreeMap::new();
    for rgb in &opaque {
        *counters.entry(quantize(*rgb, bits)).or_insert(0) += 1;
    }
    let mut ranked: Vec<([u8; 3], u64)> = counters.into_iter().collect();
    // Deterministic order: most frequent first, ties by colour value.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let dominant: Vec<Swatch> = ranked
        .iter()
        .take(config.colors.max_colors)
        .map(|(rgb, count)| Swatch {
            hex: hex(*rgb),
            rgb: *rgb,
            percentage: round2(*count as f64 * 100.0 / total.max(1) as f64),
        })
        .collect();

    let (average_rgb, brightness, saturation) = statistics(&opaque);

    let colors = Colors {
        dominant,
        average_rgb,
        brightness,
        saturation,
        sample: Sample {
            method: METHOD.to_string(),
            width: PALETTE_SAMPLE_W,
            height: PALETTE_SAMPLE_H,
            samples: total,
            transparent_samples: palette_source.dropped,
            quantization_bits: bits,
        },
    };

    let mut rows = Vec::with_capacity(grid_h as usize);
    for row in 0..grid_h {
        let mut line = Vec::with_capacity(grid_w as usize);
        for column in 0..grid_w {
            let index = (row as usize) * (grid_w as usize) + column as usize;
            line.push(grid_sample.cells[index].map(hex));
        }
        rows.push(line);
    }
    let grid = Grid {
        width: grid_w,
        height: grid_h,
        quantization_bits: bits,
        sampling: METHOD.to_string(),
        samples: u64::from(grid_w) * u64::from(grid_h),
        cells: rows,
    };

    ColorsResult { colors, grid }
}

fn statistics(sampled: &[[u8; 3]]) -> ([u8; 3], f64, f64) {
    if sampled.is_empty() {
        return ([0, 0, 0], 0.0, 0.0);
    }
    let count = sampled.len() as f64;
    let mut sum = [0f64; 3];
    let mut luma = 0f64;
    let mut sat = 0f64;
    for [r, g, b] in sampled {
        let (rf, gf, bf) = (f64::from(*r), f64::from(*g), f64::from(*b));
        sum[0] += rf;
        sum[1] += gf;
        sum[2] += bf;
        luma += (0.2126 * rf + 0.7152 * gf + 0.0722 * bf) / 255.0;
        let max = rf.max(gf).max(bf);
        let min = rf.min(gf).min(bf);
        sat += if max > 0.0 { (max - min) / max } else { 0.0 };
    }
    let average = [
        (sum[0] / count).round() as u8,
        (sum[1] / count).round() as u8,
        (sum[2] / count).round() as u8,
    ];
    (average, round4(luma / count), round4(sat / count))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
