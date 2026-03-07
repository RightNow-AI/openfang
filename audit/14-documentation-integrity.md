# Dimension 14: Documentation Integrity

**Auditor:** Copilot  
**Date:** 2025-07-17  
**Scope:** README.md, CLAUDE.md, docs/, Cargo.toml workspace, crate lib.rs doc comments  

---

## Summary

Documentation quality is **generally strong** — lib.rs doc comments exist on all library crates, key traits are well-documented for implementors, and internal image links all resolve. However, several **quantitative claims are stale or inaccurate**: the LOC count is inflated by ~9K lines, the version badge is 7 patch versions behind, the license description omits the Apache-2.0 dual-license, and the test count claims differ between README.md and CLAUDE.md.

---

## Verification Table

| Claim | Source | Actual | Status |
|-------|--------|--------|--------|
| "14 crates" | README.md | 13 crate dirs + xtask = 14 workspace members | ✅ Accurate |
| "137K LOC" / "137,728 lines of code" | README.md | 128,946 Rust LOC / 166,075 all-source LOC | ❌ Neither metric matches |
| "1,767+ tests" | README.md | 1,797 test annotations | ✅ Accurate (1,797 ≥ 1,767) |
| "currently 1744+" | CLAUDE.md | 1,797 test annotations | ⚠️ Stale (accurate but inconsistent with README) |
| "v0.1.0" | README.md (badge + body) | Cargo.toml workspace version = 0.1.7 | ❌ 7 patch versions behind |
| "MIT — use it however you want" | README.md | Cargo.toml: "Apache-2.0 OR MIT"; both LICENSE files present | ❌ Omits Apache-2.0 dual-license |
| All internal links resolve | README.md | 3 image refs → all exist; 0 internal file links | ✅ All resolve |
| All internal links resolve | CLAUDE.md | 0 internal file/image links | ✅ N/A (no links to break) |
| Crate table matches workspace | README.md | 14 entries match 14 Cargo.toml members | ✅ Match |
| All lib.rs have doc comments | crates/*/src/lib.rs | 12/12 lib.rs files have //! doc comments (cli is bin-only) | ✅ All documented |
| Key traits documented | crates/**/*.rs | 8 pub traits found, all have doc comments | ✅ Good coverage |

---

## Findings

### DI-1: LOC Claim Overstated by ~9K Lines (Medium)

**Location:** README.md line 9, line 230  
**Claim:** "137K LOC" and "137,728 lines of code"  
**Actual:**
- Rust source only (crates/ + xtask/): **128,946 lines**
- All project source (Rust + TOML + HTML + JS + CSS + MD + TS + Py + scripts): **166,075 lines**

The 137,728 figure matches neither pure Rust LOC (off by +8,782) nor total project LOC (off by −28,347). This suggests the count was accurate at some point but has not been updated as code was refactored, or it uses a counting methodology not reproducible from the current source.

**Recommendation:** Update to actual Rust LOC (~129K) or total source LOC (~166K) with a note on methodology. Consider adding a `scripts/count-loc.sh` to keep it current.

---

### DI-2: Version Badge and Body Text Stuck at v0.1.0 (Medium)

**Location:** README.md lines 22, 29, 371  
**Claim:** "v0.1.0" (badge, release notice, stability section)  
**Actual:** Cargo.toml workspace version is **0.1.7**

Three separate references to v0.1.0 in README.md are now 7 patch versions behind. This creates confusion for users checking compatibility or reading the stability notice.

**Recommendation:** Update all version references to 0.1.7 (or dynamically derive from Cargo.toml).

---

### DI-3: License Section Omits Apache-2.0 Dual-License (Low)

**Location:** README.md line 386  
**Claim:** "MIT — use it however you want."  
**Actual:** Cargo.toml declares `license = "Apache-2.0 OR MIT"`. Both `LICENSE-APACHE` and `LICENSE-MIT` files exist in the repository root.

The README implies MIT-only licensing, which understates the actual dual-license. Users relying on Apache-2.0 patent protections would not know it's available.

**Recommendation:** Update to "Apache-2.0 OR MIT — use it however you want." to match the actual license field.

---

### DI-4: Test Count Inconsistency Between README and CLAUDE.md (Low)

**Location:** README.md line 9 ("1,767+"), CLAUDE.md line 13 ("currently 1744+")  
**Actual:** 1,797 test annotations across the workspace.

Both claims use the "+" qualifier so both are technically accurate, but they cite different baselines (1,744 vs 1,767) which creates confusion. The CLAUDE.md figure appears to be an older snapshot that was never updated when README.md was.

**Recommendation:** Align both files to the same baseline. Consider a CI job that updates the count automatically, or use a single "1,700+" rounded figure in both.

---

### DI-5: openfang-cli Lacks lib.rs (Informational)

**Location:** `crates/openfang-cli/src/` — no `lib.rs`, only `main.rs` and modules  
**Impact:** None functionally — the CLI is a binary crate and doesn't expose a library API.

However, the CLAUDE.md note "Don't touch openfang-cli" suggests active development. If the CLI ever needs to be testable as a library (e.g., for integration tests that import CLI types), a lib.rs would be needed.

**Recommendation:** No action required. Note for future consideration only.

---

### DI-6: No Cross-References Between README and docs/ Directory (Low)

**Location:** README.md, `docs/` directory  
**Issue:** The `docs/` directory contains 16 detailed documentation files (architecture.md, api-reference.md, security.md, etc.) but README.md never links to any of them. All documentation links point to external `https://openfang.sh/docs` URLs.

This means contributors browsing the repository on GitHub cannot discover the local documentation without navigating to the `docs/` folder manually.

**Recommendation:** Add a "Documentation" section in README.md linking to key docs/ files, or add a `docs/README.md` index that cross-references available documentation.

---

## Per-Crate LOC and Test Distribution

| Crate | Rust LOC | Tests | Tests/KLOC |
|-------|----------|-------|------------|
| openfang-runtime | 29,729 | 624 | 21.0 |
| openfang-cli | 24,580 | 32 | 1.3 |
| openfang-channels | 20,937 | 355 | 17.0 |
| openfang-api | 14,578 | 67 | 4.6 |
| openfang-kernel | 13,040 | 220 | 16.9 |
| openfang-types | 9,177 | 265 | 28.9 |
| openfang-migrate | 4,028 | 33 | 8.2 |
| openfang-memory | 3,529 | 40 | 11.3 |
| openfang-skills | 3,071 | 52 | 16.9 |
| openfang-extensions | 2,554 | 54 | 21.1 |
| openfang-wire | 1,517 | 20 | 13.2 |
| openfang-hands | 1,406 | 35 | 24.9 |
| openfang-desktop | 792 | 0 | 0.0 |
| xtask | 4 | 0 | 0.0 |
| **Total** | **128,946** | **1,797** | **13.9** |

Notable: `openfang-cli` has the second-highest LOC but the lowest test density (1.3 tests/KLOC), and `openfang-desktop` has zero tests.

---

## Strengths

1. **Consistent crate naming:** All 14 workspace members use the `openfang-` prefix consistently in both code and documentation. The crate table in README.md is a perfect 1:1 match with Cargo.toml workspace members.

2. **Excellent lib.rs doc comments:** Every library crate has module-level `//!` documentation explaining its purpose, architecture, and key components. These are not boilerplate — they contain meaningful architectural context.

3. **Well-documented traits:** All 8 public traits (`Memory`, `LlmDriver`, `KernelHandle`, `ChannelAdapter`, `PeerHandle`, `HookHandler`, `EmbeddingDriver`, `ChannelBridgeHandle`) have trait-level and method-level doc comments sufficient for implementors.

4. **All image assets resolve:** The 3 internal image references in README.md (`openfang-logo.png`, `openfang-vs-claws.png`, `rightnow-logo.webp`) all point to existing files in `public/assets/`.

5. **Comprehensive docs/ directory:** 16 documentation files covering architecture, API reference, security, CLI reference, channel adapters, providers, configuration, troubleshooting, and more — significantly above average for an open-source project.

6. **CLAUDE.md is actionable:** The agent instructions file contains concrete build commands, API endpoint tables, common gotchas, and step-by-step testing procedures rather than generic guidance.
