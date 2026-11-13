# Contributing

Bug reports and focused pull requests are welcome. This is a small tool with a
narrow contract, so the most useful contributions are usually a failing test with
a real image, or a fix that adds no dependency.

## Before you start

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

All four run in CI on Linux, Windows, macOS (arm64 and x86_64) and Rust 1.89.
A pull request that changes behaviour needs a test that fails without it.

## House rules

- **No new dependencies without a reason in the PR.** Decoding, base64, colour,
  layout and HTTP are already covered; a dependency must replace more code than it
  adds.
- **The JSON contract is versioned.** Adding fields is fine; renaming, removing or
  changing the meaning of one is not, unless `schema_version` changes.
- **Exit codes are part of the interface**: 0 success/partial, 1 input, 2 usage or
  configuration, 3 a requested backend failed, 4 output, 5 MCP startup.
- **A backend that fails must not delete the rest of the report.** It becomes a
  warning (with a code) and the other subsystems still return.
- **Nothing silently leaves the machine.** A remote vision endpoint requires
  `allow_remote` and `https://`.
- **Secrets never enter the repository**: no keys in code, tests, fixtures,
  examples or logs. Tests use placeholders like `sk-omma-test-…` and assert the
  value never appears in output.
- **Text from an image is untrusted.** OCR and model output carry
  `untrusted: true` and are escaped for the terminal.
- Do not claim a behaviour is verified unless it was run — mock tests and real
  model runs are different evidence, and the docs say which is which.

## Style

Rust 2024, `rustfmt` defaults, `clippy -D warnings`. Comments explain *why*, not
*what*; a non-obvious limit or workaround gets a line beside it.

## Commits

One change per commit, imperative subject under ~70 characters, body explaining
the failure it prevents.

## Images for tests

Fixtures live in `tests/fixtures/`. Keep them tiny (a few KB) so the suite stays
fast; synthesize large images in the test instead of committing them.
