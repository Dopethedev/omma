# Configuration

Precedence, highest wins: **command line → environment → configuration file →
built-in defaults**.

## Location

1. `--config PATH`
2. `$OMMA_CONFIG`
3. `$XDG_CONFIG_HOME/omma/config.toml`
4. `$HOME/.config/omma/config.toml`

A missing discovered file means defaults. A missing *explicit* path (or
`$OMMA_CONFIG`) is an error: if you named a file, it must exist. If neither
`XDG_CONFIG_HOME` nor `HOME` is set, no file is looked for and defaults apply;
`omma doctor` says so.

Unknown keys are errors, not silent ignores.

## File

```toml
[general]
default_format = "text"     # text | json
max_file_size_mb = 32
max_pixels = 40000000
max_dimension = 16384       # px, per side
max_alloc_mb = 256          # decoder allocation ceiling

[ocr]
enabled = true
languages = ["eng"]
timeout_seconds = 30        # per Tesseract invocation
upscale_small_crops = true
small_crop_height = 120     # crops shorter than this are upscaled
upscale_factor = 4
min_confidence = 40.0       # below this: low-confidence warning, no layout regions

[colors]
enabled = true
max_colors = 6
grid_width = 64
grid_height = 36
quantization_bits = 5

[layout]
enabled = true

[vision]
enabled = false
endpoint = "http://127.0.0.1:11434/v1/chat/completions"
model = ""
timeout_seconds = 120
max_tokens = 400
api_key = ""                # optional; or OMMA_VISION_API_KEY
allow_remote = false        # true permits a non-loopback endpoint

[mcp]
allowed_directories = []    # empty = any path this process can read

[logging]
level = "warn"              # tracing filter, e.g. "info", "omma=debug"
```

Every section and key is optional.

## Environment

| Variable | Overrides |
|---|---|
| `OMMA_CONFIG` | configuration file path |
| `OMMA_FORMAT` | `general.default_format` |
| `OMMA_MAX_FILE_SIZE_MB` | `general.max_file_size_mb` |
| `OMMA_MAX_PIXELS` | `general.max_pixels` |
| `OMMA_MAX_DIMENSION` | `general.max_dimension` |
| `OMMA_OCR_ENABLED` | `ocr.enabled` |
| `OMMA_OCR_LANGUAGES` | `ocr.languages` (`eng` or `eng,deu`) |
| `OMMA_OCR_TIMEOUT_SECONDS` | `ocr.timeout_seconds` |
| `OMMA_OCR_MIN_CONFIDENCE` | `ocr.min_confidence` |
| `OMMA_COLORS_ENABLED` | `colors.enabled` |
| `OMMA_MAX_COLORS` | `colors.max_colors` |
| `OMMA_LAYOUT_ENABLED` | `layout.enabled` |
| `OMMA_VISION_ENABLED` | `vision.enabled` |
| `OMMA_VISION_ENDPOINT` | `vision.endpoint` |
| `OMMA_VISION_MODEL` | `vision.model` |
| `OMMA_VISION_TIMEOUT_SECONDS` | `vision.timeout_seconds` |
| `OMMA_VISION_MAX_TOKENS` | `vision.max_tokens` |
| `OMMA_VISION_API_KEY` | `vision.api_key` |
| `OMMA_VISION_ALLOW_REMOTE` | `vision.allow_remote` |
| `OMMA_LOG_LEVEL` | `logging.level` |

Booleans accept `1/0`, `true/false`, `yes/no`, `on/off`.

## Vision endpoints

The endpoint must be `http://` or `https://` with a host, no embedded
credentials, and — unless `allow_remote = true` / `OMMA_VISION_ALLOW_REMOTE=1` —
a loopback host (`localhost`, `127.0.0.0/8`, `::1`). Redirects are never
followed, so a local endpoint cannot redirect the upload elsewhere.

A configuration that is invalid is rejected before any image is read.

## Why these defaults

- 32 MB / 40 M pixels / 256 MB allocation: big enough for a 12-megapixel photo or
  a retina screenshot, small enough that a decompression bomb fails fast.
- Palette of 6 colours: the prototype's default, and about as many as a reader
  can distinguish in a fractional share.
- 64×36 grid: the correction's default; it keeps enough spatial resolution to see
  a toolbar or a dialog box.
- OCR language `eng`, timeout 30 s: Tesseract is fast; a timeout this long only
  triggers on something pathological.
- `min_confidence = 40`: screenshots measure 80–92, photographs 20–40. The
  threshold separates "text" from "texture" without hiding anything — it flags
  the output and gates layout regions, it never deletes measured text.
- Vision disabled: sending an image anywhere is opt-in.
