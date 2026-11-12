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
- [x] Tests pass: 68 tests (0 failed) with Tesseract installed; OCR tests skip with
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

## 10. Packaging and artifacts

- [x] `cargo package --list` includes sources, docs, examples, tests and fixtures
      (44 files, 324 KiB; 90.8 KiB compressed) and excludes `target/` and `dist/`.
- [x] `cargo publish --dry-run --allow-dirty` packages and compiles the crate
      successfully; the upload is aborted by the dry run. Not published.
- [x] No personal paths, secrets or hostnames in the packaged tree.
- [x] Missing `repository`/`homepage` in `Cargo.toml` is a warning only: there is
      no public remote, and inventing a URL would be a false claim. Set both when
      the project gets a real home.
- [x] Clean-room build: the tree copied to a fresh directory, built with
      `--locked --offline` and a separate target directory, produced a
      **bit-identical** binary (same SHA-256). `cargo test` there: 68 passed.

Artifacts (in `dist/`, git-ignored, not part of the crate):

```
dist/omma-0.1.0-x86_64-linux-gnu          7,440,928 bytes, stripped
dist/omma-0.1.0-x86_64-linux-gnu.sha256
sha256 9f0bc76f59c890b712cc3251c723d91ad506d7d4585b39785781360cbb500db2
```

- [x] Checksum verifies (`sha256sum -c`).
- [x] Artifact binary, `target/release/omma` and the installed
      `~/.local/bin/omma` are the same file by hash.
- [x] Installed into an isolated prefix and run with a clean environment
      (`env -i`, temp `HOME`, only that prefix on `PATH`): `--version` and
      `doctor` both work.
- [x] Dynamically linked: requires **glibc ≥ 2.39**. Older distributions must
      build from source; there is no static/musl artifact (no musl toolchain is
      installed and adding one was not justified for this release).
- [x] Release notes: `docs/release-notes-0.1.0.md` — highlights, install,
      checksum, compatibility, testing performed, known limitations.

## 11. Release

```sh
git add -A && git commit -m "omma 0.1.0"   # needs the owner's word
git tag -a v0.1.0 -m "omma 0.1.0"
cargo publish                              # if publishing to crates.io
```

- [ ] Commits, tags and publishing are owner actions; nothing is committed,
      tagged or published by the development pass.
