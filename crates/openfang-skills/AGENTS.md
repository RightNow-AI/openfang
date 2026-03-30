<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-skills

## Purpose

The skill system is the pluggable tool layer for OpenFang. Skills are tool bundles that extend agent capabilities. They can be TOML + Python/WASM/Node.js/Shell scripts, bundled at compile time (60 skills ship with OpenFang), downloaded from ClawHub marketplace, or converted from OpenClaw format. The registry loads, verifies, and manages installed skills. Skills declare their requirements (built-in tools, capabilities), provide tool definitions as JSON schemas, and can be prompt-only (context injected into LLM system prompt).

## Key Files

| File | Description |
|------|-------------|
| `src/lib.rs` | Core types: `SkillManifest`, `SkillRuntime` (Python/Wasm/Node/Shell/Builtin/PromptOnly), `SkillSource`, `InstalledSkill`. |
| `src/registry.rs` | `SkillRegistry` — loads, installs, enables/disables skills. Tracks manifest, path, enabled state. |
| `src/loader.rs` | Dynamic loading: discovers skills from filesystem, parses TOML manifests, resolves runtime types. |
| `src/bundled.rs` | Bundled skill loader — 60 compiled-in skills extracted from `bundled/` at runtime. |
| `src/marketplace.rs` | ClawHub integration — search, fetch, rate limiting. |
| `src/clawhub.rs` | ClawHub client — REST API, version resolution, caching. |
| `src/openclaw_compat.rs` | OpenClaw format migration — convert Node.js skill bundles to OpenFang format. |
| `src/verify.rs` | Skill verification — manifest validation, signature checking, security checks. |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `bundled/` | 60 pre-built skill definitions (TOML + source files). Each skill is a subdirectory with `skill.toml` + runtime files. Examples: web-search, code-exec, file-ops, math-solver. |
| `src/` | Registry, loader, marketplace, verification, and bundled skill logic. |

## For AI Agents

### Working In This Directory

- **Adding a new skill**: Create subdirectory in `bundled/` with `skill.toml` (metadata, runtime config, tool definitions) + runtime files (`.py`, `.wasm`, etc).
- **Extending registry**: Modify `registry.rs` — `SkillRegistry::load()`, `install()`, `enable()`, `disable()` are the main APIs.
- **Marketplace integration**: Update `clawhub.rs` for new ClawHub API endpoints or caching strategies.
- **Verification**: Add checks to `verify.rs` for new security policies (e.g., require signed manifests).
- **OpenClaw migration**: Extend `openclaw_compat.rs` to handle new Node.js skill formats.

### Testing Requirements

- Unit tests live alongside each module.
- Test bundled skill loading: parse manifests, verify runtime detection.
- Test registry ops: install, enable, disable, list, query.
- Test marketplace: mock ClawHub responses, verify rate limiting.
- Test OpenClaw conversion: sample Node.js skill bundles.
- No live integration tests needed (marketplace is mocked).

### Common Patterns

- All skill paths are relative to `~/.openfang/skills/` or bundled into the binary.
- Manifests use `#[serde(default)]` for optional fields (runtime type defaults to PromptOnly).
- Tool input schemas are full JSON Schema objects (required by LLM tools).
- `SkillSource` tracks provenance (Native/Bundled/OpenClaw/ClawHub).
- Skills declare `requirements.tools` (e.g., `["web_fetch"]`) and `requirements.capabilities` (e.g., `["NetConnect(*)"]`).

## Dependencies

### Internal

- `openfang-types` — shared types (AgentId, etc).

### External

- `serde`, `toml`, `serde_json` — manifest parsing.
- `tokio`, `async-trait` — async registry operations.
- `reqwest` — ClawHub HTTP client.
- `sha2`, `hex` — skill verification/signing.
- `walkdir` — filesystem discovery.
- `uuid`, `chrono` — metadata.

<!-- MANUAL: -->
