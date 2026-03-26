# OpenFang — Local Fork Notes

## Repository Shape
This checkout has two layers:

1. OpenFang core under `crates/`, `docs/`, and root scripts
2. `projects/shipinbot/`, which provides the external `shipinfabu` Hand and the video publishing workflow

Treat the current checkout as the default local source of truth.

## Default Working Model
For shipinbot-facing work, start with:

- `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml`
- `projects/shipinbot/openfang-hand/shipinfabu/SKILL.md`
- `projects/shipinbot/docs/openfang-external-hand.md`
- `projects/shipinbot/scripts/sync_openfang_local_hands.py` for local install and sync

If the change touches Telegram media batches, keep the Rust bridge/types and the shipinbot manifest/bridge parser aligned.
Only check `projects/shipinbot/skills/clean-publish-copy-qc/SKILL.md` when the task explicitly involves copy or QA wording adjustments.

## Local Verification
Default to local acceptance only.

- Do not inspect or rely on remote CI/GitHub Actions unless the user explicitly asks.
- After code changes run:

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- If a daemon is already running, also run:

```bash
scripts/preflight-openfang.sh
```

- After API routes, config loading, daemon wiring, runtime integration, or dashboard wiring changes, also run local daemon smoke:

```bash
cargo build --release -p openfang-cli --locked
scripts/smoke-openfang.sh
OPENFANG_API_KEY=<key> scripts/live-api-smoke-openfang.sh http://127.0.0.1:4200
```

Only run a real provider canary when the change touches provider wiring, real LLM message paths, metering, or the user explicitly asks.

## Local Runtime Baseline
- Preferred maintainer loop is the current checkout plus the host-host local stack.
- From the parent repo root, use `scripts/local-stack.sh start|status|restart|stop`.
- Do not default to archived runtime copies or Docker unless the task is explicitly about those paths.

## Editing Boundaries
- Avoid touching `openfang-cli` unless the user asks for it or there is no smaller fix.
- If the task is shipinbot-facing and the user did not ask for backend internals, check Hand, sync, and docs before changing service code.
- Keep docs and implementation aligned when changing config, sync flow, or deployment assumptions.

## Useful Reminders
- The daemon command is `start`, not `daemon`.
- `KernelHandle` prevents runtime/kernel circular dependencies.
- New HTTP routes must be wired in both `crates/openfang-api/src/server.rs` and `crates/openfang-api/src/routes.rs`.
- `AgentLoopResult` uses `.response`, not `.response_text`.
- If the daemon is running, `cargo build --workspace --lib` avoids binary lock issues.
