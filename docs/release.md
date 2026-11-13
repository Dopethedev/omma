# Release checklist

What has to be true before a version goes out. Every line is a command or a check
that was actually run for 0.1.0, not an aspiration.

## 1. Engineering gate

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

- [x] Formatting clean.
- [x] Clippy clean at `-D warnings` for all targets and features.
- [x] Tests pass: 75 tests (0 failed) with Tesseract installed; OCR tests skip with
      a printed note when it is absent.
- [x] Release profile builds (thin LTO, one codegen unit, stripped).

## 2. Install and smoke test

```sh
install -m 755 target/release/omma ~/.local/bin/omma
omma --version && omma doctor
omma IMAGE --json | jq -e '.schema_version == "1.0"'
omma IMAGE            # human report
```

- [x] `omma` resolves on `PATH` and runs outside the checkout.
- [x] `doctor` reports the backend, configuration and limits without secrets.
- [x] Both CLI forms (`omma IMAGE`, `omma analyze IMAGE`) work.

## 3. Formats

- [x] PNG, JPEG, WebP, BMP, TIFF decode and OCR identically on the same source.
- [x] GIF detected as animated; first frame analyzed.
- [x] SVG and non-image bytes refused with a clear error (exit 1).
- [x] Format is taken from content: a PNG renamed `.bmp` still reports `png`.

## 4. Limits and error paths

- [x] Oversized file (40 MB) rejected before reading: exit 1, real byte count.
- [x] Oversized dimensions (20000×100) rejected with the image's real size.
- [x] Pixel-count limit rejects a small image under a tight configuration.
- [x] Truncated file: decode error, exit 1.
- [x] Permission denied: exit 1.
- [x] Directory path: exit 1.
- [x] No arguments: exit 2 with usage.
- [x] Bad region / bad flag / unknown config key: exit 2.
- [x] Missing Tesseract: run still succeeds, `ocr: null`, warning; explicit `--ocr`
      exits 3.

## 5. Vision (optional subsystem)

- [x] No request is made unless `--ask`/`--vision`/`vision: true` is given.
- [x] Local endpoint: request carries model, max_tokens, question and a PNG data URI.
- [x] Timeout honoured (2 s configured → abort, exit 3, warning).
- [x] HTTP 500 → exit 3 with the status in the warning.
- [x] Non-JSON response → exit 3 with a parse error.
- [x] Slow but valid response (1 s) returns the description.
- [x] Unreachable endpoint → exit 3, analysis still returned.
- [x] Non-loopback endpoint refused without `allow_remote`; credentials in a URL
      refused; redirects never followed.
- [ ] **Real local vision model: not verified** — none is installed on this
      machine. Do this before advertising the feature as tested:

      ```sh
      # e.g. Ollama; the model download is several GB and needs approval
      ollama pull qwen2.5vl:7b
      omma screenshot.png --ask "Describe this image." --model qwen2.5vl:7b
      ```

## 6. MCP

- [x] Server starts over stdio; stdout carries protocol frames only.
- [x] Handshake reports `omma` + capabilities.
- [x] `tools/list` exposes `analyze_image` and `omma_capabilities` with schemas and
      read-only/idempotent annotations.
- [x] `analyze_image` returns both text content and `structuredContent`.
- [x] Invalid arguments, missing files and unknown tools produce tool/protocol
      errors as specified, never crashes.
- [x] Allowed directories: inside allowed, outside denied, `/etc` denied, and a
      symlink inside the root pointing outside is denied.
- [x] Verified with the official MCP TypeScript SDK client and with a real agent
      (Control), which called both tools over the bridge.

## 7. Real images

- [x] Desktop screenshot (1920×1080, browser + chat): 1,919 chars, confidence 57.
- [x] Screenshot with small UI text (779×337): exact text, confidence 92.
- [x] Tiny clip (63×30): 4× upscale path, correct boxes in analyzed pixels.
- [x] Transparent PNG: transparent pixels excluded from the palette, grid cells null.
- [x] Multi-colour wallpapers (1080p, 1440p, 4K): palettes and grids coherent.
- [x] No-text image (wallpaper): empty text and regions, no warnings.
- [x] Diagram/chart: title, values and axis labels read; one merged region.
- [x] Photograph noise: flagged as low confidence, excluded from regions.

## 8. Documentation

- [x] README: install, CLI, JSON rules, MCP, vision, configuration, performance,
      limitations, security — all matching the shipped behaviour.
- [x] `docs/cli.md`, `docs/mcp.md`, `docs/json-schema.md`, `docs/configuration.md`,
      `docs/architecture.md` current.
- [x] `examples/config.toml` validates (`omma doctor --config examples/config.toml`).
- [x] `examples/mcp-config.json` is valid JSON.
- [x] SDK/agent compatibility claims limited to what was tested.

## 9. Licensing and versioning

- [x] `LICENSE-MIT` and `LICENSE-APACHE` present; `Cargo.toml` declares
      `MIT OR Apache-2.0`.
- [x] `version` is `0.1.0` in `Cargo.toml` (single source; the binary, the JSON
      `tool` object and the README example all read from it).
- [x] `schema_version` bumped only for incompatible JSON changes.
- [x] No secrets in the tree, in examples, or in logs.
- [x] `~/.local/bin/see` untouched (checksum verified).

## 10. Cross-platform CI

`.github/workflows/ci.yml`, on every push and pull request:

| Job | Runner | What it does |
|---|---|---|
| `test` | ubuntu-latest | fmt, clippy, full suite, release build, binary smoke tests, MCP exchange |
| `test` | macos-latest (arm64) | same |
| `test` | macos-15-intel (x86_64) | same |
| `test` | windows-latest | same |
| `msrv` | ubuntu-latest | build + compile all tests on the declared minimum (1.89.0) |

Two macOS architectures because the free-plan arm64 pool frequently evicts queued
jobs and `macos-13` is retired (macOS 14 images went away 2026-11-02), so an Intel
runner is the reliable second path. A job that never gets a runner must be rerun:
`gh run rerun <run> --job <job-id>`.

- [x] Tesseract installed per platform (apt / brew / choco) and reported in the log.
- [x] Formatting, Clippy (`-D warnings`) and the full suite pass on all three.
- [x] The release binary — not just the test harness — is exercised on each runner:
      human report, JSON contract, 4× upscale path, region crop, stdin, exit
      codes, and a complete MCP exchange where every stdout line must parse as a
      protocol frame.
- [x] Each runner packages `omma-<version>-<platform>` plus a `.sha256` and uploads
      it as a build artifact.
- [ ] Artifacts are build outputs, not a published release; attaching them to a
      GitHub release is an owner action.

## 11. Packaging and artifacts

- [x] `cargo package --list` includes sources, docs, examples, tests and fixtures
      and excludes `target/` and `dist/`.
- [x] `cargo publish --dry-run` packages and compiles the crate successfully; the
      upload is aborted by the dry run. Not published.
- [x] No personal paths, secrets or hostnames in the packaged tree.
- [x] `repository` points at the real remote; the earlier "no documentation,
      homepage or repository" warning is gone.
- [x] Clean-room build: the tree copied to a fresh directory, built with
      `--locked --offline` and a separate target directory, produced a
      **bit-identical** binary (same SHA-256). `cargo test` there: 75 passed.
- [x] Release artifacts are the CI-built binaries for the three tested platforms,
      each with a `.sha256` beside it; verify with `sha256sum -c` (or
      `shasum -a 256 -c` on macOS).
- [x] The Linux artifact installed into an isolated prefix and run under `env -i`
      with a temp `HOME` and only that prefix on `PATH`.
- [x] Linux artifact is dynamically linked and needs **glibc ≥ 2.39**; older
      distributions build from source (no musl toolchain is installed, and adding
      one was not justified for this release).
- [x] Release notes: `docs/release-notes-0.1.0.md` — highlights, install per
      platform, compatibility matrix, testing performed, known limitations.

## 12. Release

```sh
git add -A && git commit -m "omma 0.1.0"   # needs the owner's word
git tag -a v0.1.0 -m "omma 0.1.0"
cargo publish                              # if publishing to crates.io
```

- [ ] Commits, tags and publishing are owner actions; nothing is committed,
      tagged or published by the development pass.
