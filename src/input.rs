//! Reading, validating and decoding an image, including region cropping.
//!
//! Everything here treats the input as untrusted: the format comes from the
//! bytes, never from the file extension, and decoding runs under the `image`
//! crate's own limits rather than a post-hoc dimension check.

use std::io::Read;
use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageReader};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::report::RegionMeta;

pub struct Decoded {
    /// The original encoded bytes, still used for the optional vision call.
    pub bytes: Vec<u8>,
    pub filename: String,
    pub format: ImageFormat,
    pub file_size: u64,
    /// Frame count is only readable for GIF; `None` means not detectable.
    pub animated: Option<bool>,
    pub source_width: u32,
    pub source_height: u32,
    pub region: Option<RegionMeta>,
    pub image: DynamicImage,
}

impl Decoded {
    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    pub fn format_name(&self) -> &'static str {
        format_name(self.format)
    }
}

pub fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Gif => "gif",
        ImageFormat::Tiff => "tiff",
        other => other.extensions_str().first().copied().unwrap_or("unknown"),
    }
}

/// Read the raw bytes of a path, or of stdin when the path is `-`.
pub fn read_bytes(source: &str, limit: u64) -> Result<(Vec<u8>, u64)> {
    if source == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .lock()
            .take(limit.saturating_add(1))
            .read_to_end(&mut buf)
            .map_err(|err| Error::Usage(format!("cannot read stdin: {err}")))?;
        let size = buf.len() as u64;
        if size > limit {
            return Err(Error::TooLarge { size, limit });
        }
        if buf.is_empty() {
            return Err(Error::EmptyInput);
        }
        return Ok((buf, size));
    }

    let path = Path::new(source);
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) => return Err(map_open_error(path, err)),
    };
    if !meta.is_file() {
        return Err(Error::NotRegularFile(path.to_path_buf()));
    }
    if meta.len() > limit {
        return Err(Error::TooLarge {
            size: meta.len(),
            limit,
        });
    }
    let bytes = std::fs::read(path).map_err(|err| map_open_error(path, err))?;
    if bytes.is_empty() {
        return Err(Error::EmptyInput);
    }
    let size = bytes.len() as u64;
    Ok((bytes, size))
}

fn map_open_error(path: &Path, err: std::io::Error) -> Error {
    match err.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound(path.to_path_buf()),
        std::io::ErrorKind::PermissionDenied => Error::Permission(path.to_path_buf()),
        _ => Error::Io {
            path: path.to_path_buf(),
            source: err,
        },
    }
}

/// Decode bytes with the configured limits applied during decoding.
pub fn decode(source: &str, bytes: Vec<u8>, file_size: u64, config: &Config) -> Result<Decoded> {
    if is_markup(&bytes) {
        return Err(Error::UnsupportedFormat(
            "svg or other XML markup is not supported".to_string(),
        ));
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(config.general.max_dimension);
    limits.max_image_height = Some(config.general.max_dimension);
    limits.max_alloc = Some(config.max_alloc_bytes());

    let mut reader = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|err| Error::Decode(err.to_string()))?;
    let format = reader
        .format()
        .ok_or_else(|| Error::UnsupportedFormat("unrecognized image content".to_string()))?;

    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|err| map_decode_error(err, config, &bytes))?;

    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return Err(Error::Decode("image has zero extent".to_string()));
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > config.general.max_pixels {
        return Err(Error::Dimensions {
            width,
            height,
            max_pixels: config.general.max_pixels,
            max_dimension: config.general.max_dimension,
        });
    }

    let animated = if format == ImageFormat::Gif {
        gif_is_animated(&bytes)
    } else {
        None
    };

    Ok(Decoded {
        filename: filename_of(source),
        format,
        file_size,
        animated,
        source_width: width,
        source_height: height,
        region: None,
        image,
        bytes,
    })
}

fn map_decode_error(err: image::ImageError, config: &Config, bytes: &[u8]) -> Error {
    let text = err.to_string();
    if text.contains("limit") || text.contains("Limits") {
        // The decoder refused before allocating; read the header (no decode) so
        // the error names the image's real size rather than the limit twice.
        let (width, height) = probe_dimensions(bytes)
            .unwrap_or((config.general.max_dimension, config.general.max_dimension));
        return Error::Dimensions {
            width,
            height,
            max_pixels: config.general.max_pixels,
            max_dimension: config.general.max_dimension,
        };
    }
    if matches!(err, image::ImageError::Unsupported(_)) {
        return Error::UnsupportedFormat(text);
    }
    Error::Decode(text)
}

/// Header-only dimension read, without our limits, so an oversized image can
/// still be described accurately in the error.
fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.no_limits();
    reader.into_dimensions().ok()
}

/// XML/SVG sniffing: an image file never starts with `<`.
fn is_markup(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'<')
}

fn filename_of(source: &str) -> String {
    if source == "-" {
        return "stdin".to_string();
    }
    Path::new(source)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string())
}

fn gif_is_animated(bytes: &[u8]) -> Option<bool> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let mut frames = 0usize;
    for frame in decoder.into_frames().take(2) {
        if frame.is_err() {
            return None;
        }
        frames += 1;
    }
    Some(frames > 1)
}

/// Parse `X,Y,W,H` into four numbers, rejecting anything malformed.
pub fn parse_box(spec: &str) -> Result<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(Error::Region {
            region: spec.to_string(),
            reason: "expected X,Y,W,H".to_string(),
        });
    }
    let mut values = [0u32; 4];
    for (index, part) in parts.iter().enumerate() {
        values[index] = part.parse().map_err(|_| Error::Region {
            region: spec.to_string(),
            reason: format!("{part:?} is not a whole number"),
        })?;
    }
    let [x, y, width, height] = values;
    Ok((x, y, width, height))
}

/// Paint opaque grey over `X,Y,W,H` rectangles before anything reads pixels.
///
/// This runs on the decoded image, so a covered area is absent from OCR, the
/// colour grid, layout regions and the copy sent to a vision model alike. That
/// is the point: cropping only changed what the model saw while the report kept
/// the text. Coordinates address the image as it stands at this moment — source
/// pixels, or the crop's pixels when a region was requested first — and a box
/// that runs past the edge is clamped rather than refused.
pub fn redact(image: &mut DynamicImage, specs: &[String]) -> Result<Vec<RegionMeta>> {
    let mut applied = Vec::with_capacity(specs.len());
    for spec in specs {
        let (x, y, width, height) = parse_box(spec)?;
        let (source_width, source_height) = (image.width(), image.height());
        if width == 0 || height == 0 {
            return Err(Error::Region {
                region: spec.clone(),
                reason: "width and height must be greater than 0".to_string(),
            });
        }
        if x >= source_width || y >= source_height {
            return Err(Error::Region {
                region: spec.clone(),
                reason: format!(
                    "origin ({x},{y}) lies outside the {source_width}x{source_height} image"
                ),
            });
        }
        let width = width.min(source_width - x);
        let height = height.min(source_height - y);
        let fill = image::RgbaImage::from_pixel(width, height, image::Rgba([128, 128, 128, 255]));
        image::imageops::replace(
            image,
            &DynamicImage::ImageRgba8(fill),
            i64::from(x),
            i64::from(y),
        );
        applied.push(RegionMeta {
            x,
            y,
            width,
            height,
            source_width,
            source_height,
        });
    }
    Ok(applied)
}

/// Parse `X,Y,W,H` and crop, validating against the decoded image.
pub fn crop(image: DynamicImage, spec: &str) -> Result<(DynamicImage, RegionMeta)> {
    let (x, y, width, height) = parse_box(spec)?;
    let (source_width, source_height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return Err(Error::Region {
            region: spec.to_string(),
            reason: "width and height must be greater than 0".to_string(),
        });
    }
    if x >= source_width || y >= source_height {
        return Err(Error::Region {
            region: spec.to_string(),
            reason: format!(
                "origin ({x},{y}) lies outside the {source_width}x{source_height} image"
            ),
        });
    }
    if u64::from(x) + u64::from(width) > u64::from(source_width)
        || u64::from(y) + u64::from(height) > u64::from(source_height)
    {
        return Err(Error::Region {
            region: spec.to_string(),
            reason: format!("region extends past the {source_width}x{source_height} image"),
        });
    }
    let cropped = image.crop_imm(x, y, width, height);
    Ok((
        cropped,
        RegionMeta {
            x,
            y,
            width,
            height,
            source_width,
            source_height,
        },
    ))
}

/// Canonicalize the requested path and require it to resolve inside one of the
/// allowed roots. Canonicalization resolves symlinks first, so a link pointing
/// out of a root is refused rather than followed.
pub fn enforce_allowed_roots(source: &str, roots: &[std::path::PathBuf]) -> Result<()> {
    if source == "-" || roots.is_empty() {
        return Ok(());
    }
    let path = Path::new(source);
    let canonical = std::fs::canonicalize(path).map_err(|err| map_open_error(path, err))?;
    for root in roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        if canonical.starts_with(&root) {
            return Ok(());
        }
    }
    Err(Error::Usage(format!(
        "{source} is outside the allowed directories"
    )))
}

impl Decoded {
    /// Encode the analyzed image as PNG, in memory. OCR is fed through stdin so
    /// no temporary file is ever written.
    pub fn png_bytes(&self) -> Result<Vec<u8>> {
        encode_png(&self.image).map_err(Error::Decode)
    }
}

/// Re-encode a decoded image as PNG; shared by OCR and the vision upload.
pub fn encode_png(image: &DynamicImage) -> std::result::Result<Vec<u8>, String> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|err| format!("cannot re-encode image: {err}"))?;
    Ok(buffer.into_inner())
}
