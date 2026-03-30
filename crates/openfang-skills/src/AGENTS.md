<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-skills/src

## Purpose

Core skill system for OpenFang: registry, loader, and execution engine. Bundles 60 domain-specific skill prompts (ansible, docker, python-expert, etc.), lazy-loads them on demand, provides skill discovery and composition, and supports marketplace updates via ClawHub.

## Key Files

| File | Description |
|------|-------------|
| `lib.rs` | Core types: `Skill`, `SkillRegistry`, `SkillInfo`, error types. Public API. |
| `registry.rs` | Skill Registry: loads bundled skills, caches them, provides query API (`list_all()`, `get_by_id()`, `search()`), composition support. |
| `bundled.rs` | Compile-time embedded SKILL.md files (60 skills) loaded as const data. |
| `loader.rs` | SKILL.md parser: extracts YAML frontmatter (name, description), body content, validates structure. |
| `clawhub.rs` | Marketplace integration: fetches skill updates from ClawHub, version tracking, upgrade flow. |
| `marketplace.rs` | Marketplace API: search/filter skills, ratings, download counts, user reviews. |
| `openclaw_compat.rs` | OpenClaw format compatibility: converts between OpenClaw skill format and OpenFang format. |
| `verify.rs` | Skill verification: validates frontmatter structure, content coherence, detects common errors. |

## For AI Agents

### Working In This Directory

- `SkillRegistry` is the main API: call `load_bundled()` to initialize, then query via `get_by_id()`, `search()`, `list_all()`.
- Each skill is identified by ID (directory name in `bundled/`): `get_by_id("docker")` returns the Docker skill.
- Skills are lazy-loaded on first access; subsequent accesses use an in-memory cache.
- `Skill` struct contains: ID, name, description (frontmatter), content (full SKILL.md body), metadata (tags, category).
- Skill content is served directly to agents in system prompts via `agent.system_prompt + skill.content`.
- ClawHub marketplace queries are optional; bundled skills are always available offline.

### Testing Requirements

- Test registry: verify bundled skills load, cache works, `get_by_id()` returns correct content.
- Test loader: parse SKILL.md frontmatter, verify YAML is valid, content is non-empty.
- Test search: filter by name, category, tags; verify results match query.
- Test composition: combine 2+ skills in a single system prompt, verify no conflicts.
- Test marketplace: mock ClawHub API, verify updates fetch and merge correctly.
- Test verify: detect malformed frontmatter, missing descriptions, empty content.

### Common Patterns

- Skill ID is always lowercase with hyphens (e.g., "python-expert", "code-reviewer").
- Frontmatter is YAML: `name: <string>`, `description: <string>`.
- Skill content starts after frontmatter with `# Title` heading.
- Bundled skills are always available; marketplace skills are optional upgrades.
- Skill caching is transparent; registry handles cache invalidation on updates.

## Dependencies

### Internal
- `openfang-types` — error types, config structures

### External
- **Parsing:** `serde_yaml` — frontmatter parsing, `regex` — content extraction
- **Data:** `serde`/`serde_json` — skill serialization
- **HTTP:** `reqwest` — ClawHub API calls (optional feature)
- **Async:** `tokio` — concurrent skill loads
- **Utilities:** `once_cell` / `lazy_static` — caching, `thiserror` — error types, `tracing` — logging

<!-- MANUAL: -->
