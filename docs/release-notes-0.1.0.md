# OMMA 0.1.0 — first release

**Give a text-only model eyes.** OMMA turns an image into structured text and
JSON an AI agent can reason about: OCR text with boxes and confidence, colour
palette, a spatial colour grid, layout regions, and an optional description from
a local vision model. One engine behind a CLI and an MCP stdio server. No cloud
service, no model changes, no telemetry.

```console
$ omma screenshot.png
$ omma screenshot.png --json
$ omma mcp serve
```

## Highlights

- **Text**: Tesseract via subprocess, image through stdin (no temp files), small
  crops upscaled 4×, two segmentation passes with the longer result winning,
  hierarchy and boxes from Tesseract's own TSV columns. Word/line/paragraph/block
  confidence is reported, and low-confidence output is flagged rather than passed
  off as text.
- **Colour**: deterministic palette (5-bit quantized, ranked by frequency with an
  explicit tie-break) and a 64×36 spatial grid of hex cells that describes
  structure even when an image contains no text. Transparent pixels are excluded,
  not flattened to black.
- **Layout**: text regions from measured blocks with nine-cell position labels
  and reading order — and a credibility rule (≥2 alphanumeric characters, mean
  confidence ≥ 40) so photograph texture never becomes invented regions.
- **JSON**: versioned (`schema_version: 1.0`), every key always present, `null`
  when a subsystem could not run, `warnings[]` saying why, and `untrusted: true`
  on OCR text and vision prose so a consuming agent treats them as data.
- **MCP**: `analyze_image` and `omma_capabilities`, structured and text content,
  read-only/idempotent annotations, canonicalized paths, optional allowed
  directories with symlink-escape protection.
- **Safety**: format from content not extension, decoder limits, SVG refused, no
  shell, OCR timeouts, loopback-only vision by default, redirects not followed,
  nothing logged but diagnostics.

## Install

### Prebuilt binary (Linux x86_64)

```sh
install -m 755 omma-0.1.0-x86_64-linux-gnu ~/.local/bin/omma
omma doctor
```

The prebuilt binary is dynamically linked and needs **glibc ≥ 2.39** (Arch,
Fedora 40+, Ubuntu 24.04 or newer). On older systems, build from source.

Verify the download:

```sh
sha256sum -c omma-0.1.0-x86_64-linux-gnu.sha256
```

`omma-0.1.0-x86_64-linux-gnu.sha256`:
`9f0bc76f59c890b712cc3251c723d91ad506d7d4585b39785781360cbb500db2`

### From source (any Linux, Rust 1.88+)

```sh
cargo build --release --locked
install -m 755 target/release/omma ~/.local/bin/omma
```

The build is reproducible: this release was built twice from a clean copy of the
tree and produced the same SHA-256 both times.

### Optional tools

- `tesseract` 5.x — OCR. Without it, `ocr` is `null` and a warning explains why;
  everything else still works.
- A local OpenAI-compatible vision server — only for `--ask` / `--vision`.

## Compatibility

- Linux x86_64 (the only platform tested).
- Rust 1.88 or newer to build; edition 2024.
- Images: PNG, JPEG, WebP, BMP, GIF (first frame; animation detected), TIFF.
  SVG and other XML markup are refused.

## Testing in this release

- 68 automated tests: CLI, JSON schema, decoding and limits, OCR, MCP protocol,
  vision request construction (against a local mock), path restrictions.
- `cargo fmt --check` and `cargo clippy --all-targets --all-features -D warnings`
  clean.
- MCP verified with the official TypeScript SDK client (18/18 checks) and driven
  end to end by a real agent (Control).
- Filesystem boundary verified through a real client: outside-root, `/etc`, and a
  symlink escaping an allowed root are all denied.
- Performance: 3840×2160 PNG, decode + metadata + palette + grid (`--no-ocr`),
  10 warm runs: 98–101 ms, peak RSS 33 MB.
- Real images: 20 wallpapers (1080p/1440p/4K), desktop and dialog screenshots, a
  63×30 clip, transparent PNG, a chart.

## Known limitations

Stated so nothing here is a surprise after install:

- **Real local vision model integration is not verified.** No vision server was
  installed on the development machine; the provider is protocol-complete and was
  exercised against a local stub for success, timeout, HTTP errors, non-JSON
  bodies and unreachable endpoints. Expect to test it against your own server.
- **OCR on photographs is unreliable by nature.** Tesseract reads texture as
  glyphs (a 4K wallpaper yielded ~6,000 characters at confidence 21). OMMA reports
  that text, flags it (`ocr_low_confidence`) and excludes it from layout regions,
  but does not delete the measurement. Such runs are also slow (tens of seconds at
  4K); screenshots take ~0.4 s.
- **Dense screens segment coarsely** — a full desktop is often one region to
  Tesseract. Sparse text on plain backgrounds segments well.
- OCR languages are limited to the Tesseract packs you have installed (`eng` and
  `osd` here).
- EXIF orientation is not applied; first frame only for animated images; no PDF,
  video, batch, QR/barcode, table or chart understanding.
- The crate is not published to crates.io, and no repository/homepage URL is set
  in `Cargo.toml`, because no public remote exists yet.

## Licence

MIT OR Apache-2.0.
