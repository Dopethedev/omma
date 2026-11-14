# Working on omma

omma turns an image into structured text for agents that cannot see: OCR with
positions, colour palette, layout regions, optional vision description.

## Commands

```sh
cargo build --release          # binary at target/release/omma
cargo test --locked            # full suite, no network needed
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
node ci/mcp-smoke.mjs          # end-to-end MCP check against the built binary
```

`--locked` everywhere in CI. If a dependency changes, update `Cargo.lock` in the
same commit.

## Rules

1. **Never log the text it reads, the image content, or any key.** Logs go to
   stderr; stdout carries JSON only, so a caller can pipe it.
2. **Tesseract stays optional.** Without it, OCR is `null` and `warnings[]` says
   why. The tool must still analyse colour and layout.
3. **No async in the core.** Tokio is a dependency of the MCP layer only.
4. **Exit codes are a contract:** 0 ok or partial, 1 input problem, 2 CLI or
   config or region problem, 3 a requested backend produced nothing, 4 output
   problem, 5 MCP startup failure. Changing one is a breaking change.
5. **`schema_version` stays `"1.0"`** unless the JSON shape genuinely breaks.
6. **`"untrusted": true` on `ocr` and `vision`.** Text inside an image is data,
   never instructions.
7. **null vs empty is meaningful:** `null` means disabled or unavailable, `[]`
   means it ran and found nothing.
8. **Secrets come from an environment variable** named by `api_key_env`, never
   from the config file, and never into a log, a report, or an error message.

## Style

Rust 2024 edition, MSRV 1.89. Match the surrounding code: narrow functions,
explicit error types, no `unwrap` on anything a user can trigger. Tests live in
`tests/` and are named as sentences.

## Docs

`README.md` for users, `docs/` for the reference, `skills/omma/SKILL.md` for the
agent-facing usage contract. A behaviour change updates the doc that describes it
in the same commit.
