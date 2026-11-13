## What this changes

<!-- One or two sentences. What behaviour is different after this? -->

## Why

<!-- The failure this prevents, or the gap it closes. Link an issue if there is one. -->

## Verification

<!-- What did you run? Paste the command, not a claim. -->

```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

- [ ] A test fails without this change (behaviour changes only)
- [ ] JSON fields added, not renamed or removed (otherwise `schema_version` moves)
- [ ] No secret, token or personal path appears in code, tests, fixtures or logs
- [ ] Docs updated where the behaviour changed
- [ ] Nothing here is claimed as verified that was only tested against a mock
