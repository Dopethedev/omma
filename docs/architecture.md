# Architecture

## Shape

```
src/
├── main.rs            argument dispatch, exit codes
├── lib.rs             modules, logging setup
├── cli.rs             clap surface: `omma IMAGE` and `omma analyze IMAGE`
├── config.rs          defaults < file < environment < CLI
├── error.rs           one error type, stable exit-code mapping
├── input.rs           read, sniff, decode under limits, crop, allowed roots
├── report.rs          the serializable report: one shape, one version
├── analysis/
│   ├── mod.rs         the engine: stage timing, subsystem selection, assembly
│   ├── colors.rs      sampling, quantization, palette, statistics, grid
│   ├── ocr.rs         tesseract subprocess, passes, TSV hierarchy
│   └── layout.rs      text regions, position labels, reading order
├── vision.rs          OpenAI-compatible provider, endpoint policy, base64
├── output/
│   ├── mod.rs         pick a presentation
│   ├── json.rs        compact or pretty JSON
│   └── text.rs        the human report
├── doctor.rs          backends, configuration, limits
└── mcp.rs             stdio MCP server, two tools
```

The library is synchronous and knows nothing about the CLI or MCP. `mcp.rs` owns
the only async runtime (a current-thread Tokio runtime) and runs analysis on the
blocking pool.

## Pipeline

1. **Read.** A path, or `-` for stdin. Size is checked before the read; stdin is
   read through a capped reader. The format is guessed from the bytes.
2. **Decode.** `image::ImageReader` with `image::Limits` set from configuration
   (maximum width/height, maximum allocation) and an explicit pixel-count check.
   SVG/XML is refused by sniffing the first non-whitespace byte.
3. **Crop.** `--region X,Y,W,H` is validated against the decoded dimensions and
   applied before any analysis, so every coordinate in the report refers to the
   analyzed frame; the source dimensions are preserved in `image` and the offset
   in `region`.
4. **Colour.** One bicubic (`CatmullRom`) downsample to the grid size, and one to
   64×36 for the palette (reused when the grid is already 64×36). Pixels below
   alpha 8 are dropped; partial alpha is composited over white. Channels are
   quantized by keeping the top 5 bits, counted, and ranked by count then colour
   value — deterministic, no clustering, no seeds.
5. **OCR.** Tesseract is located on `PATH`; its version and language packs are
   read. Small crops (height < 120) are upscaled 4× with Lanczos and greyscaled,
   matching the original prototype. Passes at `--psm 6` and `--psm 11` run on the
   same prepared image; the longer text wins; that pass is then re-run with the
   `tsv` config to obtain structure. Boxes from the upscaled frame are divided
   back into analyzed pixels. The image is fed over stdin, so no temporary file
   is ever written.
6. **Layout.** One region per Tesseract text block that qualifies as text (two or
   more alphanumeric characters, confidence at or above `ocr.min_confidence`):
   measured box, position label from the box centre in thirds, its confidence and
   reading order. Images without text produce an empty array, not guesses; blocks
   that fail the rule stay in `ocr.blocks` and are counted in a warning.
7. **Vision** (optional). Only when asked. One POST to the configured endpoint
   with a compact JSON body containing the analyzed image as a PNG data URI.
   Inline redirects are disabled, so a local endpoint cannot bounce the upload
   elsewhere.
8. **Assemble.** Warnings accumulate per subsystem; a failure in one never erases
   another's results. Stage durations are recorded.

## Subsystem independence

Optional backends fail into `warnings[]` and leave their own field `null`:

| Failure | Result |
|---|---|
| Tesseract missing | `ocr: null`, warning `ocr_unavailable` |
| Language pack missing | that language dropped, warning `ocr_language_missing`; `ocr: null` if none remain |
| OCR timeout | warning `ocr_timeout`; text kept if any pass finished |
| Vision unreachable | `vision: null`, warning `vision_failed` |
| Vision misconfigured | configuration error before any work (exit 2) |

Exit code 3 is reserved for a backend that was *explicitly* named (`--ask`,
`--ocr`) and produced nothing; the report is still written first.

## Determinism

Given the same input and configuration, everything except timings is identical
between runs: no threading in the analysis path, no randomness, no hash-map
iteration order in output (a `BTreeMap` plus an explicit sort), fixed sampling
kernel, fixed rounding.

## Dependencies

`image` (decode), `clap` (CLI), `serde`/`serde_json` (report), `toml`
(configuration), `thiserror` (errors), `rmcp` (MCP, pulls Tokio), `ureq`
(one synchronous HTTP call), `tracing`/`tracing-subscriber` (stderr logging).
No OCR library, no OpenCV, no async runtime in the core.

## Performance notes

Measured on an i5-12400 / 15 GiB / Arch Linux with rustc 1.99.1, release build
(thin LTO, one codegen unit, stripped), 3840×2160 PNG, warm cache, 10 runs:

| Workload | Result |
|---|---|
| decode + metadata + palette + grid (`--no-ocr`) | 88.9–105.8 ms, median 90.7 ms |
| peak RSS for the same run | 33 MB |

Colour analysis is one bicubic resize to a tiny target plus a linear pass — it is
not the bottleneck. OCR is: each full run is three Tesseract invocations
(~0.4–1 s for a screenshot). Nothing is parallelised, because nothing has
profiled as needing it.
