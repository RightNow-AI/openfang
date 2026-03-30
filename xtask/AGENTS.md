<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# xtask

## Purpose
Cargo xtask build automation — for custom build, test, and release tasks run via `cargo xtask <task>`.

## Key Files
| File | Description |
|------|-------------|
| `Cargo.toml` | xtask binary crate definition |
| `src/main.rs` | Task implementations (currently empty) |

## For AI Agents

### Working In This Directory
- xtask runs in the workspace root context — can access all crates via cargo metadata.
- Common tasks: automated testing, release packaging, version bumping, changelog generation, platform-specific builds.
- Invoke tasks via `cargo xtask <task_name>` — arguments passed to main.
- Before adding a task, verify it's not already handled by a cargo hook or CI workflow.
- Keep tasks focused — one logical operation per task.

<!-- MANUAL: -->
