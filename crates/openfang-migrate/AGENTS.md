<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-migrate

## Purpose
Migration engine for importing agents, memory, sessions, skills, and channel configurations from other agent frameworks (OpenClaw, with LangChain and AutoGPT planned) into OpenFang. Handles parsing legacy workspace layouts, performing dry-run analysis, and generating detailed migration reports.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Core API: `MigrateSource` enum, `MigrateOptions`, `run_migration()`, and `MigrateError` types. |
| `src/openclaw.rs` | OpenClaw JSON5 parser and migration logic. Handles `~/.openclaw/openclaw.json` and related workspace structure. |
| `src/report.rs` | Migration report generation with item tracking (`MigrateItem`, `SkippedItem`, `MigrationReport`). |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Core migration logic and error handling. |
| `tests/` | Integration tests for migration workflows. |

## For AI Agents

### Working In This Directory
- Read `src/lib.rs` first to understand the public API and error types.
- OpenClaw migration is the only implemented source; new sources (LangChain, AutoGPT) follow the same `MigrateSource` → handler pattern.
- `MigrateOptions` provides `dry_run` flag — use this in tests to avoid side effects.
- Workspace layout parsing uses `walkdir` for recursive traversal and `serde_json`/`serde_yaml` for config parsing.
- All file I/O errors are wrapped in `MigrateError::Io`.

### Testing Requirements
- Test dry-run mode: verify report is generated without filesystem mutations.
- Test OpenClaw parsing: verify agents, skills, and channel configs are extracted correctly.
- Test error handling: invalid paths, malformed JSON5, missing configs should return appropriate `MigrateError` variants.
- Use `tempfile` crate for safe temporary workspace creation in tests.

### Common Patterns
- `MigrateSource` switch statement in `run_migration()` routes to handler functions.
- OpenClaw paths follow `~/.openclaw/` (or legacy names like `~/.clawdbot`).
- Reports accumulate both successful `MigrateItem`s and `SkippedItem`s for user transparency.
- Errors use `thiserror` with Display implementations for CLI output.

## Dependencies

### Internal
- `openfang-types` — shared types for agents, config, and events.

### External
- `serde`/`serde_json`/`serde_yaml`/`json5` — multi-format config parsing.
- `toml` — output format for OpenFang configs.
- `thiserror` — error type derivation.
- `tracing` — structured logging (info, warn, error).
- `walkdir` — recursive directory traversal for workspace discovery.
- `chrono` — timestamps in migration reports.
- `uuid` — unique identifiers for migrated agents.
- `dirs` — platform-specific home directory resolution.

<!-- MANUAL: -->
