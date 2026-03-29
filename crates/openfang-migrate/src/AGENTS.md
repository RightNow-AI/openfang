<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-migrate — Import from Other Frameworks

## Purpose

Provides data migration tools for importing agents, memory, sessions, skills, and channel configurations from other agent frameworks (OpenClaw, and future support for LangChain, AutoGPT) into OpenFang.

## Key Files

| File | Purpose |
|------|---------|
| `lib.rs` | Main API — `MigrateSource` enum, `MigrateOptions`, `run_migration()` |
| `openclaw.rs` | OpenClaw importer — agent YAML parsing, memory/session/skill import |
| `report.rs` | `MigrationReport` — migration results, statistics, warnings |

## For AI Agents

**When to read:** Understand data migration, importing legacy agent definitions, or adding support for new frameworks.

**Key interface:**
- `MigrateSource` — enum of supported frameworks
- `MigrateOptions` — source dir, target dir, dry-run flag
- `run_migration()` — entry point for running migrations
- `MigrationReport` — results including counts and warnings

**Supported frameworks:**
- OpenClaw (fully supported)
- LangChain (stub, not yet implemented)
- AutoGPT (stub, not yet implemented)

**Common tasks:**
- Importing from OpenClaw → call `run_migration()` with `MigrateSource::OpenClaw`
- Adding new framework support → implement handler in `lib.rs` match statement + new module
- Running dry-run → set `MigrateOptions { dry_run: true }`

**Architecture note:** Each framework handler is isolated in its own module for maintainability.
