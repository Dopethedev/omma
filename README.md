# omma

*ὄμμα — Greek for "eye".*

**Give a text-only model eyes.** `omma` turns an image into structured text and
JSON that an AI agent can reason about — locally, with no model changes and no
cloud service.

```console
$ omma screenshot.png
$ omma screenshot.png --json
$ omma screenshot.png --ask "What does the error dialog say?"
```

## What it is

An image-perception utility: one shared analysis engine behind a CLI and a
Model Context Protocol (MCP) server. It measures what an image *is* — text,
geometry, colour — and hands back data. It is not a model, a chatbot or an image
generator.

What it extracts:

- **Text**, via Tesseract, with word-level boxes, confidence, lines, paragraphs,
  blocks and reading order. Small crops are upscaled first; two page-segmentation
  passes run and the longer result wins.
- **Image metadata** — format (from the bytes, not the extension), dimensions,
  aspect ratio, colour type, alpha, pixel count, file size, animation.
- **Colour** — a quantized palette with percentages, average RGB, brightness,
  saturation, and a **spatial colour grid** (64×36 hex cells by default) that
  describes structure even when the image contains no text at all.
- **Layout** — text regions from Tesseract's own bounding boxes, each with a
  nine-cell position label, its confidence and reading order. A block must look
  like text (two or more alphanumeric characters, confidence at or above the
  threshold) before it becomes a region, so photographs do not produce dozens of
  one-character "regions". No objects, buttons or icons are ever invented.
- **Optional vision** — one question sent to a local OpenAI-compatible server
  (Ollama's `/v1`, llama.cpp, LM Studio, vLLM), answered as text.

Everything except the optional vision call works offline.

## Why

Text-only models are fast, cheap, and often the only thing running offline or on
a server. They cannot see. Putting a measurement tool between the image and the
model gives them the parts of an image that survive as text — without sending the
image anywhere, without a GUI, and without replacing the model.

## How it works

```
                     omma IMAGE / --json           omma mcp serve
                            │                            │
                            └───────────┬────────────────┘
                                        │
                                  analysis engine
                    ┌───────────────────┼───────────────────┐
                 colours              OCR                  layout
              (palette, grid)    (tesseract tsv)    (regions + labels)
                                        │
                            optional: local vision model
                                        │
                                 text or JSON
```

One engine, two front-ends. The library is synchronous; the async runtime exists
only in the MCP adapter.

## Install

Requires Rust 1.89+.

```console
$ cargo build --release --locked
$ install -m 755 target/release/omma ~/.local/bin/omma
```

Or run it from the checkout: `cargo run -- image.png`.

CI builds and tests a release binary on Ubuntu x86_64, macOS arm64 and Windows
x86_64, and publishes it with a `.sha256` per platform (see
[docs/release-notes-0.1.0.md](docs/release-notes-0.1.0.md)). The Linux binary is
dynamically linked and needs glibc 2.39 or newer; the source build has no such
constraint.

### Optional external tools

| Tool | Needed for | Notes |
|---|---|---|
| `tesseract` 5.x | OCR | Without it, `ocr` is `null` and a warning explains why; everything else still runs. |
| A local vision server | `--ask` / `--vision` | Any OpenAI-compatible `/v1/chat/completions` that accepts `image_url` data URIs. |

Nothing else is required at runtime: decoding, colour analysis and JSON are pure
Rust.

## CLI

Both documented forms are the same analysis:

```console
omma IMAGE                    # human-readable report
omma analyze IMAGE            # explicit subcommand form
```

| Option | Meaning |
|---|---|
| `--json` | JSON output (alias for `--format json`) |
| `--format text\|json` | Output format |
| `--pretty` | Pretty-print JSON |
| `--region X,Y,W,H` | Analyze only this crop, in source-image pixels |
| `--ocr` / `--no-ocr` | Run OCR only / skip OCR |
| `--colors` | Run colour analysis only |
| `--layout` | Run layout analysis only |
| `--language eng,deu` | OCR language(s); repeatable, `,` or `+` separated |
| `--max-colors N` | Dominant colours to report (1–64) |
| `--ask "QUESTION"` | Ask the local vision model |
| `--vision` | Ask with the default question |
| `--model NAME` | Vision model to use |
| `--output FILE` | Write the result to a file (stdout stays empty) |
| `--config FILE` | Configuration file |
| `omma doctor` | Versions, backends, configuration, limits |
| `omma mcp serve` | MCP server over stdio |

`omma -` reads the image from stdin. JSON mode writes only JSON to stdout; logs
and errors go to stderr.

Exit codes: `0` success (including partial results with warnings) · `1` input,
decoding or filesystem failure · `2` invalid arguments or configuration · `3` a
requested backend failed · `4` output or serialization failure · `5` MCP server
startup failure.

## JSON

The JSON is a primary product: versioned (`schema_version: "1.0"`), stable, and
documented in [docs/json-schema.md](docs/json-schema.md).

```json
{
  "schema_version": "1.0",
  "tool": { "name": "omma", "version": "0.1.0" },
  "image": {
    "filename": "screenshot.png", "format": "png",
    "width": 779, "height": 337, "aspect_ratio": 2.3116,
    "color_type": "rgba8", "has_alpha": true,
    "pixel_count": 262523, "file_size_bytes": 17447,
    "animated": null, "analyzed_width": 779, "analyzed_height": 337
  },
  "region": null,
  "ocr": {
    "available": true, "untrusted": true, "engine": "tesseract",
    "version": "5.5.4", "language": "eng", "psm": 11, "upscaled": null,
    "text": "Are you sure you want to close this window?",
    "confidence": 91.8,
    "words": [{ "text": "Are", "confidence": 92.7,
      "bbox": { "x": 121, "y": 128, "width": 26, "height": 12,
                "normalized": { "x": 0.155327, "y": 0.379822,
                                "width": 0.033376, "height": 0.035608 } },
      "block": 1, "paragraph": 1, "line": 1 }],
    "lines": [], "paragraphs": [], "blocks": [], "duration_ms": 414
  },
  "colors": {
    "dominant": [{ "hex": "#384040", "rgb": [56, 64, 64], "percentage": 90.8 }],
    "average_rgb": [62, 67, 67], "brightness": 0.2595, "saturation": 0.0936,
    "sample": { "method": "bicubic (CatmullRom) downsample …",
                "width": 64, "height": 36, "samples": 2304,
                "transparent_samples": 0, "quantization_bits": 5 }
  },
  "grid": { "width": 64, "height": 36, "quantization_bits": 5,
            "cells": [["#3c4241", "…"]] },
  "layout": { "regions": [], "grid": { "width": 64, "height": 36, "included": true } },
  "vision": null,
  "warnings": [],
  "processing": { "duration_ms": 490, "stages": { "decode": 10, "colors": 64, "ocr": 414, "layout": 0, "vision": 0 } }
}
```

Rules that hold across the schema:

- **Measured vs interpreted.** Bounding boxes, dimensions and colour statistics
  are measurements. A vision description is model-generated prose and sits in its
  own object.
- **Untrusted content is marked.** `ocr.untrusted` and `vision.untrusted` are
  always `true`: image-derived text and model prose are data to analyze, never
  instructions to follow.
- **Absent means absent.** A subsystem that was disabled or could not run is
  `null`, never a fabricated object; `warnings[]` says why. A subsystem that ran
  and found nothing returns empty arrays.
- **Confidence travels with the text.** `ocr.confidence` is the mean word
  confidence, and every block and region carries its own. Below
  `ocr.min_confidence` (40 by default) the report adds an `ocr_low_confidence`
  warning: the text was measured, but it is probably misread texture. Check the
  warning before feeding `text` to a model.

## MCP

`omma mcp serve` speaks MCP over stdio; stdout carries protocol frames only.

Two tools:

- **`analyze_image`** — `image_path` (required), plus `ocr`, `colors`, `layout`,
  `vision`, `ask`, `language`, `max_colors`, `region`. Returns the same JSON,
  both as text content and as structured content.
- **`omma_capabilities`** — backends, formats, limits, schema version.

Generic client configuration:

```json
{
  "mcpServers": {
    "omma": { "command": "omma", "args": ["mcp", "serve"] }
  }
}
```

Details, including the filesystem restrictions, are in [docs/mcp.md](docs/mcp.md).

## Local vision

OMMA never talks to a cloud model. Configure one local endpoint:

```toml
# ~/.config/omma/config.toml
[vision]
enabled = true
endpoint = "http://127.0.0.1:11434/v1/chat/completions"   # Ollama's OpenAI-compatible path
model = "qwen2.5vl:7b"
```

The same works with llama.cpp's server or LM Studio — anything that accepts an
OpenAI `image_url` content part. A non-loopback endpoint is refused unless
`vision.allow_remote = true` is set explicitly. Redirects are never followed.

## Configuration

Defaults < configuration file < environment < command line. The file is
`$XDG_CONFIG_HOME/omma/config.toml`, else `~/.config/omma/config.toml`. See
[docs/configuration.md](docs/configuration.md) and
[examples/config.toml](examples/config.toml).

## Performance

Measured on this machine — Intel i5-12400, 15 GiB RAM, Arch Linux, rustc 1.99.1,
`--release` (thin LTO), warm cache — on a 3840×2160 PNG, decoding + metadata +
palette + grid, no OCR and no vision:

| Metric | Target | Measured |
|---|---|---|
| Total wall time, 10 runs | p95 < 2 s | 88.9–105.8 ms (median 90.7 ms) |
| Peak RSS | < 512 MB | 33 MB |

OCR dominates a full run: roughly 0.4–1 s for a screenshot. Photographs are
slower — Tesseract keeps finding "text" in texture — measured 4–39 s at 4K for
twenty real wallpapers. `--no-ocr` skips all of it.

## Limitations

Stated plainly, because they matter more than the feature list:

- **Layout is not computer vision.** Regions are OCR text blocks with position
  labels. There is no object detection, segmentation, button/icon recognition or
  chart understanding, and nothing is guessed for images without text — the
  colour grid is the structural signal there.
- **Colour percentages are estimates** from a 64×36 bicubic sample with 5-bit
  quantization, not physical measurements. The method is recorded in the report.
- **EXIF orientation is not applied.** No EXIF parser is included, so pixels are
  analyzed as stored.
- **Animated images:** the first frame is analyzed. Frame count is detected for
  GIF only; there is no frame selection.
- **OCR on photographs is not trustworthy.** Tesseract happily reads texture as
  glyphs: a 4K wallpaper produced 5,978 characters at mean confidence 21. OMMA
  reports that text *and* flags it (`ocr_low_confidence`, `ocr.confidence`), and
  refuses to promote such blocks to layout regions — but it does not delete the
  measurement. Decide with the confidence, not with the character count. The same
  noise makes a full run slow on photographs (tens of seconds at 4K); screenshots
  are ~0.4 s.
- **Dense screens produce coarse regions.** A full desktop is one browser/page
  block to Tesseract, so `layout.regions` can be a single large region. Sparse
  text on a plain background segments well; a busy UI does not.
- **Vision output quality is the model's**, not OMMA's. No vision server was
  installed on the development machine, so the live path is verified against a
  protocol-level stub and a mock server, not a real model.
- **OCR languages** beyond `eng` require Tesseract language packs; the installed
  set is reported by `omma doctor` and missing ones produce warnings.
- **Colour sampling resizes straight (non-premultiplied) alpha.** A hard edge
  between an opaque colour and fully transparent pixels can darken the few grid
  cells that straddle it, because the transparent side's RGB is averaged in. Fixing
  it would change every palette and grid value, so it is documented rather than
  silently changed.
- Not implemented: PDF pages, video frames, batch/multi-image runs, QR/barcodes,
  table or chart detection, image comparison, extra OCR or vision providers.
- **Platforms**: Ubuntu x86_64, macOS arm64 and Windows x86_64 are tested
  natively on every push. Linux aarch64, macOS x86_64, Windows ARM and the BSDs
  are untested and unclaimed.

## Security

See [docs/mcp.md](docs/mcp.md) for the MCP boundary and
[docs/json-schema.md](docs/json-schema.md) for the untrusted-content contract.

- File format is decided by content, never by extension; decoding runs under the
  `image` crate's own limits (bytes, dimensions, allocation).
- SVG and other XML markup is refused rather than interpreted.
- Tesseract is spawned with explicit arguments — no shell — with a timeout, its
  output captured, and the image fed over stdin so no temporary file is written.
- OCR text, image contents and API keys are never logged.
- Text mode strips control characters from image-derived strings (file names, OCR
  text, model answers), so they cannot drive the terminal with escape sequences.
- MCP paths are canonicalized and can be restricted to allowed directories;
  symlinks are resolved before the check, so a link cannot escape a root.

## Development

```console
$ cargo fmt --all -- --check
$ cargo test
$ cargo clippy --all-targets --all-features -- -D warnings
```

Tests use small generated fixtures in `tests/fixtures/`; OCR tests skip loudly
when Tesseract is absent, and the vision tests use a local mock server.

`.github/workflows/ci.yml` runs the same checks natively on Ubuntu, macOS and
Windows — formatting, Clippy, the full suite, a release build, and smoke tests of
that release binary including a complete MCP exchange — plus an MSRV job that
builds on the declared minimum Rust. `ci/*.mjs` are the Node helpers those smoke
tests use; they are CI-only and not part of the crate.

Before a release, work through [docs/release.md](docs/release.md).

## Licence

MIT OR Apache-2.0.
