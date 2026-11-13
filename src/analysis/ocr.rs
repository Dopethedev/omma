//! Tesseract OCR through a safely invoked subprocess, fed over stdin.
//!
//! The prototype's recognition shape is preserved: small crops are upscaled and
//! greyscaled first, two page-segmentation passes run, and the longer result
//! wins. Structure comes from the winning pass's TSV output, using Tesseract's
//! own level/block/paragraph/line identifiers rather than re-deriving grouping.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

use crate::config::Config;
use crate::report::{Bbox, Block, Line, Norm, Ocr, Paragraph, Warning, Word};

/// The two passes the prototype compared.
pub const PSM_PASSES: [u32; 2] = [6, 11];

pub const ENGINE: &str = "tesseract";

/// Refuse to feed Tesseract an upscaled width beyond this; a 4x crop of a wide
/// strip can otherwise become an absurd bitmap.
const MAX_OCR_WIDTH: u32 = 16_384;

pub struct Backend {
    pub path: PathBuf,
    pub version: Option<String>,
    pub languages: Vec<String>,
}

pub fn backend() -> Option<Backend> {
    let path = which("tesseract")?;
    let version = version_of(&path);
    let languages = languages_of(&path);
    Some(Backend {
        path,
        version,
        languages,
    })
}

pub struct Outcome {
    pub ocr: Option<Ocr>,
    pub warnings: Vec<Warning>,
}

fn warn(code: &'static str, message: String) -> Warning {
    Warning { code, message }
}

pub fn analyze(image: &DynamicImage, config: &Config) -> Outcome {
    let Some(backend) = backend() else {
        return Outcome {
            ocr: None,
            warnings: vec![warn(
                "ocr_unavailable",
                "tesseract is not installed; text extraction was skipped".to_string(),
            )],
        };
    };

    let mut warnings = Vec::new();
    let requested = &config.ocr.languages;
    let missing: Vec<&String> = requested
        .iter()
        .filter(|lang| !backend.languages.iter().any(|have| have == *lang))
        .collect();
    for lang in &missing {
        warnings.push(warn(
            "ocr_language_missing",
            format!(
                "language pack {lang:?} is not installed; installed: {}",
                join_languages(&backend.languages)
            ),
        ));
    }
    let usable: Vec<String> = requested
        .iter()
        .filter(|lang| backend.languages.iter().any(|have| have == *lang))
        .cloned()
        .collect();
    if usable.is_empty() {
        warnings.push(warn(
            "ocr_language_unavailable",
            format!(
                "none of the requested languages are installed; installed: {}",
                join_languages(&backend.languages)
            ),
        ));
        return Outcome {
            ocr: None,
            warnings,
        };
    }

    let started = Instant::now();
    let (prepared, upscaled) = match prepare(image, config) {
        Ok(pair) => pair,
        Err(message) => {
            warnings.push(warn("ocr_prepare_failed", message));
            return Outcome {
                ocr: None,
                warnings,
            };
        }
    };
    let languages = usable.join("+");
    let timeout = Duration::from_secs(config.ocr.timeout_seconds);

    // Pass 1 and 2: plain text, exactly as the prototype compared them.
    let mut best: Option<(usize, String, u32)> = None;
    for psm in PSM_PASSES {
        let psm_arg = psm.to_string();
        let args = [
            "stdin",
            "stdout",
            "--psm",
            psm_arg.as_str(),
            "-l",
            languages.as_str(),
        ];
        match run(&backend.path, &args, &prepared, timeout) {
            Ok(Run::TimedOut) => {
                warnings.push(warn(
                    "ocr_timeout",
                    format!(
                        "tesseract pass --psm {psm} exceeded {}s",
                        config.ocr.timeout_seconds
                    ),
                ));
            }
            Ok(Run::Done {
                status,
                stdout,
                stderr,
            }) => {
                if !status {
                    warnings.push(warn(
                        "ocr_failed",
                        format!(
                            "tesseract pass --psm {psm} exited with failure: {}",
                            first_line(&stderr)
                        ),
                    ));
                    continue;
                }
                let text = normalize_text(&String::from_utf8_lossy(&stdout));
                let length = text.chars().count();
                if best
                    .as_ref()
                    .is_none_or(|(best_len, _, _)| length > *best_len)
                {
                    best = Some((length, text, psm));
                }
            }
            Err(err) => {
                warnings.push(warn(
                    "ocr_failed",
                    format!("cannot run {}: {err}", backend.path.display()),
                ));
                return Outcome {
                    ocr: None,
                    warnings,
                };
            }
        }
    }

    let Some((_, text, psm)) = best else {
        warnings.push(warn(
            "ocr_failed",
            "every tesseract pass failed; no text was extracted".to_string(),
        ));
        return Outcome {
            ocr: None,
            warnings,
        };
    };

    // Pass 3: the winning segmentation, as TSV, for structure.
    let psm_arg = psm.to_string();
    let tsv_args = [
        "stdin",
        "stdout",
        "--psm",
        psm_arg.as_str(),
        "-l",
        languages.as_str(),
        "tsv",
    ];
    let rows = match run(&backend.path, &tsv_args, &prepared, timeout) {
        Ok(Run::Done {
            status: true,
            stdout,
            ..
        }) => parse_tsv(&String::from_utf8_lossy(&stdout)),
        Ok(Run::Done { stderr, .. }) => {
            warnings.push(warn(
                "ocr_structure_unavailable",
                format!(
                    "tsv pass failed: {}; text was still extracted",
                    first_line(&stderr)
                ),
            ));
            Vec::new()
        }
        Ok(Run::TimedOut) => {
            warnings.push(warn(
                "ocr_timeout",
                "tsv pass timed out; text was still extracted".to_string(),
            ));
            Vec::new()
        }
        Err(err) => {
            warnings.push(warn(
                "ocr_structure_unavailable",
                format!("cannot run tesseract for structure: {err}"),
            ));
            Vec::new()
        }
    };

    let rows = match upscaled {
        Some(factor) if factor > 1 => rescale_rows(&rows, factor),
        _ => rows,
    };
    let structure = Structure::build(&rows, image.width(), image.height());
    let confidence = structure.mean_confidence();
    if let Some(mean) = confidence
        && mean < config.ocr.min_confidence
    {
        warnings.push(warn(
            "ocr_low_confidence",
            format!(
                "mean word confidence {mean:.1} is below the {} threshold; the extracted text \
                 is likely misread pixel noise and should not be trusted",
                config.ocr.min_confidence
            ),
        ));
    }

    Outcome {
        ocr: Some(Ocr {
            available: true,
            untrusted: true,
            engine: ENGINE.to_string(),
            version: backend.version.clone(),
            language: languages,
            psm,
            upscaled,
            text,
            confidence,
            words: structure.words,
            lines: structure.lines,
            paragraphs: structure.paragraphs,
            blocks: structure.blocks,
            duration_ms: started.elapsed().as_millis() as u64,
        }),
        warnings,
    }
}

fn join_languages(languages: &[String]) -> String {
    if languages.is_empty() {
        "none".to_string()
    } else {
        languages.join(", ")
    }
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no diagnostics")
        .to_string()
}

/// Prototype behaviour: strip trailing spaces and drop blank lines.
pub fn normalize_text(raw: &str) -> String {
    raw.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Upscale small crops (the prototype's `height < 120` rule) and hand Tesseract
/// greyscale pixels; otherwise pass the analyzed image through unchanged.
fn prepare(image: &DynamicImage, config: &Config) -> Result<(Vec<u8>, Option<u32>), String> {
    let small = image.height() < config.ocr.small_crop_height;
    let factor = config.ocr.upscale_factor;
    let scaled_width = image.width().saturating_mul(factor);
    if config.ocr.upscale_small_crops && small && factor > 1 && scaled_width <= MAX_OCR_WIDTH {
        let height = image.height().saturating_mul(factor).max(1);
        let scaled = image
            .resize_exact(scaled_width.max(1), height, FilterType::Lanczos3)
            .to_luma8();
        let encoded = encode_png(&DynamicImage::ImageLuma8(scaled))?;
        return Ok((encoded, Some(factor)));
    }
    Ok((encode_png(image)?, None))
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|err| format!("cannot encode image for ocr: {err}"))?;
    Ok(buffer.into_inner())
}

/// Find an executable on `PATH`. Windows resolves through `PATHEXT` (the real
/// file is `tesseract.exe`), so a bare name must be tried with each suffix.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names = exe_names(name);
    for dir in std::env::split_paths(&path) {
        for candidate_name in &names {
            let candidate = dir.join(candidate_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn exe_names(name: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![name.to_string()];
    }
    let mut names = vec![format!("{name}.exe")];
    if let Some(pathext) = std::env::var_os("PATHEXT") {
        for ext in std::env::split_paths(&pathext) {
            let ext = ext
                .to_string_lossy()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if ext == "exe" || ext.is_empty() {
                continue;
            }
            names.push(format!("{name}.{ext}"));
        }
    }
    names.push(name.to_string());
    names
}

fn version_of(exe: &Path) -> Option<String> {
    let args = ["--version"];
    let output = run(exe, &args, b"", Duration::from_secs(10)).ok()?;
    let Run::Done { stdout, stderr, .. } = output else {
        return None;
    };
    let combined = if stdout.is_empty() { stderr } else { stdout };
    let text = String::from_utf8_lossy(&combined);
    let first = text.lines().next()?.trim();
    Some(
        first
            .strip_prefix("tesseract ")
            .unwrap_or(first)
            .to_string(),
    )
}

pub fn languages_of(exe: &Path) -> Vec<String> {
    let args = ["--list-langs"];
    let Ok(Run::Done { stdout, stderr, .. }) = run(exe, &args, b"", Duration::from_secs(10)) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(if stdout.is_empty() { &stderr } else { &stdout });
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("List of available languages"))
        .map(str::to_string)
        .collect()
}

enum Run {
    Done {
        status: bool,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    TimedOut,
}

/// Run a child with a writer thread and two reader threads, then poll for the
/// deadline so a hung backend cannot hang the tool.
///
/// The child leads its own process group (Unix): killing only the child leaves
/// any grandchildren it started — a wrapper script's `sleep`, say — holding the
/// output pipes, and the readers would then block on them forever. The group is
/// killed instead, and the reads are bounded anyway so a survivor cannot wedge
/// the caller after the process itself is gone.
fn run(exe: &Path, args: &[&str], input: &[u8], timeout: Duration) -> std::io::Result<Run> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let pid = child.id() as i32;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("stdin not piped"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("stdout not piped"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("stderr not piped"))?;

    let payload = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&payload);
    });
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (err_tx, err_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        let _ = out_tx.send(buffer);
    });
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        let _ = err_tx.send(buffer);
    });

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let success = loop {
        match child.try_wait()? {
            Some(status) => break status.success(),
            None => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    stop(&mut child, pid);
                    break false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };

    let _ = writer.join();
    // Bounded: after a normal exit the reads finish at once, and after a kill
    // only a surviving grandchild could still be holding the pipe.
    let grace = Duration::from_secs(2);
    let stdout = out_rx.recv_timeout(grace).unwrap_or_default();
    let stderr = err_rx.recv_timeout(grace).unwrap_or_default();
    if timed_out {
        return Ok(Run::TimedOut);
    }
    Ok(Run::Done {
        status: success,
        stdout,
        stderr,
    })
}

/// Kill the child and everything it started.
#[cfg(unix)]
fn stop(child: &mut std::process::Child, pid: i32) {
    // Negative pid addresses the group the child leads, set by process_group(0).
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn stop(child: &mut std::process::Child, _pid: i32) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Debug, Clone)]
pub struct Row {
    pub level: u32,
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub confidence: f64,
    pub text: String,
}

/// Tesseract TSV columns: level page_num block_num par_num line_num word_num
/// left top width height conf text.
pub fn parse_tsv(raw: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in raw.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 11 {
            continue;
        }
        let number = |index: usize| -> u32 { fields[index].trim().parse().unwrap_or(0) };
        rows.push(Row {
            level: number(0),
            block: number(2),
            paragraph: number(3),
            line: number(4),
            left: number(6),
            top: number(7),
            width: number(8),
            height: number(9),
            confidence: fields[10].trim().parse().unwrap_or(-1.0),
            text: fields.get(11).copied().unwrap_or("").to_string(),
        });
    }
    rows
}

/// Tesseract reports boxes in the frame it was given; when that frame was an
/// upscaled copy, divide the boxes back into analyzed-image pixels so every
/// coordinate in the report means the same thing.
fn rescale_rows(rows: &[Row], factor: u32) -> Vec<Row> {
    rows.iter()
        .map(|row| Row {
            left: row.left / factor,
            top: row.top / factor,
            width: descend(row.width, factor),
            height: descend(row.height, factor),
            ..row.clone()
        })
        .collect()
}

/// Divide, but never shrink a non-zero extent to nothing.
fn descend(value: u32, factor: u32) -> u32 {
    if value == 0 {
        0
    } else {
        value.div_ceil(factor.max(1))
    }
}

pub struct Structure {
    pub words: Vec<Word>,
    pub lines: Vec<Line>,
    pub paragraphs: Vec<Paragraph>,
    pub blocks: Vec<Block>,
}

impl Structure {
    fn build(rows: &[Row], image_width: u32, image_height: u32) -> Structure {
        let words: Vec<&Row> = rows
            .iter()
            .filter(|row| row.level == 5 && !row.text.trim().is_empty() && row.confidence >= 0.0)
            .collect();

        let word_entries: Vec<Word> = words
            .iter()
            .map(|row| Word {
                text: row.text.trim().to_string(),
                confidence: round1(row.confidence),
                bbox: bbox(
                    row.left,
                    row.top,
                    row.width,
                    row.height,
                    image_width,
                    image_height,
                ),
                block: row.block,
                paragraph: row.paragraph,
                line: row.line,
            })
            .collect();

        let mut lines = Vec::new();
        for group in group_rows(&words, |row| (row.block, row.paragraph, row.line)) {
            let first = group[0];
            let (x, y, w, h) = union(&group);
            lines.push(Line {
                id: format!("{}.{}.{}", first.block, first.paragraph, first.line),
                text: join_words(&group),
                bbox: level_bbox(
                    rows,
                    4,
                    (first.block, first.paragraph, first.line),
                    image_width,
                    image_height,
                )
                .unwrap_or_else(|| bbox(x, y, w, h, image_width, image_height)),
                block: first.block,
                paragraph: first.paragraph,
            });
        }

        let mut paragraphs = Vec::new();
        for group in group_rows(&words, |row| (row.block, row.paragraph)) {
            let first = group[0];
            let (x, y, w, h) = union(&group);
            paragraphs.push(Paragraph {
                id: format!("{}.{}", first.block, first.paragraph),
                text: join_words(&group),
                bbox: level_bbox(
                    rows,
                    3,
                    (first.block, first.paragraph, 0),
                    image_width,
                    image_height,
                )
                .unwrap_or_else(|| bbox(x, y, w, h, image_width, image_height)),
                block: first.block,
            });
        }

        let mut blocks = Vec::new();
        for group in group_rows(&words, |row| (row.block,)) {
            let first = group[0];
            let (x, y, w, h) = union(&group);
            blocks.push(Block {
                id: first.block,
                text: join_words(&group),
                confidence: group_confidence(&group),
                bbox: level_bbox(rows, 2, (first.block, 0, 0), image_width, image_height)
                    .unwrap_or_else(|| bbox(x, y, w, h, image_width, image_height)),
            });
        }

        Structure {
            words: word_entries,
            lines,
            paragraphs,
            blocks,
        }
    }

    fn mean_confidence(&self) -> Option<f64> {
        if self.words.is_empty() {
            return None;
        }
        let sum: f64 = self.words.iter().map(|word| word.confidence).sum();
        Some(round1(sum / self.words.len() as f64))
    }
}

/// Group words by a key, preserving first-seen order (Tesseract's reading order).
fn group_rows<'a, K: PartialEq>(rows: &[&'a Row], key: impl Fn(&Row) -> K) -> Vec<Vec<&'a Row>> {
    let mut groups: Vec<(K, Vec<&'a Row>)> = Vec::new();
    for row in rows {
        let this = key(row);
        match groups.iter_mut().find(|(existing, _)| *existing == this) {
            Some((_, bucket)) => bucket.push(row),
            None => groups.push((this, vec![row])),
        }
    }
    groups.into_iter().map(|(_, bucket)| bucket).collect()
}

fn join_words(rows: &[&Row]) -> String {
    rows.iter()
        .map(|row| row.text.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn group_confidence(rows: &[&Row]) -> Option<f64> {
    if rows.is_empty() {
        return None;
    }
    let sum: f64 = rows.iter().map(|row| row.confidence.max(0.0)).sum();
    Some(round1(sum / rows.len() as f64))
}

fn union(rows: &[&Row]) -> (u32, u32, u32, u32) {
    let left = rows.iter().map(|row| row.left).min().unwrap_or(0);
    let top = rows.iter().map(|row| row.top).min().unwrap_or(0);
    let right = rows
        .iter()
        .map(|row| row.left.saturating_add(row.width))
        .max()
        .unwrap_or(0);
    let bottom = rows
        .iter()
        .map(|row| row.top.saturating_add(row.height))
        .max()
        .unwrap_or(0);
    (
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

/// Tesseract's own box for a hierarchy row, when it emitted one.
fn level_bbox(
    rows: &[Row],
    level: u32,
    key: (u32, u32, u32),
    image_width: u32,
    image_height: u32,
) -> Option<Bbox> {
    let row = rows.iter().find(|row| {
        row.level == level && row.block == key.0 && row.paragraph == key.1 && row.line == key.2
    })?;
    if row.width == 0 || row.height == 0 {
        return None;
    }
    Some(bbox(
        row.left,
        row.top,
        row.width,
        row.height,
        image_width,
        image_height,
    ))
}

pub fn bbox(x: u32, y: u32, width: u32, height: u32, image_width: u32, image_height: u32) -> Bbox {
    let normalise = |value: u32, extent: u32| -> f64 {
        if extent == 0 {
            0.0
        } else {
            round6(f64::from(value) / f64::from(extent))
        }
    };
    Bbox {
        x,
        y,
        width,
        height,
        normalized: Norm {
            x: normalise(x, image_width),
            y: normalise(y, image_height),
            width: normalise(width, image_width),
            height: normalise(height, image_height),
        },
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
