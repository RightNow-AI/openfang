# Maestro Project — Comprehensive Handoff Document

**Date:** 2026-03-08
**Version:** v0.3.29
**Active Branch:** `feature/phase-5-caching-layer`
**Repository:** [ParadiseAI/maestro-legacy](https://github.com/ParadiseAI/maestro-legacy)
**Primary Reference:** [Google Doc — Comprehensive Research Report & Gap Analysis](https://docs.google.com/document/d/1QocTh5_gua_WElzFtrN-arnGoR3YxtAK9c_7YSZrBkw/edit)

---

## Table of Contents

1. [Project Genesis and Vision](#1-project-genesis-and-vision)
2. [Research Phase: The Foundation](#2-research-phase-the-foundation)
3. [The Architectural Decision: OpenFang as the Core](#3-the-architectural-decision-openfang-as-the-core)
4. [The Fusion Framework: What Was Kept and What Was Discarded](#4-the-fusion-framework-what-was-kept-and-what-was-discarded)
5. [Implementation History: Phase by Phase](#5-implementation-history-phase-by-phase)
6. [Current Architecture](#6-current-architecture)
7. [Current Codebase State](#7-current-codebase-state)
8. [Repository Structure and Branch Strategy](#8-repository-structure-and-branch-strategy)
9. [Key Technical Decisions and Lessons Learned](#9-key-technical-decisions-and-lessons-learned)
10. [What Comes Next: Phase 6 and Beyond](#10-what-comes-next-phase-6-and-beyond)
11. [How to Resume Work](#11-how-to-resume-work)
12. [Reference Materials](#12-reference-materials)

---

## 1. Project Genesis and Vision

### The Original Problem

The Maestro project began as an ambitious vision for an enterprise-grade, multi-agent AI orchestration platform. The original `maestro-legacy` repository existed but was fundamentally incomplete — it was full of stub crates with `todo!()` implementations, aspirational architecture documents, and no working end-to-end functionality. The codebase had been scaffolded but never brought to life.

### The Inspiration: Kore.ai

The project owner (Rohit Iyer) identified **Kore.ai** as the gold standard for what Maestro should become. Kore.ai is a mature, enterprise-grade conversational AI platform with:

- **Multi-agent orchestration** with supervisor/worker patterns
- **6-scanner guardrails architecture** (PII, toxicity, hallucination, etc.)
- **Knowledge/RAG system** with 400+ pre-built connectors
- **Observability suite** with full request tracing
- **Agent/Skill marketplace** for reusable components
- **Evaluation studio** for automated testing

The initial research (documented in **Tab 1: Research Report** of the Google Doc) performed an exhaustive analysis of Kore.ai's architecture, identifying which capabilities were genuinely enterprise-grade versus marketing fluff.

### The Pivot: From Maestro-First to OpenFang-First

A critical realization emerged during the research phase: **Maestro's codebase was not salvageable as a foundation.** It had the right ideas but no working implementation. Meanwhile, **OpenFang** — an open-source Rust-based agent framework — had 151K lines of working code, 59+ tools, multi-agent orchestration, a robust kernel, and an active community.

> **The core strategic decision:** Adopt OpenFang as the foundational agent framework, surgically extract the genuinely valuable concepts from Maestro and Kore.ai, and implement them as extension crates on top of the battle-tested OpenFang core.

This decision is documented in **Tab 8: Architectural Blueprint** of the Google Doc and represents the project's defining architectural choice.

---

## 2. Research Phase: The Foundation

The research phase produced 11 documents (organized as tabs in the Google Doc). Each document served a specific purpose in building the intellectual foundation for the project:

### Tab 1: Comprehensive Research Report

An exhaustive analysis of Kore.ai's platform capabilities, covering:
- Core platform architecture (XO Platform, Agent AI, Search AI, Contact Center AI)
- Enterprise modules (guardrails, knowledge, observability, marketplace)
- Multi-agent orchestration patterns (supervisor/worker, hierarchical delegation)
- Developer ecosystem (SDKs, APIs, integration patterns)

### Tab 2: Brutal Gap Analysis

An honest, no-sycophancy assessment identifying **8 critical gaps** between Maestro's aspirations and reality:

| Gap | Description |
|---|---|
| 1. Memory & Persistence | SQLite-only, no graph, no vector search |
| 2. Multi-Agent Orchestration | Stub only, no working supervisor pattern |
| 3. Guardrails & Safety | No PII detection, no toxicity filtering |
| 4. Knowledge/RAG | No document ingestion, no retrieval pipeline |
| 5. Observability | No tracing, no metrics, no audit trail |
| 6. Model Abstraction | Bespoke LLM drivers, limited provider support |
| 7. Evaluation & Testing | No automated agent evaluation framework |
| 8. Marketplace/Ecosystem | No skill sharing, no agent templates |

### Tab 3: Kore.ai Platform Architecture

A visual and structural guide to Kore.ai's platform, used as the reference architecture for what enterprise-grade looks like. Key patterns extracted:
- Layered architecture (data → processing → orchestration → interface)
- Scanner-based guardrails (composable, ordered pipeline)
- Multi-tenancy as a first-class concern

### Tab 4: OpenFang vs Maestro Comparison

A side-by-side comparison that made the case for OpenFang as the foundation:

| Dimension | OpenFang | Maestro |
|---|---|---|
| Lines of Code | 151K (working) | ~15K (mostly stubs) |
| Tools | 59+ implemented | 0 working |
| Multi-agent | Working kernel | Stub only |
| Memory | SQLite (working) | SQLite (stub) |
| Community | Active | None |

### Tab 5: PAI & Fabric Analysis

Analysis of two additional frameworks:
- **PAI (Personal AI):** Self-evolution loop concept — agents that learn from their own interactions and improve over time. The key insight was to implement this as a **structured data layer** (not a prompt architecture), which became the `maestro-pai` crate concept.
- **Fabric:** Microsoft's data integration framework. Patterns for connecting heterogeneous data sources were noted for the Knowledge/RAG system design.

### Tab 6: Rig.rs & RLM Assessment

Two critical technology choices:
- **Rig.rs:** A Rust LLM abstraction layer supporting 19+ providers and 9+ vector stores. Adopted as the core model abstraction layer, replacing OpenFang's bespoke LLM drivers. This became the `maestro-model-hub` crate concept and the Phase 1 work.
- **RLM (Recursive Language Model):** A unique pattern for ultra-long context processing that no other open-source framework offers. This became the `maestro-rlm` crate concept.

### Tab 7: Integration Guide — Maestro Crates to OpenFang

A practical mapping of how each Maestro concept translates to an OpenFang extension crate:

| Maestro Concept | OpenFang Integration |
|---|---|
| Algorithm Pipeline | `maestro-algorithm` crate (7-phase pipeline) |
| Model Selector | `maestro-model-hub` crate (capability-aware routing) |
| Predictive Analytics | `maestro-observability` crate (usage prediction) |
| Guardrails | `maestro-guardrails` crate (6-scanner architecture) |
| Knowledge System | `maestro-knowledge` crate (RAG pipeline) |

### Tab 8: Architectural Blueprint — Maestro-OpenFang Fusion

The definitive architectural document. It established the **6-point core strategy:**

1. Adopt OpenFang as the foundational agent framework
2. Adopt Rig.rs as the core model abstraction layer
3. Implement the REAL RLM (Recursive Language Model) pattern
4. Port Maestro's 3 genuinely valuable assets (Algorithm Pipeline, Model Selector, Predictive Analytics)
5. Implement Kore.ai's best-in-class platform patterns as new crates
6. Implement PAI's self-evolution loop as a structured data layer

### Tab 9: Agent Trace + Claude Subconscious Assessment

A brutally honest assessment of two advanced concepts the project owner brought in:
- **Agent Trace** (from Cognition.ai): Attributable action — every agent decision can be traced back to its reasoning chain. This was mapped to Phase 6 (FalkorDB analytics) as Task 6.4.
- **Claude Subconscious** (from Letta.ai): Stateful learning — agents that maintain a persistent "subconscious" memory. This was identified as being equivalent to the **Phase 7 Supervisor Agent** concept and cannot be implemented until the data layers (Phases 4-6) are complete.

### Tab 10: FalkorDB Exhaustive Audit

An exhaustive audit that concluded **FalkorDB is the superior choice for the graph-based analytics tier** of the architecture:
- Native multi-tenancy
- High-performance graph processing via GraphBLAS
- Specialized GenAI and agentic memory frameworks
- Out-of-the-box GraphRAG, code analysis, and Text-to-SQL
- More direct development path than SurrealDB's WASM-based extensibility for analytics use cases

This audit directly shaped Phase 6 of the roadmap.

### Tab 11: Evolved Roadmap

The latest version of the project roadmap, reflecting all discoveries and pivots. This is kept in sync with the `ROADMAP.md` file in the repository.

---

## 3. The Architectural Decision: OpenFang as the Core

The project's four-layer architecture was established based on the research:

```
┌─────────────────────────────────────────────────────────┐
│  L4: FalkorDB Analytics Engine (Phase 6)                │
│      Deep graph analytics, PageRank, agent trace        │
├─────────────────────────────────────────────────────────┤
│  L3: SurrealDB Memory Substrate (Phase 4) ✅            │
│      Production-grade persistence, 8 tables, 36 methods │
├─────────────────────────────────────────────────────────┤
│  L2: Redis Distributed Cache (Phase 5) ✅               │
│      Optional, feature-gated, graceful degradation      │
├─────────────────────────────────────────────────────────┤
│  L1: Moka In-Process Cache (Phase 5) ✅                 │
│      Sub-ms latency, TinyLFU eviction, 3 partitions     │
└─────────────────────────────────────────────────────────┘
```

The key insight was that **data infrastructure must come before intelligence.** You cannot build a Supervisor Agent (Phase 7) without the analytics engine (Phase 6), and you cannot build the analytics engine without the persistence layer (Phase 4) and caching layer (Phase 5). This bottom-up approach was validated repeatedly as the project progressed.

---

## 4. The Fusion Framework: What Was Kept and What Was Discarded

### From Maestro (Kept)

| Asset | Why It Was Kept | Current State |
|---|---|---|
| 7-Phase Algorithm Pipeline | Unique capability for structured reasoning | `maestro-algorithm` crate (stub) |
| Capability-Aware Model Selector | Intelligent model routing based on task requirements | `maestro-model-hub` crate (stub) |
| Predictive Analytics Engine | Usage prediction and cost optimization | Part of `maestro-observability` (stub) |

### From Maestro (Discarded)

| Asset | Why It Was Discarded |
|---|---|
| Original kernel | OpenFang's kernel is battle-tested with 151K LOC |
| SQLite memory backend | Replaced by SurrealDB for graph + vector + document capabilities |
| Bespoke LLM drivers | Replaced by Rig.rs (19+ providers, 9+ vector stores) |
| Original tool system | OpenFang has 59+ working tools |

### From Kore.ai (Adopted as Patterns)

| Pattern | Implementation Plan |
|---|---|
| 6-scanner guardrails | `maestro-guardrails` crate (Phase 8) |
| Knowledge/RAG system | `maestro-knowledge` crate (Phase 8) |
| Observability suite | `maestro-observability` crate (Phase 8) |
| Agent marketplace | `maestro-marketplace` crate (Phase 8) |
| Evaluation studio | `maestro-eval` crate (Phase 8) |

### From PAI & Fabric (Adopted as Concepts)

| Concept | Implementation Plan |
|---|---|
| Self-evolution loop | `maestro-pai` crate — structured data layer, not prompt architecture (Phase 8) |
| Reinforcement Learning from Memory | `maestro-rlm` crate — unique long-context processing (Phase 8) |

### From Rig.rs (Adopted as Core Dependency)

Rig.rs was integrated in Phase 1 as the model abstraction layer. It provides:
- 19+ LLM provider support (OpenAI, Anthropic, Google, Cohere, etc.)
- 9+ vector store backends
- Structured extraction and tool-use patterns
- Native Rust async support

---

## 5. Implementation History: Phase by Phase

### Pre-Phase Work: Scaffolding (develop branch)

**Commit:** `0a0c4a4` — "chore: add 10 maestro fusion crates"

Created the 10 `maestro-*` extension crates as empty scaffolds:
- `maestro-algorithm`, `maestro-guardrails`, `maestro-knowledge`
- `maestro-model-hub`, `maestro-observability`, `maestro-pai`
- `maestro-rlm`, `maestro-marketplace`, `maestro-eval`, `maestro-sdk`

All crates compiled, tested, and passed clippy. This established the workspace structure.

### Phase 1: Rig.rs Driver & Model Hub

**Branches:** `feature/phase-1-rig-driver-v3`, `feature/phase-1-model-hub`

Integrated Rig.rs as the core model abstraction layer. This work established the pattern for how external Rust crates are integrated into the OpenFang workspace.

### Phase 2: Algorithm Pipeline & Guardrails

**Branch:** `feature/phase-2-algorithm-pipeline-guardrails`

Initial implementation of the algorithm pipeline and guardrails scaffolding.

### Phase 3: Research & Architecture Assessment

Produced the `Maestro_Multi_Tier_Architecture_Assessment.md` document that defined the four-layer architecture (L1-L4) and established the Phase 4-7 roadmap. This was the strategic planning phase that set the direction for all subsequent work.

### Phase 4: L3 — SurrealDB Memory Substrate ✅

The largest and most complex phase, broken into 4 tasks due to discovered prerequisites:

**Task 4.1 — Type Unification & Memory Trait Extension (v0.3.26)**

| Item | Detail |
|---|---|
| Commit | `52d7071` |
| Branch | `feature/phase-4-surrealdb-memory` → `feature/phase-4-type-migration` |
| Key Changes | Unified `Session`, `UsageRecord`, `UsageSummary` types in `openfang-types`; extended `Memory` trait with `save_session`, embedding methods; made runtime backend-agnostic (`&dyn Memory`); created `SurrealUsageStore` API surface; fixed kernel to use standalone SQLite for metering |
| Files Changed | 22 files, ~2,000 lines |

**Task 4.2 — SurrealDB Query Implementation (v0.3.27)**

| Item | Detail |
|---|---|
| Commit | `4c7c8d7` |
| Key Changes | Replaced all `todo!()` stubs with real SurrealQL queries for 36 methods across 8 tables; implemented `SurrealUsageStore` (12 methods) and `SurrealMemorySubstrate` (24 methods) |
| Tables | `memories`, `sessions`, `kv_store`, `agents`, `paired_devices`, `tasks`, `usage_records`, `llm_summaries` |

**Task 4.3 — SurrealDB v3 Upgrade (v0.3.28) — Discovered Prerequisite**

| Item | Detail |
|---|---|
| Commit | `d48394a`, `c84d363` |
| Key Changes | Migrated from SurrealDB v2 to v3.0.2; replaced `RocksDb` engine with `SurrealKv`; fixed type inference issues with `Surreal::new()` requiring explicit `Surreal<Any>` annotation |
| Why Discovered | SurrealDB v2 had API incompatibilities that only surfaced during integration testing. v3 was fully async-native, which triggered Task 4.4. |

**Task 4.4 — Full Workspace Async Propagation (v0.3.28) — Discovered Prerequisite**

| Item | Detail |
|---|---|
| Commit | `5895118` |
| Key Changes | Removed all `block_on` calls from library code; propagated `async fn` through 18 files across 7 crates; established 7 sync/async boundary patterns at CLI, TUI, MCP, WASM, and desktop entry points |
| Files Changed | 18 files, 339 insertions, 330 deletions |
| Scope | `openfang-kernel` (63 async fns, 125 .await calls), `openfang-api` (routes, ws, openai_compat, channel_bridge), `openfang-cli` (main, mcp, tui), `openfang-runtime` (kernel_handle, tool_runner, host_functions), `openfang-desktop` (commands, server) |

### Phase 5: L1/L2 — Caching & Shared State ✅

**Task 5.1 — Moka L1 + Redis L2 Caching Layer (v0.3.29)**

| Item | Detail |
|---|---|
| Commit | `49ca542` |
| Key Changes | Created `maestro-cache` crate with `CachingMemory` wrapper; L1 (Moka) with TinyLFU eviction and 3 partitions; L2 (Redis) feature-gated with graceful degradation; cache-aside reads, write-invalidate writes; drop-in kernel integration (`Arc<SurrealMemorySubstrate>` → `Arc<CachingMemory>`) |
| Tests | 8/8 passing (7 unit + 1 doc-test) |
| Files Changed | 10 files, 1,492 insertions, 20 deletions |

---

## 6. Current Architecture

### Crate Dependency Graph

```
Layer 0 (Foundation):  openfang-types
Layer 1 (Storage):     openfang-memory (SQLite)  |  maestro-surreal-memory (SurrealDB v3)
Layer 1.5 (Cache):     maestro-cache (Moka L1 + Redis L2 + SurrealDB L3)
Layer 2 (Runtime):     openfang-runtime
Layer 3 (Kernel):      openfang-kernel
Layer 4 (Interface):   openfang-api  |  openfang-cli  |  openfang-desktop
```

Dependencies flow DOWN only. Never add an upward dependency.

### Memory Subsystem

```
Read path:   L1 (Moka, <1ms) → L2 (Redis, ~1ms) → L3 (SurrealDB, ~5-50ms)
Write path:  L3 (SurrealDB) → invalidate L2 → invalidate L1
```

The kernel holds `memory: Arc<CachingMemory>`, which wraps `SurrealMemorySubstrate` with two caching tiers. The `MeteringEngine` uses a separate standalone SQLite connection for cost tracking.

### Async Architecture

The entire workspace is natively async. All `block_on` calls have been removed from library code. Sync/async boundaries exist only at 7 well-defined entry points (CLI main, TUI event loop, MCP backend init, WASM host functions, desktop server lifecycle).

### 25 Workspace Crates

| Category | Crates |
|---|---|
| **OpenFang Core (13)** | `openfang-types`, `openfang-memory`, `openfang-runtime`, `openfang-wire`, `openfang-api`, `openfang-kernel`, `openfang-cli`, `openfang-channels`, `openfang-migrate`, `openfang-skills`, `openfang-desktop`, `openfang-hands`, `openfang-extensions` |
| **Maestro Extensions (12)** | `maestro-surreal-memory`, `maestro-cache`, `maestro-algorithm`, `maestro-guardrails`, `maestro-knowledge`, `maestro-model-hub`, `maestro-observability`, `maestro-pai`, `maestro-rlm`, `maestro-marketplace`, `maestro-eval`, `maestro-sdk` |

---

## 7. Current Codebase State

### Compilation Status

`cargo check --workspace` passes with **0 errors** as of v0.3.29.

### Test Status

- `maestro-surreal-memory`: 9 tests (all passing as of v0.3.27; need re-verification after v3 upgrade)
- `maestro-cache`: 8 tests (all passing — 7 unit + 1 doc-test)
- Other crates: existing tests from OpenFang upstream (not modified)

### Version Tags

| Tag | Phase | Description |
|---|---|---|
| v0.3.26 | 4.1 | Type Unification & Memory Trait Extension |
| v0.3.27 | 4.2 | SurrealDB Query Implementation |
| v0.3.28 | 4.3 + 4.4 | SurrealDB v3 Upgrade + Async Propagation |
| v0.3.29 | 5.1 | L1/L2 Caching Layer (Moka + Redis) |

### GitHub Releases

All 4 releases (v0.3.26–v0.3.29) are published on GitHub with corrected phase/task labels in their titles and descriptions.

---

## 8. Repository Structure and Branch Strategy

### Branch Naming Convention

All feature branches use the `feature/phase-N-description` format:

| Branch | Status | Content |
|---|---|---|
| `feature/phase-5-caching-layer` | **Default, active** | Latest — all Phase 4 + 5 work |
| `feature/phase-4-surrealdb-memory` | Archived | Initial Phase 4 SurrealDB work |
| `feature/phase-4-type-migration` | Archived | Phase 4.1 type unification |
| `feature/phase-2-algorithm-pipeline-guardrails` | Archived | Phase 2 work |
| `feature/phase-1-rig-driver-v3` | Archived | Phase 1 Rig.rs integration |
| `feature/phase-1-model-hub` | Archived | Phase 1 model hub |
| `feature/maestro-fusion-complete` | Archived | Original fusion scaffolding |
| `main` | Upstream | Original OpenFang community code |
| `develop` | Archived | 10 maestro crate scaffolds |

### Stale Branches to Clean Up

There are two stale `fix/` prefixed branches on GitHub that should be deleted:
- `fix/phase-4-full-type-migration` (superseded by `feature/phase-4-type-migration`)
- `fix/phase-5-surrealdb-v3` (superseded by `feature/phase-5-caching-layer`)

There is also a `feature/phase-4-5-complete` branch that was an intermediate rename and should be deleted.

### GitHub Authentication

The repository uses personal access tokens in the format:

```
https://roALAB1:TOKEN@github.com/ParadiseAI/maestro-legacy.git
```

Tokens expire periodically. If push fails with "Invalid username or token," request a new token from the project owner.

---

## 9. Key Technical Decisions and Lessons Learned

### Decision 1: SurrealDB v3 over v2

SurrealDB v3 was a necessary upgrade because v2's API had incompatibilities with the async patterns required by the workspace. The v3 migration required:
- Replacing `RocksDb` engine with `SurrealKv` (feature flag: `kv-surrealkv`)
- Adding explicit type annotations (`let db: Surreal<Any> = ...`)
- Full async propagation since v3 removed sync wrappers

### Decision 2: Cache-Aside over Write-Through

The caching layer uses **cache-aside** (lazy population on read miss) rather than write-through because:
- Simpler consistency model — writes always go to SurrealDB first
- No risk of stale data in cache after failed writes
- Cache only holds frequently-accessed data, not everything

### Decision 3: CachingMemory as Drop-In Replacement

`CachingMemory` exposes all `SurrealMemorySubstrate` methods directly (not just the `Memory` trait), allowing a type-only swap in the kernel with zero changes to any call sites. This was critical because the kernel uses many substrate-specific methods (like `save_agent`, `load_all_agents`, `get_session`, `append_canonical`) that are NOT on the `Memory` trait.

### Decision 4: FalkorDB for Analytics, SurrealDB for Operations

SurrealDB serves as the operational database (L3), while FalkorDB will serve as the analytics engine (L4). This separation was validated by the exhaustive FalkorDB audit (Tab 10):
- SurrealDB excels at document + graph + KV storage for operational data
- FalkorDB excels at high-performance graph analytics (GraphBLAS, PageRank, community detection)
- An async ETL pipeline will move data from SurrealDB → FalkorDB for analysis

### Lesson 1: Discovered Prerequisites Are Normal

The SurrealDB v3 upgrade and async propagation were not in the original plan. They emerged during implementation and were necessary to complete Phase 4. The roadmap was updated to reflect this reality, and the concept of "Discovered Prerequisites" was formalized in the documentation.

### Lesson 2: Compile Crate-by-Crate, Not Workspace

`surrealdb-core` takes ~10 minutes to compile. Always compile incrementally in dependency order:
```bash
cargo check -p openfang-types → maestro-surreal-memory → maestro-cache → openfang-kernel → openfang-api → openfang-cli
```

### Lesson 3: Backup Before Heavy Compilations

The sandbox environment can time out during heavy compilations (especially test binaries). Always create a backup zip of modified files before running `cargo test` on SurrealDB-dependent crates.

### Lesson 4: Method Renames Happen

During async propagation, some methods were renamed (e.g., `append_to_canonical_session` → `append_canonical`). Always verify the actual method name in the source before updating call sites.

---

## 10. What Comes Next: Phase 6 and Beyond

### Phase 6 — L4 FalkorDB Analytics Engine

**Goal:** Build the `maestro-falkor-analytics` crate and an asynchronous ETL pipeline for deep graph analytics.

| Task | Description | Complexity |
|---|---|---|
| 6.1 | Create `maestro-falkor-analytics` crate with FalkorDB Rust client | Medium |
| 6.2 | Implement async ETL pipeline: SurrealDB → FalkorDB | High |
| 6.3 | Implement graph analytics (PageRank, community detection) | High |
| 6.4 | Integrate agent trace capabilities for observability | Medium |
| 6.5 | Implement write-back of analytical insights to the kernel | Medium |

**Key research needed:** FalkorDB Rust client library, Cypher query patterns, ETL scheduling strategies.

### Phase 7 — The Supervisor Agent

**Goal:** First true multi-agent orchestrator. Depends on Phase 6 analytics for strategic decisions.

| Task | Description |
|---|---|
| 7.1 | Design Supervisor agent type and configuration |
| 7.2 | Build task decomposition engine |
| 7.3 | Create worker delegation protocol |
| 7.4 | Integrate with FalkorDB analytics for strategic decisions |

### Phase 8 — Stub Crate Implementations

**Goal:** Flesh out the remaining 10 stub crates.

| Task | Crates |
|---|---|
| 8.1 | `maestro-observability` (OpenTelemetry), `maestro-guardrails` (PII, safety) |
| 8.2 | `maestro-model-hub` (dynamic routing), `maestro-algorithm` (pipeline) |
| 8.3 | `maestro-rlm` (Recursive Language Model), `maestro-eval` (evaluation) |
| 8.4 | `maestro-sdk`, `maestro-marketplace`, `maestro-knowledge`, `maestro-pai` |
| 8.5 | Embedding/vector search in SurrealDB |
| 8.6 | A2A protocol, WASM sandbox |

---

## 11. How to Resume Work

### Pull the Latest Code

```bash
git clone https://github.com/ParadiseAI/maestro-legacy.git
cd maestro-legacy
git checkout feature/phase-5-caching-layer
git pull origin feature/phase-5-caching-layer
```

Or if you already have the repo:

```bash
git fetch origin
git checkout feature/phase-5-caching-layer
git pull origin feature/phase-5-caching-layer
```

### Read the Skills First

Before starting any work, read these Manus skills:

1. **`/home/ubuntu/skills/maestro-development/SKILL.md`** — Complete development handbook with architecture, compilation order, pitfalls, and workflow
2. **`/home/ubuntu/skills/rust-async-migration/SKILL.md`** — Async patterns, boundary decisions, and common mistakes
3. **`/home/ubuntu/skills/surrealdb/SKILL.md`** — SurrealDB v3 API reference

### Verify the Workspace Compiles

```bash
cargo check --workspace
```

This should produce 0 errors. If it fails, check that Rust toolchain and SurrealDB dependencies are up to date.

### Starting Phase 6

1. Research the FalkorDB Rust client (`falkordb-rs` or equivalent)
2. Create the `maestro-falkor-analytics` crate scaffold
3. Add it to the workspace `Cargo.toml`
4. Implement the ETL pipeline from SurrealDB → FalkorDB
5. Create a new branch: `feature/phase-6-falkordb-analytics`

### GitHub Push Pattern

```bash
git remote set-url origin https://roALAB1:TOKEN@github.com/ParadiseAI/maestro-legacy.git
git push origin feature/phase-6-falkordb-analytics --tags
```

---

## 12. Reference Materials

### Primary Documents

| Document | Location |
|---|---|
| Google Doc (11 tabs) | [Link](https://docs.google.com/document/d/1QocTh5_gua_WElzFtrN-arnGoR3YxtAK9c_7YSZrBkw/edit) |
| ROADMAP.md | `maestro-legacy/ROADMAP.md` |
| CHANGELOG.md | `maestro-legacy/CHANGELOG.md` |
| PHASE-5-BLUEPRINT.md | `maestro-legacy/PHASE-5-BLUEPRINT.md` |
| MIGRATION.md | `maestro-legacy/MIGRATION.md` |
| This handoff document | `maestro-legacy/HANDOFF.md` |

### Google Doc Tab Index

| Tab | Title | Purpose |
|---|---|---|
| 1 | Research Report | Exhaustive Kore.ai analysis |
| 2 | Brutal Gap Analysis | 8 critical gaps identified |
| 3 | Kore.ai Platform Architecture | Visual architecture reference |
| 4 | OpenFang vs Maestro Comparison | Side-by-side comparison |
| 5 | PAI & Fabric Analysis | Self-evolution and data integration patterns |
| 6 | Rig.rs & RLM | Model abstraction and long-context processing |
| 7 | Integration Guide | Maestro crate → OpenFang mapping |
| 8 | Architectural Blueprint | The 6-point core strategy |
| 9 | Agent Trace + Claude Subconscious | Observability and stateful learning assessment |
| 10 | FalkorDB Exhaustive Audit | Analytics engine technology choice |
| 11 | Evolved Roadmap | Latest roadmap (synced with ROADMAP.md) |

### Manus Skills

| Skill | Purpose |
|---|---|
| `maestro-development` | Development handbook, architecture, compilation, pitfalls |
| `rust-async-migration` | Async patterns, boundary decisions, common mistakes |
| `surrealdb` | SurrealDB v3 API reference and best practices |
| `skill-creator` | How to create/update Manus skills |

### Key Crate Documentation

| Crate | Docs |
|---|---|
| SurrealDB v3 | https://docs.rs/surrealdb/3.0.2 |
| Moka | https://docs.rs/moka/latest/moka/future/struct.Cache.html |
| Redis-rs | https://docs.rs/redis/latest/redis/ |
| Rig.rs | https://docs.rs/rig-core/latest/rig/ |
| FalkorDB | https://docs.falkordb.com/ |
