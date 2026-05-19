## Summary

<!-- What does this PR do? Link related issues with "Fixes #123". -->

## Changes

<!-- Brief list of what changed. -->

## Pre-PR checks (run locally before opening)

Run the full gate locally before opening the PR — CI is strict and Windows clippy in particular catches things macOS/Linux clippy skips via `#[cfg]`.

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo audit` (no new vulns; warnings reviewed)
- [ ] If touching cross-platform code: verify `#[cfg(unix)]` / `#[cfg(windows)]` gates on imports as well as items (Windows clippy fires `-D unused-imports` on imports only used by a gated `main`)

## Testing

- [ ] Live integration tested (if applicable)

## Security

- [ ] No new unsafe code
- [ ] No secrets or API keys in diff
- [ ] User input validated at boundaries
