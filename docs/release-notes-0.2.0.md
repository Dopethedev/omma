# OMMA 0.2.0 — first public release

**Give a text-only model eyes.** OMMA turns an image into structured text and
JSON an AI agent can reason about: OCR text with boxes and confidence, colour
palette, a spatial colour grid, layout regions, and a description from a vision
model — local or hosted. One engine behind a CLI and an MCP stdio server. No
telemetry, no cloud service unless you configure one.

```console
$ omma screenshot.png                                  # human report
$ omma screenshot.png --json                           # the same, as data
$ omma screenshot.png --ask "What does this screen do?" # + a model's description
$ omma mcp serve                                        # MCP for an agent
```

## Highlights

- **Text**: Tesseract via subprocess, image through stdin (no temporary files),
  small crops upscaled 4×, two segmentation passes with the longer result
  winning, and the word/line/paragraph/block hierarchy taken from Tesseract's own
  TSV columns. Confidence travels with the text and low-confidence output is
  flagged (`ocr_low_confidence`) rather than passed off as reliable.
- **Colour**: a deterministic palette (5-bit quantized, ranked by frequency with
  an explicit tie-break) and a 64×36 grid of hex cells that describes structure
  even when an image contains no text. Transparent pixels are excluded, not
  flattened to black, and a fully transparent image says so
  (`colors_no_opaque_samples`).
- **Layout**: text regions from measured blocks with nine-cell position labels,
  normalized bounding boxes and reading order — plus a credibility rule (≥2
  alphanumeric characters, mean confidence ≥ 40) so photograph texture never
  becomes an invented region.
- **Vision**: one question, answered as text by any OpenAI-compatible
  `/v1/chat/completions` endpoint — a local model (Ollama, llama.cpp, LM Studio,
  vLLM) or a hosted one (GLM-5.3-Flash on W&B Inference or Z.AI). The image is
  downscaled to a configured ceiling, the model gets a system prompt that asks it
  to describe rather than obey, and the answer is marked `untrusted`.
- **MCP**: `analyze_image` accepts a path **or** the image bytes (`image_base64`,
  data URI accepted) — exactly one source per call — plus `omma_capabilities` for
  backends and limits. No temporary files, no shell, no network beyond the
  configured vision endpoint.
- **JSON**: versioned (`schema_version: 1.0`), every key always present, `null`
  when a subsystem could not run, `warnings[]` saying why.

## Install

### Prebuilt binaries

CI builds and tests the release binary on every platform it supports; each
artifact carries its own `.sha256`:

| Artifact | Platform |
|---|---|
| `omma-0.2.0-linux-x86_64` | Ubuntu, glibc ≥ 2.39 |
| `omma-0.2.0-macos-aarch64` | macOS on Apple Silicon |
| `omma-0.2.0-macos-x86_64` | macOS on Intel |
| `omma-0.2.0-windows-x86_64.exe` | Windows x86_64 |

```sh
install -m 755 omma-0.2.0-linux-x86_64 ~/.local/bin/omma
omma doctor
sha256sum -c omma-0.2.0-linux-x86_64.sha256   # shasum -a 256 -c on macOS
```

The Linux binary is dynamically linked and needs **glibc ≥ 2.39** (Ubuntu 24.04,
Arch, Fedora 40 or newer). On older systems, build from source.

### From source (any platform, Rust 1.89+)

```sh
cargo build --release --locked
install -m 755 target/release/omma ~/.local/bin/omma
```

With `--locked` and the committed `Cargo.lock`, a clean checkout builds
reproducibly: two local builds from separate directories produced the same
SHA-256, and CI builds each platform from the same lockfile.

### Optional tools

- `tesseract` 5.x — OCR. Without it `ocr` is `null` and a warning explains why;
  everything else still works.
- A vision endpoint — only for `--ask` / `--vision`. Local: start Ollama,
  llama.cpp or LM Studio. Hosted: an API key in an environment variable.

## Vision in one screen

```toml
# ~/.config/omma/config.toml
[vision]
enabled = true
endpoint = "https://api.inference.wandb.ai/v1/chat/completions"
model = "zai-org/GLM-5.3-Flash"
allow_remote = true             # the explicit yes to sending an image off-machine
api_key_env = "WANDB_API_KEY"   # the key lives in the environment, not in this file
max_tokens = 2500               # reasoning models think before they answer
```

```sh
export WANDB_API_KEY="<your-key>"
omma screenshot.png --ask "Describe this screen and what each control does"
```

For a local model, drop `allow_remote`, point `endpoint` at
`http://127.0.0.1:11434/v1/chat/completions` and set `model` to yours. Setup,
model choices and the token-budget trap: [docs/vision.md](docs/vision.md).

## Compatibility

Verified by running the suite natively on each platform (GitHub Actions: four
platform jobs plus an MSRV job):

| Platform | Runner | Tesseract | Tests |
|---|---|---|---|
| Ubuntu x86_64 | `ubuntu-latest` | 5.5.0 (apt) | 101 passed |
| macOS arm64 | `macos-latest` | 5.5.4 (brew) | 101 passed |
| macOS x86_64 | `macos-15-intel` | 5.5.4 (brew) | 101 passed |
| Windows x86_64 | `windows-latest` | 5.5.x (choco) | 101 passed |
| Rust 1.89.0 | `ubuntu-latest` | 5.5.x (apt) | build + all test targets compile |

All jobs also run formatting, Clippy at `-D warnings`, a release build, and smoke
tests of that release binary (human report, JSON contract, region crop, stdin,
exit codes, and a full MCP exchange including an inline image).

Other targets — Linux aarch64, Windows ARM, the BSDs — are untested and
unclaimed. Images supported: PNG, JPEG, WebP, BMP, GIF (first frame; animation
detected), TIFF. SVG and other XML markup are refused.

## What was actually run

Real vision, GLM-5.3-Flash through W&B Inference, four images through the release
binary:

| Image | Result |
|---|---|
| Bar chart, 800×500 | chart type, title, all four labelled values (30/50/40/75), axis range and gridlines |
| Architecture diagram, 820×420 | all six boxes with labels and fills, and every arrow with its direction |
| Desktop screenshot, 1600×900 → 1280×720 | status bar, window, session id, the visible config lines, the highlighted message, the input prompt |
| 4K artwork → 1280×720 | scene and composition, with observation separated from inference |

Latency 3.4–8.8 s per image; 161–730 reasoning tokens; 24 KB–1.7 MB per request.
The MCP `image_base64` path was exercised against the same provider and returned
the chart values correctly with `image.filename: "inline"`.

Also verified: 101 automated tests cover the request shape (system + user
messages, data URI, bearer token, `reasoning_effort` only when configured),
downscaling and size ceilings, and every failure path — 401/403, 429, timeout,
unreachable, non-JSON, empty content, redirects, truncation by token budget. The
API key is asserted absent from JSON, stderr and `doctor` on success and failure.

Performance (this machine): 3840×2160 PNG, decode + metadata + palette + grid
(`--no-ocr`), 10 warm runs: 98–101 ms, peak RSS 33 MB; OCR takes ~0.4 s.

## Security

An adversarial audit ran before this release; five defects it confirmed are fixed
here, each with a regression test:

- **Vision**: the loopback check accepted any host starting with `127.`, so
  `127.0.0.1.attacker.example` was treated as local and an image could leave the
  machine with `allow_remote = false`. Only `localhost`, `::1` and a parsed 127/8
  literal count as local now, and a remote endpoint must be `https://`.
- **MCP**: `image_path: "-"` made the server read its own stdin — the JSON-RPC
  channel — and wedge the request. It is refused with a tool error.
- **Text output**: file names, OCR text, region content and model answers were
  printed raw, so an escape sequence could drive the terminal. Control characters
  are replaced (newline and tab kept); JSON was already escaped.
- **CLI**: `--json --format text` printed a human report and exited 0. It is a
  usage error now.
- **Colour**: a fully transparent image reported `samples: 0` with
  `average_rgb: [0,0,0]`, which reads as black. It now carries the
  `colors_no_opaque_samples` warning.

Standing rules: the API key is read from an environment variable and sent only as
a bearer token, never written to JSON, logs, the doctor or an error message; a
remote endpoint needs `allow_remote` **and** https; redirects are never followed;
MCP paths are canonicalized and can be restricted to allowed directories with
symlink-escape protection; OCR text and model output are marked `untrusted`.

## Known limitations

- **Vision quality is the model's.** Local model integration is protocol-complete
  and verified against a mock server, but no local model was installed on the
  development machine — expect to test it against your own server.
- **Reasoning models need a bigger token budget.** GLM-5.3-Flash spent 1,341
  tokens thinking on one screenshot; with `max_tokens = 600` the answer came back
  empty and OMMA now says so instead of claiming there was no content.
- **OCR on photographs is unreliable by nature** — Tesseract reads texture as
  glyphs; OMMA reports that text, flags it and excludes it from layout regions
  rather than deleting the measurement.
- **Dense screens segment coarsely**: a full desktop is often one region.
- EXIF orientation is not applied; first frame only for animated images; no PDF,
  video, batch, QR/barcode or table extraction.
- Colour percentages are estimates from a 64×36 sample; straight-alpha resizing
  can darken cells that straddle a hard transparent edge.

## Licence

MIT OR Apache-2.0.
