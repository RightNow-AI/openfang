# OpenFang — Agent Instructions

## Project Overview
OpenFang is a Rust workspace for the local daemon, API server, runtime, memory, hands, skills, channels, and desktop embedding.

- Config: `~/.openfang/config.toml`
- Default API: `http://127.0.0.1:4200`
- Local binary: `target/release/openfang` or `target/debug/openfang`
- Windows builds may use `openfang.exe`

## Default Acceptance Gate
Validate locally by default.

- Do not inspect or rely on remote CI/GitHub Actions unless the user explicitly asks.
- Treat local build, test, lint, and required local smoke checks as the acceptance gate.

Run these after code changes:

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

If a daemon is already running, also run:

```bash
scripts/preflight-openfang.sh
```

## Live Verification
Run local live verification after API routes, config loading, daemon wiring, runtime integration, or dashboard wiring changes. Unit tests alone are not enough for these paths.

Default local path:

```bash
cargo build --release -p openfang-cli --locked
```

Then start a fresh local daemon, confirm `GET /api/health`, and run:

```bash
scripts/smoke-openfang.sh
OPENFANG_API_KEY=<key> scripts/live-api-smoke-openfang.sh http://127.0.0.1:4200
```

Only require a real provider canary when the change touches provider wiring, real LLM message paths, metering, or the user explicitly asks for it.

The daemon command is `start`, not `daemon`.

## Architecture Notes
- Avoid touching `openfang-cli` unless the user asks for it or there is no smaller fix.
- `KernelHandle` prevents circular dependencies between runtime and kernel.
- New HTTP routes must be wired in both `crates/openfang-api/src/server.rs` and `crates/openfang-api/src/routes.rs`.
- Dashboard changes usually need both HTML and JS/state wiring.
- Config additions need schema coverage, serde defaults where appropriate, and `Default` impl coverage.

## Common Gotchas
- If the daemon is running, `cargo build --workspace --lib` avoids binary lock issues.
- `PeerRegistry` is `Option<PeerRegistry>` on kernel but `Option<Arc<PeerRegistry>>` on `AppState`; use `.as_ref().map(|r| Arc::new(r.clone()))`.
- `AgentLoopResult` uses `.response`, not `.response_text`.
