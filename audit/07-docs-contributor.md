# Dimension 7 — Docs & Contributor Alignment

**Auditor:** Copilot CLI
**Date:** 2025-07-18
**Scope:** README.md, CLAUDE.md, CONTRIBUTING.md, CHANGELOG.md, MIGRATION.md, SECURITY.md, docs/, root Cargo.toml

---

## Summary

Documentation is extensive and well-structured — a new contributor can understand the architecture, build, test, and submit a PR using the provided docs. However, there are **significant inconsistencies between documents** where numerical claims (test counts, tool counts, endpoint counts, provider counts, version numbers) diverge across README.md, CLAUDE.md, CONTRIBUTING.md, CHANGELOG.md, and docs/README.md. The license statement in README.md also conflicts with the actual dual-license declared in Cargo.toml. The core onboarding path (clone → build → test → PR) is clear and accurate. All internal file links and image assets resolve correctly.

**Actual test count (verified):** 1,796 passing across all workspace crates.

---

## Findings

### D1 — Version Badge Mismatch (Severity: High)

**Location:** `README.md:23` vs `Cargo.toml:21`

README badge displays `v0.1.0` and the stability notice references `v0.1.0`, but `Cargo.toml` workspace version is `0.1.7`. External visitors see a stale version. The `CHANGELOG.md` only documents `[0.1.0]` — versions 0.1.1 through 0.1.7 have no changelog entries.

**Recommendation:** Update the README badge to `0.1.7` (or the current version). Add changelog entries for intermediate versions, or keep the badge dynamic via a CI-generated shield.

---

### D2 — Test Count Inconsistency Across Documents (Severity: Medium)

**Locations & claims:**

| Document | Claimed Count |
|----------|--------------|
| `README.md:9` | 1,767+ |
| `README.md:23` (badge) | 1,767+ |
| `README.md:359` | 1,767+ |
| `CLAUDE.md:13` | 1,744+ |
| `CONTRIBUTING.md:65` | 1,744+ |
| `CONTRIBUTING.md:325` | 1,744+ |
| `CHANGELOG.md:159` | 1,731+ |
| `docs/README.md:82` | 967 |

**Actual:** 1,796 tests pass (`cargo test --workspace`).

All "X+" claims are technically valid since the actual count exceeds them, but four different numbers across documents signals poor synchronization. `docs/README.md` at 967 is severely stale — roughly half the actual count.

**Recommendation:** Standardize on a single source of truth. Either keep one manually-updated number, or generate a badge from CI. Remove the "967" from docs/README.md immediately.

---

### D3 — Tool Count Inconsistency (Severity: Medium)

| Document | Claimed Count |
|----------|--------------|
| `README.md:236` | 53 tools |
| `CONTRIBUTING.md:127` (architecture table) | 38 built-in tools |
| `CHANGELOG.md:17` | 41 built-in tools |
| `docs/README.md:76` | 38 |

Three different numbers for the same metric. The README number (53) may include MCP/A2A tools, while 38 may be pure built-in tools, but this isn't clarified anywhere.

**Recommendation:** Settle on a clear definition (e.g., "53 tools total: 38 built-in + 15 via MCP/A2A") and apply it consistently.

---

### D4 — API Endpoint Count Inconsistency (Severity: Medium)

| Document | Claimed Count |
|----------|--------------|
| `README.md:239, 311` | 140+ |
| `CONTRIBUTING.md:130` | 76 |
| `CHANGELOG.md:66` | 100+ |
| `docs/README.md:38, 78` | 76 |

The docs and CONTRIBUTING agree on 76, while README claims 140+. Either the endpoint count grew without updating the docs, or README over-counts (e.g., including sub-paths, WebSocket channels, or SSE streams as separate endpoints).

**Recommendation:** Audit the actual router registrations in `server.rs` and document the authoritative count. Use a consistent definition across all files.

---

### D5 — License Statement Mismatch (Severity: High)

**Location:** `README.md:386` vs `Cargo.toml:24`

- **README.md** states: `MIT — use it however you want.`
- **Cargo.toml** declares: `license = "Apache-2.0 OR MIT"`
- Both `LICENSE-MIT` and `LICENSE-APACHE` files exist in the repository root.

The README omits the Apache-2.0 option entirely, which could mislead downstream users about their licensing obligations and rights.

**Recommendation:** Update the README license section to: `Dual-licensed under MIT and Apache 2.0 — choose whichever suits your project.` Update the license badge accordingly.

---

### D6 — Migration CLI Flag Mismatch (Severity: Medium)

**Location:** `README.md:287` vs `MIGRATION.md:31` vs actual CLI

- **README.md:** `openfang migrate --from openclaw --path ~/.openclaw`
- **MIGRATION.md:** `openfang migrate --from openclaw --source-dir /path/to/openclaw/workspace`
- **Actual CLI** (`crates/openfang-cli/src/main.rs:312`): `--source-dir`

The README uses `--path` which is not a valid CLI flag. A new user copy-pasting from README would get an error.

**Recommendation:** Fix README to use `--source-dir`.

---

### D7 — CHANGELOG Crate Count Error (Severity: Low)

**Location:** `CHANGELOG.md:13`

CHANGELOG states "15-crate Rust workspace" but the workspace has 14 members (13 openfang-* crates + xtask). All other documents correctly say 14.

**Recommendation:** Change "15-crate" to "14-crate" in CHANGELOG.md.

---

### D8 — LLM Provider Count Inconsistency (Severity: Low)

| Document | Claimed Count |
|----------|--------------|
| `README.md:268` | 27 providers, 123+ models |
| `docs/README.md:32, 77-78` | 20 providers, 51 models |

The README number is significantly higher. Either the provider list expanded without updating docs, or README counts aliases/sub-providers differently.

**Recommendation:** Reconcile. Document which providers are "native" vs. "routed-through" to explain any counting methodology.

---

### D9 — Orphaned Documentation File (Severity: Low)

**Location:** `docs/launch-roadmap.md`

This file exists in the `docs/` directory but is not linked from `docs/README.md` or any other document. It is invisible to contributors browsing the docs index.

**Recommendation:** Either link it from docs/README.md under a "Planning" section, or remove it if it's no longer relevant.

---

### D10 — docs/README.md Architecture Description Stale (Severity: Low)

**Location:** `docs/README.md:20`

The architecture link description says "12-crate structure" but the actual architecture doc (`docs/architecture.md:27`) correctly states 14 crates. The docs/README.md index is stale.

**Recommendation:** Update the description from "12-crate" to "14-crate".

---

### D11 — CLAUDE.md Deviates From README on Test Count (Severity: Low)

**Location:** `CLAUDE.md:13` vs `README.md:359`

CLAUDE.md says "currently 1744+" while README says "1,767+". CLAUDE.md is the AI agent instruction file — an agent following it will quote 1,744+ while the README says 1,767+. Neither matches the actual 1,796.

**Recommendation:** Since CLAUDE.md is injected as system prompt context for AI agents, it should match the README or use a deliberately vague claim like "1,700+ tests".

---

## Strengths

1. **Clear onboarding path.** Clone → `cargo build` → `cargo test` → `cargo clippy` → PR is documented in README, CONTRIBUTING, and CLAUDE.md. A new contributor can go from zero to running tests in under 5 minutes.

2. **Comprehensive CONTRIBUTING.md.** Includes step-by-step guides for three common contribution types (agent template, channel adapter, built-in tool), code style conventions, PR process, and commit message format. This is significantly better than most open-source projects.

3. **Rich docs/ directory.** 17 documentation files covering architecture, API reference, configuration, security, channel adapters, CLI reference, providers, skills, MCP/A2A, desktop app, production checklist, and troubleshooting. All internal links from docs/README.md resolve correctly.

4. **All image assets resolve.** `public/assets/openfang-logo.png`, `openfang-vs-claws.png`, and `rightnow-logo.webp` all exist.

5. **Thorough MIGRATION.md.** Detailed tool name mapping table, provider mapping, config format comparison, and manual migration steps. Covers edge cases and troubleshooting.

6. **SECURITY.md follows best practices.** Clear reporting instructions, responsible disclosure process, SLA commitments, and a comprehensive scope definition. Security architecture is well-documented with dependency audit table.

7. **Architecture docs match reality.** The crate dependency graph in docs/architecture.md and CONTRIBUTING.md matches the actual workspace members in Cargo.toml (14 members).

8. **CLAUDE.md provides actionable agent context.** Includes live integration testing workflow, common gotchas, and key API endpoints — essential for AI-assisted development.
