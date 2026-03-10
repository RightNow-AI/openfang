# Maestro-OpenFang Fusion: Comprehensive Architecture Overview

**Author:** Manus AI
**Date:** March 9, 2026
**Version:** v0.3.33 — Phase 11 Complete
**Repository:** [ParadiseAI/maestro-legacy](https://github.com/ParadiseAI/maestro-legacy) — branch `feature/phase-8-stub-implementation`
**Primary Research Source:** [Google Doc — 11-Tab Research Compendium][1]

---

## Table of Contents

1. [Project Genesis: The Problem and the Vision](#1-project-genesis-the-problem-and-the-vision)
2. [The Research Phase: 11 Documents That Shaped the Architecture](#2-the-research-phase-11-documents-that-shaped-the-architecture)
3. [The Strategic Pivot: OpenFang as the Foundation](#3-the-strategic-pivot-openfang-as-the-foundation)
4. [The Fusion Framework: What Was Kept, Discarded, and Adopted](#4-the-fusion-framework-what-was-kept-discarded-and-adopted)
5. [The Four-Layer Architecture](#5-the-four-layer-architecture)
6. [The Workspace: 28 Crates and Their Roles](#6-the-workspace-28-crates-and-their-roles)
7. [Phase 1: Rig.rs Model Abstraction](#7-phase-1-rigrs-model-abstraction)
8. [Phase 2: Guardrails & Algorithm Pipeline](#8-phase-2-guardrails--algorithm-pipeline)
9. [Phase 3: PAI Core Integration](#9-phase-3-pai-core-integration)
10. [Phase 4: Building the L3 Memory Substrate](#10-phase-4-building-the-l3-memory-substrate)
11. [Phase 5: Building the L1/L2 Caching Layer](#11-phase-5-building-the-l1l2-caching-layer)
12. [Phase 6: The L4 FalkorDB Analytics Engine](#12-phase-6-the-l4-falkordb-analytics-engine)
13. [Phase 7: The Supervisor Agent](#13-phase-7-the-supervisor-agent)
14. [Phase 8: MAESTRO Algorithm & Feature Backlog](#14-phase-8-maestro-algorithm--feature-backlog)
15. [Phase 9: The Hand System](#15-phase-9-the-hand-system)
16. [Phase 10: Production Hardening](#16-phase-10-production-hardening)
17. [Phase 11: The FangHub Marketplace](#17-phase-11-the-fanghub-marketplace)
18. [The Memory Subsystem in Detail](#18-the-memory-subsystem-in-detail)
19. [The Async Architecture](#19-the-async-architecture)
20. [Key Technical Decisions and Rationale](#20-key-technical-decisions-and-rationale)
21. [References](#21-references)

---

## 1. Project Genesis: The Problem and the Vision

The Maestro project began with a stark contrast between ambition and reality. The `maestro-legacy` repository contained a sophisticated vision for an enterprise-grade, multi-agent AI orchestration platform — but the codebase itself was almost entirely non-functional. Crates like `maestro-algorithm`, `maestro-guardrails`, and `maestro-knowledge` existed as scaffolds, but every method body was a `todo!()` macro. The architecture documents were detailed and thoughtful, but the implementation was empty.

The project owner, Rohit Iyer, identified **Kore.ai** as the benchmark — a mature, commercial AI platform that represented what Maestro should eventually become. Kore.ai is not merely a chatbot framework; it is a full enterprise platform with multi-agent orchestration, a 6-scanner guardrails architecture, a knowledge/RAG system with 400+ connectors, an evaluation studio, an agent marketplace, and a complete observability suite. The gap between Maestro's aspirations and Kore.ai's reality was the central problem the project set out to address.

The project's working context was also updated in March 2026 to reflect an evolved vision: rather than following the original Maestro design, the goal became **evolving the core of OpenFang** — adding the best parts of Maestro's concepts and expanding the platform into enterprise scope with Kore.ai as the inspiration. This is a subtle but important distinction: OpenFang is not a tool being used to build Maestro; OpenFang *is* the new Maestro.

---

## 2. The Research Phase: 11 Documents That Shaped the Architecture

The research phase produced eleven documents, organized as tabs in the Google Doc [1]. Each one addressed a specific question about what to build, what to adopt, and what to discard.

### Tab 1: Comprehensive Research Report — Kore.ai Platform Analysis

The foundational research document. It performed an exhaustive analysis of Kore.ai's platform across 100+ pages of official documentation and 15+ GitHub repositories. The key finding was that Kore.ai's strength lies in its **layered, composable architecture** — a core agent platform that is model-agnostic, data-agnostic, and cloud-agnostic, with specialized enterprise modules built on top.

The components most relevant to Maestro were identified as:

| Kore.ai Component | What It Does | Maestro Equivalent |
|---|---|---|
| Multi-Agent Orchestration | Supervisor/worker patterns, intelligent task routing | Phase 7 Supervisor Agent |
| Agent Memory | Short-term (session) + long-term (persistent) memory | Phase 4 SurrealDB substrate |
| A2A Protocol | Standardized agent-to-agent communication | Future Phase |
| Model Hub | Model-agnostic routing across 19+ providers | `maestro-model-hub` crate |
| Guardrails | 6-scanner safety architecture (PII, toxicity, etc.) | `maestro-guardrails` crate |
| Knowledge/RAG | Document ingestion, hybrid vector search | `maestro-knowledge` crate |
| Observability | End-to-end tracing, real-time analytics | `maestro-observability` crate |
| Evaluation Studio | Automated agent testing and benchmarking | `maestro-eval` crate |
| Agent Marketplace | Reusable agent templates and skill sharing | `fanghub-registry` + `maestro-marketplace` |

### Tab 2: Brutally Honest Gap Analysis — Kore.ai vs. Maestro

This document performed a zero-sycophancy assessment of Maestro against Kore.ai. The conclusion was direct: **Maestro is a well-engineered single-agent framework, not an agent platform.** Eight critical gaps were identified:

| Gap | Kore.ai Has | Maestro Had |
|---|---|---|
| 1. Memory & Persistence | Multi-model DB with graph + vector | SQLite (stub, non-functional) |
| 2. Multi-Agent Orchestration | Supervisor/worker with dynamic routing | Single agent, linear phases |
| 3. Guardrails & Safety | 6-scanner architecture | Nothing |
| 4. Knowledge/RAG | 400+ connectors, hybrid search | Nothing |
| 5. Observability | Full request tracing, audit logs | Nothing |
| 6. Model Abstraction | 19+ providers, model factory | Static `llm_config.toml` |
| 7. Evaluation & Testing | Evaluation Studio, App Diagnostics | Nothing |
| 8. Marketplace/Ecosystem | 250+ templates, SDK, community | Nothing |

This analysis directly shaped the priority order of the implementation roadmap — Gap 1 (Memory) became Phase 4, Gap 6 (Model Abstraction) became Phase 1, and so on.

### Tab 3: Kore.ai Platform Architecture — Visual Guide

A structural breakdown of Kore.ai's layered architecture, used as a reference model. The key pattern extracted was the **separation of concerns across layers**: operational data at the bottom, caching in the middle, analytics above that, and orchestration at the top. This directly inspired the four-layer architecture (L1-L4) that defines the project's roadmap.

### Tab 4: OpenFang vs. Maestro — Comparative Analysis

This document answered the most important early question: should the project build on Maestro's codebase or adopt a different foundation? The comparison was definitive:

| Dimension | OpenFang | Original Maestro |
|---|---|---|
| Lines of Code | 151,000 (working) | ~15,000 (mostly stubs) |
| Tools Implemented | 59+ | 0 working |
| Multi-Agent Kernel | Production-ready | Stub only |
| Memory Backend | SQLite (working) | SQLite (stub) |
| LLM Integration | Working | Partial |
| Community | Active | None |
| Test Coverage | Comprehensive | Minimal |

The conclusion was unambiguous: **OpenFang is not merely a theoretical framework; it is a production-ready, multi-agent system.** Adopting it as the foundation would save years of development time and provide a battle-tested base for the enterprise capabilities being added.

### Tab 5: PAI & Fabric Frameworks — Honest Assessment

This document analyzed two frameworks that influenced Maestro's original design: Daniel Miessler's **Personal AI (PAI)** and **Fabric**. The key findings were:

**PAI** is a sophisticated prompt architecture, not a software architecture. Its value lies in the concept of a **self-evolution loop** — agents that learn from their own interactions through structured feedback mechanisms (RatingCapture, WorkCompletionLearning, SessionHarvester). The recommendation was to implement this concept as a **structured data layer** in SurrealDB, not as a prompt system. This became the `maestro-pai` crate concept.

**The 7-Phase Algorithm** (OBSERVE → ORIENT → PLAN → EXECUTE → VERIFY → LEARN → ADAPT) was identified as the single most valuable, salvageable asset from Maestro's original design. It represents a robust, structured approach to complex task execution and was preserved as the `maestro-algorithm` crate.

### Tab 6: Rig.rs & RLM — Framework Assessment

Two critical technology choices were validated here:

**Rig.rs** is a Rust LLM abstraction framework supporting 19+ model providers and 9+ vector stores. It was adopted as the core model abstraction layer, replacing OpenFang's bespoke LLM drivers. This decision was implemented in Phase 1 and is now foundational to the workspace.

**RLM (Recursive Language Model)** was found to be fundamentally misunderstood in Maestro's original implementation. The real RLM pattern is an inference-time context scaling strategy — it treats the prompt as an external data source that the LLM interacts with via a REPL, enabling 10M+ token contexts. The recommendation was to implement the real RLM pattern using PyO3 to embed a Python interpreter in Rust, which became the `maestro-rlm` crate.

### Tab 7: Integration Guide — Maestro Crates to OpenFang

A practical mapping document showing how each Maestro concept integrates into the OpenFang workspace as an extension crate. The key insight was that these crates **extend** OpenFang — they do not replace any existing component. The extension crates are additive capabilities layered on top of the working OpenFang kernel.

### Tab 8: Architectural Blueprint — The Maestro-OpenFang Fusion

The definitive architectural document. It established the **6-point core strategy** that governs all implementation decisions:

1. **Adopt OpenFang as the foundational agent framework.** Its 151K LOC, 59+ tools, and robust kernel provide a base that would take years to replicate.
2. **Adopt Rig.rs as the core model abstraction layer.** Support for 19+ providers and 9+ vector stores is vastly superior to any bespoke solution.
3. **Implement the REAL RLM pattern.** A unique capability for ultra-long context processing that no other open-source framework offers.
4. **Port Maestro's 3 genuinely valuable assets:** the 7-Phase Algorithm Pipeline, the Capability-Aware Model Selector, and the Predictive Analytics Engine.
5. **Implement Kore.ai's best-in-class platform patterns** as new crates: guardrails, knowledge/RAG, observability, marketplace, evaluation.
6. **Implement PAI's self-evolution loop** as a structured data layer, not a prompt architecture.

### Tab 9: Agent Trace + Claude Subconscious Assessment

This document evaluated two advanced concepts the project owner identified as critical for an enterprise platform:

**Agent Trace** (from Cognition.ai) provides **attributable action** — every agent decision can be traced back to its reasoning chain. This capability was mapped to Phase 6 (FalkorDB analytics, Task 6.4) because the graph database is the natural home for storing and querying agent decision traces.

**Claude Subconscious** (from Letta.ai) provides **stateful learning** — agents that maintain a persistent "subconscious" memory that evolves over time. This was identified as equivalent to the Phase 7 Supervisor Agent concept. Critically, the document noted that this capability cannot be implemented until the foundational data layers (Phases 4-6) are complete — a key principle that validates the bottom-up implementation strategy.

### Tab 10: FalkorDB Exhaustive Audit

An exhaustive audit that established FalkorDB as the superior choice for the analytics layer (L4). The key differentiators:

| Capability | FalkorDB | SurrealDB |
|---|---|---|
| Graph Analytics | GraphBLAS-powered (PageRank, community detection) | Manual implementation required |
| Multi-tenancy | Native | Manual implementation |
| Performance | Specialized graph engine | General-purpose |
| Agent Memory Frameworks | Dedicated frameworks | General storage |

The conclusion was that SurrealDB serves as the **operational database** (L3) while FalkorDB serves as the **analytics engine** (L4). An async ETL pipeline moves data between them.

### Tab 11: Evolved Roadmap

The living roadmap document that supersedes all previous versions. It formalizes the concept of **Discovered Prerequisites** — work not in the original plan that became necessary during implementation. The SurrealDB v3 upgrade (Task 4.3) and full async propagation (Task 4.4) are the canonical examples.

---

## 3. The Strategic Pivot: OpenFang as the Foundation

The most consequential decision in the project's history was the pivot from building on Maestro's codebase to adopting OpenFang as the foundation. This decision was not made lightly — it was supported by the quantitative comparison in Tab 4 and the qualitative assessment in Tab 2.

The pivot meant that the project is not "Maestro with OpenFang features." It is **OpenFang, evolved into an enterprise platform** using Maestro's best concepts and Kore.ai's architectural patterns as the blueprint. The original Maestro codebase contributed three valuable concepts (the 7-Phase Algorithm, the Model Selector, and the Predictive Analytics Engine) and nothing else.

This framing is important for understanding the codebase: the `openfang-*` crates are the working core, and the `maestro-*` crates are the enterprise extensions being added on top.

---

## 4. The Fusion Framework: What Was Kept, Discarded, and Adopted

### From Original Maestro (Kept)

| Asset | Why Kept | Current State |
|---|---|---|
| 7-Phase Algorithm Pipeline | Unique structured reasoning approach | `maestro-algorithm` crate — complete |
| Capability-Aware Model Selector | Intelligent routing based on task requirements | `maestro-model-hub` crate — complete |
| Predictive Analytics Engine | Usage prediction and cost optimization | `maestro-observability` crate — complete |

### From Original Maestro (Discarded)

| Asset | Why Discarded |
|---|---|
| Original kernel | OpenFang's kernel is battle-tested with 151K LOC |
| SQLite memory backend | Replaced by SurrealDB for graph + vector + document |
| Bespoke LLM drivers | Replaced by Rig.rs (19+ providers) |
| Original tool system | OpenFang has 59+ working tools |
| Original RLM implementation | Missed the core architectural insight entirely |

### From Kore.ai (Adopted as Patterns)

| Pattern | Implementation | Phase |
|---|---|---|
| 6-scanner guardrails | `maestro-guardrails` crate | Phase 8 |
| Knowledge/RAG system | `maestro-knowledge` crate | Phase 8 |
| Observability suite | `maestro-observability` crate | Phase 8 |
| Agent marketplace | `fanghub-registry` + `maestro-marketplace` | Phase 11 |
| Evaluation studio | `maestro-eval` crate | Phase 8 |
| Multi-agent orchestration | Phase 7 Supervisor Agent | Phase 7 |

### From PAI & Fabric (Adopted as Concepts)

| Concept | Implementation | Phase |
|---|---|---|
| Self-evolution loop | `maestro-pai` crate (structured data layer) | Phase 8 |
| Reinforcement Learning from Memory | `maestro-rlm` crate (real RLM pattern) | Phase 8 |

### From Rig.rs (Adopted as Core Dependency)

Rig.rs was integrated in Phase 1 as the model abstraction layer. It provides unified APIs for 19+ LLM providers, 9+ vector store backends, structured extraction, tool-use patterns, and native Rust async support. It is now a foundational dependency of the workspace.

---

## 5. The Four-Layer Architecture

The project's architecture is organized into four layers, each building on the one below it. This layering ensures that no component depends on a layer above it, and that intelligence is always backed by robust data infrastructure.

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 4: FalkorDB Analytics Engine (Phase 6) ✅                │
│  Purpose: Deep graph analytics, agent trace, strategic insights │
│  Technology: FalkorDB (GraphBLAS), 13 Cypher query templates    │
│  Crate: maestro-falkor-analytics                                │
│  Status: Complete                                               │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 3: SurrealDB Memory Substrate (Phase 4) ✅               │
│  Purpose: Production-grade persistence for all agent state      │
│  Technology: SurrealDB v3.0.2 (SurrealKv engine)                │
│  Tables: memories, sessions, kv_store, agents, paired_devices,  │
│          tasks, usage_records, llm_summaries                    │
│  Methods: 36 implemented (24 memory + 12 usage)                 │
│  Status: Complete, all tests passing                            │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 2: Redis Distributed Cache (Phase 5) ✅                  │
│  Purpose: Distributed caching for horizontal scaling            │
│  Technology: redis-rs (async), feature-gated                    │
│  Behavior: Graceful degradation if Redis unavailable            │
│  Status: Complete, feature-gated                                │
├─────────────────────────────────────────────────────────────────┤
│  LAYER 1: Moka In-Process Cache (Phase 5) ✅                    │
│  Purpose: Sub-millisecond latency for hot data                  │
│  Technology: Moka (TinyLFU eviction), 3 partitions              │
│  Partitions: KV store, sessions, agents                         │
│  Status: Complete, all tests passing                            │
└─────────────────────────────────────────────────────────────────┘
```

Above these four data layers sits the **OpenFang kernel** — the orchestration engine that coordinates agents, tools, sessions, and tasks. The kernel holds a single `Arc<CachingMemory>` reference that abstracts away all four layers. Above the kernel sit the interface layers: the REST API (`openfang-api`), the CLI (`openfang-cli`), the TUI, and the desktop application.

---

## 6. The Workspace: 28 Crates and Their Roles

The workspace is organized into two categories: 13 core OpenFang crates (the working foundation) and 15 Maestro extension crates (the enterprise capabilities added on top).

### OpenFang Core Crates (13)

These crates provide the foundational agent operating system. They were inherited from the OpenFang project and have been extended throughout the Maestro phases.

| Crate | Role |
|---|---|
| `openfang-types` | Shared data types: `Session`, `Message`, `UsageRecord`, `Memory` trait |
| `openfang-memory` | SurrealDB v3 memory substrate (unified in Phase 8 memory unification) |
| `openfang-runtime` | Agent runtime: `KernelHandle` trait, `ToolRunner`, host functions |
| `openfang-wire` | Wire protocol types for agent communication |
| `openfang-api` | Axum-based REST API: 140+ routes, WebSocket, OpenAI-compat, channel bridge |
| `openfang-kernel` | The orchestration kernel: agent lifecycle, session management, task dispatch |
| `openfang-cli` | CLI entry point with TUI, MCP backend, block_on boundaries |
| `openfang-channels` | 40 messaging adapters with rate limiting and DM/group policies |
| `openfang-migrate` | Database migration tooling |
| `openfang-skills` | 60 bundled skills, SKILL.md parser, FangHub marketplace client |
| `openfang-desktop` | Tauri 2.0 native app (system tray, notifications, global shortcuts) |
| `openfang-hands` | Hand system: `HandRegistry`, `HandScheduler`, `HandDefinition`, `HandInstance` |
| `openfang-extensions` | Extension loading system |

### Maestro Extension Crates (15)

These crates add the enterprise features and the MAESTRO algorithm on top of the OpenFang foundation.

| Crate | Role | Phase |
|---|---|---|
| `maestro-cache` | 3-tier caching: Moka L1 + Redis L2 + SurrealDB L3 | 5 |
| `maestro-falkor-analytics` | FalkorDB graph analytics: 13 Cypher queries, 10 typed result types | 6 |
| `maestro-algorithm` | MAESTRO 7-phase pipeline: OBSERVE → ORIENT → PLAN → EXECUTE → VERIFY → LEARN → ADAPT | 7-8 |
| `maestro-guardrails` | PII scanner, prompt injection detector, topic control, custom regex | 8 |
| `maestro-knowledge` | SurrealDB-backed RAG pipeline, HNSW vector search, chunking strategies | 8 |
| `maestro-model-hub` | Capability-aware model router, 11 pre-configured models, circuit breakers | 8 |
| `maestro-observability` | OpenTelemetry traces, metrics, cost tracking, alerts, audit log | 8 |
| `maestro-pai` | Self-evolution engine: hooks, patterns, telos, wisdom (SurrealDB-backed) | 8 |
| `maestro-rlm` | Recursive Language Model for long-context processing via PyO3 | 8 |
| `maestro-marketplace` | Local agent marketplace: install, search, publish, update_all | 8 |
| `maestro-eval` | ScoringEngine, SuiteRunner, RegressionTracker, BenchmarkRunner, ABTester | 8 |
| `maestro-sdk` | Rust embedding SDK: `AgentHandle`, `SessionHandle`, `MaestroClientBuilder` | 8 |
| `maestro-integration-tests` | Black-box integration test suite (44 tests, 7 test binaries) | 10 |
| `fanghub-registry` | FangHub backend: Axum REST API + Leptos SSR UI + SurrealDB store | 11 |
| `fang-cli` | FangHub developer CLI: `login`, `package`, `publish` commands | 11 |

The dependency graph flows strictly downward: `openfang-types` → storage crates → `openfang-runtime` → `openfang-kernel` → interface crates. No upward dependencies are permitted.

---

## 7. Phase 1: Rig.rs Model Abstraction

Phase 1 integrated the Rig.rs framework as the core model abstraction layer, replacing OpenFang's bespoke LLM drivers. This provided unified APIs for 19+ LLM providers and 9+ vector stores.

| Task | Description | Commit |
|---|---|---|
| 1.1 | Integrate Rig.rs and openai-rig driver | 46ca425 |
| 1.2 | Rewrite Rig driver with full history and tool calls | 02b0160 |

---

## 8. Phase 2: Guardrails & Algorithm Pipeline

Phase 2 implemented the first two Maestro extension crates: `maestro-guardrails` and `maestro-algorithm`.

| Task | Description | Commit |
|---|---|---|
| 2.1 | Implement `maestro-guardrails` middleware | 91be825 |
| 2.2 | Implement `maestro-algorithm` pipeline with kernel hooks | 086a55c |

---

## 9. Phase 3: PAI Core Integration

Phase 3 integrated the `maestro-pai` crate, which implements Daniel Miessler's PAI framework as a structured data layer. This included the Fabric PatternManager and the TELOS user context manager.

| Task | Description | Commit |
|---|---|---|
| 3.1 | Implement PatternManager and TelosManager | 126d18d |
| 3.2 | Wire into kernel and agent loop | 126d18d |

---

## 10. Phase 4: Building the L3 Memory Substrate

Phase 4 was the most complex phase to date, requiring four distinct tasks and introducing two discovered prerequisites that were not in the original plan.

### Task 4.1 — Type Unification & Memory Trait Extension (v0.3.26)

The first task established the data model that all subsequent work depends on. The `openfang-types` crate was extended with unified, serializable types: `Session`, `Message`, `UsageRecord`, `UsageSummary`, and `AgentEntry`. The `Memory` trait was extended with new methods including `save_session`, `recall`, `remember`, and embedding operations. The kernel was refactored to use `&dyn Memory` (a trait object) rather than a concrete type, making it backend-agnostic.

### Task 4.2 — SurrealDB Query Implementation (v0.3.27)

The `maestro-surreal-memory` crate was brought to life with 36 real SurrealQL queries across 8 tables. The two key components are:

**`SurrealMemorySubstrate`** — 24 methods covering the full agent memory lifecycle:

| Method Group | Methods |
|---|---|
| Agent management | `save_agent`, `load_all_agents`, `remove_agent` |
| Session management | `get_session`, `create_session`, `save_session`, `delete_session`, `list_sessions` |
| Session operations | `append_canonical`, `set_session_label`, `find_session_by_label`, `delete_canonical_session` |
| KV store | `structured_get`, `structured_set`, `structured_delete`, `list_kv` |
| Device management | `load_paired_devices`, `save_paired_device`, `remove_paired_device` |
| Task queue | `task_post`, `task_claim`, `task_complete`, `task_list` |
| Analytics | `store_llm_summary`, `canonical_context` |

**`SurrealUsageStore`** — 12 methods for usage tracking and cost analytics, including `record_usage`, `get_usage_summary`, `list_usage_records`, `get_cost_estimate`, and `prune_old_records`.

### Task 4.3 — SurrealDB v3 Upgrade (Discovered Prerequisite, v0.3.28)

During integration testing, SurrealDB v2 was found to have API incompatibilities with the workspace's async patterns. The upgrade to v3.0.2 required replacing the `kv-rocksdb` feature flag with `kv-surrealkv`, changing the engine type from `RocksDb` to `SurrealKv` in all connection code, and updating all SurrealQL queries that used v2-specific syntax. This was a discovered prerequisite — it was not in the original Phase 4 plan but became necessary to complete the phase correctly.

### Task 4.4 — Full Workspace Async Propagation (Discovered Prerequisite, v0.3.28)

SurrealDB v3 is fully async-native. Its Rust driver removed all synchronous wrappers, meaning that any code calling SurrealDB must be in an async context. This triggered a full audit and propagation of `async fn` through the entire workspace.

| Crate | Changes |
|---|---|
| `openfang-kernel` | 63 async fns, 125 .await calls |
| `openfang-api` | routes.rs, ws.rs, openai_compat.rs, channel_bridge.rs |
| `openfang-cli` | main.rs, mcp.rs, tui/* |
| `openfang-runtime` | kernel_handle.rs, tool_runner.rs, host_functions.rs |
| `openfang-desktop` | commands.rs, server.rs |

Seven sync/async boundaries were established at the entry points where the async runtime must be created.

---

## 11. Phase 5: Building the L1/L2 Caching Layer
### Task 5.1 — Moka L1 + Redis L2 + CachingMemory Wrapper (v0.3.29)

Phase 5 introduced the `maestro-cache` crate, which implements a transparent 3-tier caching system. The key design decisions were:

**Cache-Aside Pattern for Reads:** When a value is requested, the system checks L1 first, then L2, then L3 (SurrealDB). On a cache miss, the value is fetched from the lower tier and promoted to all higher tiers. This is simpler and safer than write-through because the cache only holds data that has actually been requested.

**Write-Invalidate Pattern for Writes:** When a value is written, it is always written to L3 (SurrealDB) first. Only after a successful write are the L1 and L2 caches invalidated. This ensures that the database is always the source of truth and prevents stale data from persisting in the cache after a failed write.

**Drop-In Replacement Design:** `CachingMemory` exposes all `SurrealMemorySubstrate` methods directly, not just the `Memory` trait methods. This was critical because the kernel calls many substrate-specific methods (like `save_agent`, `load_all_agents`, `get_session`) that are not on the `Memory` trait. The kernel type was changed from `Arc<SurrealMemorySubstrate>` to `Arc<CachingMemory>` with zero changes to any call sites.

**L1 — Moka (In-Process):** Uses the Moka crate with TinyLFU eviction policy. Three separate cache partitions are maintained: one for KV store entries, one for sessions, and one for agent records. Partition separation prevents one type of data from evicting another under memory pressure.

**L2 — Redis (Distributed):** Feature-gated behind the `redis-cache` Cargo feature. When enabled, Redis provides a shared cache across multiple instances of the application, enabling horizontal scaling. When Redis is unavailable (connection refused, timeout), the system gracefully degrades to L1 + L3 without errors.

---
## 12. Phase 6: The L4 FalkorDB Analytics Engine

Phase 6 built the `maestro-falkor-analytics` crate, providing the L4 analytics layer for deep graph analytics and agent trace capabilities. FalkorDB was chosen over SurrealDB for this layer because it is a specialized graph database built on GraphBLAS, offering significantly better performance for graph-specific queries like PageRank and community detection.

### Graph Schema

The FalkorDB graph schema models the relationships between entities and memories:

```
(:Entity {id, name, type, created_at, updated_at})
(:Memory {id, content, agent_id, source, confidence, created_at, accessed_at, access_count, scope})
-[:RELATION {type, confidence, created_at}]->
```

### Analytics Query API

The `FalkorAnalytics` struct exposes 10 typed async query methods:

| Method | Purpose |
|---|---|
| `entity_neighborhood(id, depth)` | Traverse the graph around an entity |
| `shortest_path(from_id, to_id)` | Find the shortest path between two entities |
| `entity_type_distribution()` | Aggregate entity counts by type |
| `most_connected_entities(limit)` | Identify hub entities by degree |
| `agent_memory_timeline(agent_id, limit)` | Chronological memory trace for an agent |
| `relation_strength_analysis(min_confidence)` | Filter edges by confidence score |
| `search_entities_by_name(query, limit)` | Full-text entity search |
| `graph_stats()` | Graph-wide statistics |
| `agent_memory_stats(agent_id)` | Per-agent memory statistics |
| `memories_for_entity(entity_id)` | All memories linked to an entity |

### Analytics API Routes

The analytics engine is exposed via the `openfang-api` crate under the `/api/analytics/` prefix, providing REST endpoints for all 10 query methods.

---

## 13. Phase 7: The Supervisor Agent

Phase 7 delivered the first true multi-agent orchestrator in Maestro, the **Supervisor Agent**. It implements the full 7-phase MAESTRO algorithm for complex task decomposition and delegation.

### MAESTRO Algorithm Phases

| Phase | Output Type | Key Fields |
|---|---|---|
| OBSERVE | `ObserveOutput` | task_restatement, entities, constraints, information_gaps, prior_learnings |
| ORIENT | `OrientOutput` | complexity (1-10), sub_tasks, risks, recommended_agent_count |
| PLAN | `PlanOutput` | steps, criteria (ISC), agent_assignments, estimated_token_budget |
| EXECUTE | `ExecuteOutput` | step_results, all_steps_completed, tokens_used |
| VERIFY | `VerifyOutput` | criterion_results, overall_satisfaction, threshold_met |
| LEARN | `LearnOutput` | learnings, successes, failures, recommendations |
| ADAPT | `AdaptOutput` | adjustments, rationale, confidence |

All types derive `Serialize`, `Deserialize`, and `JsonSchema` for Rig.rs structured extraction.

### Dynamic Scaling Logic

The `SupervisorEngine` implements a dynamic scaling strategy to avoid unnecessary orchestration overhead for simple tasks:

```
complexity ≤ threshold_sequential (default: 3) → run_single_agent()  [passthrough]
complexity ≤ threshold_parallel   (default: 6) → run_sequential()    [full MAESTRO pipeline]
complexity >  threshold_parallel               → run_parallel()      [multi-agent delegation]
```

This means that a simple question like "What is the capital of France?" passes through with zero orchestration overhead, while a complex task like "Research and write a 10,000-word report on quantum computing" triggers full parallel multi-agent orchestration.

### Kernel Integration

The `SupervisorEngine` is integrated into the `OpenFangKernel` as an optional field:

```rust
pub struct OpenFangKernel {
    pub analytics: Option<Arc<FalkorAnalytics>>,         // Phase 6.5
    pub supervisor_engine: Option<Arc<SupervisorEngine>>, // Phase 7
    pub hand_registry: Arc<HandRegistry>,                // Phase 9
    pub hand_scheduler: Arc<HandScheduler>,              // Phase 10
    pub booted_at: Option<std::time::Instant>,           // Phase 10
}
```

Both `analytics` and `supervisor_engine` are `Option` — non-fatal if not configured.

---

## 14. Phase 8: MAESTRO Algorithm & Feature Backlog

Phase 8 was a massive effort to implement the full MAESTRO algorithm and flesh out the remaining stub crates from the initial project scaffold. It was broken into seven sub-tasks across two versions.

### Task 8.1 — Observability Suite (v0.3.30)

The `maestro-observability` crate was implemented with a full OpenTelemetry integration. The `ObservabilityEngine` provides structured tracing, metrics collection, cost tracking, alert management, and an audit log. All spans are exported via OTLP and are compatible with standard observability platforms like Jaeger and Grafana.

### Task 8.2 — Guardrails (v0.3.30)

The `maestro-guardrails` crate implements a multi-scanner safety architecture. The `GuardrailEngine` orchestrates four specialized scanners: a `PiiScanner` for detecting and redacting personally identifiable information, an `InjectionDetector` for identifying prompt injection attacks, a `TopicController` for enforcing topic boundaries, and a custom regex scanner for domain-specific rules.

### Task 8.3 — Model Hub (v0.3.30)

The `maestro-model-hub` crate provides capability-aware model routing. The `ModelRouter` selects the best model for a given task based on declared capabilities (e.g., `code_generation`, `long_context`, `vision`), cost constraints, and latency requirements. It includes a `CircuitBreaker` that automatically falls back to alternative models when a provider is unavailable, and comes pre-configured with 11 models across 4 providers.

### Task 8.4 — Vector Search & RAG (v0.3.30)

The `maestro-knowledge` crate implements a full RAG pipeline backed by SurrealDB's HNSW vector index. The `SurrealKnowledgeStore` supports multiple chunking strategies (fixed-size, sentence-boundary, semantic), automatic embedding generation via the configured model hub, and hybrid search that combines vector similarity with keyword matching.

### Task 8.5 — Evaluation Framework (v0.3.30)

The `maestro-eval` crate provides a comprehensive evaluation suite. The `ScoringEngine` uses LLM-as-judge patterns to score agent outputs against defined criteria. The `SuiteRunner` orchestrates multi-turn evaluation scenarios. The `RegressionTracker` compares current performance against historical baselines. The `BenchmarkRunner` runs standardized benchmarks, and the `ABTester` supports statistical comparison of two agent configurations.

### Task 8.6 — SDK & Marketplace (v0.3.30)

The `maestro-sdk` crate provides a Rust HTTP client SDK for embedding OpenFang agents in external applications. The `maestro-marketplace` crate provides a local agent marketplace for managing and discovering Hands without requiring a network connection.

### Task 8.7 — PAI & RLM (v0.3.30)

The `maestro-pai` crate implements the PAI self-evolution loop as a structured data layer in SurrealDB. The `LearningStore` records agent interactions, the `PatternSynthesizer` identifies recurring patterns, and the `WisdomStore` accumulates long-term insights. The `maestro-rlm` crate implements the Recursive Language Model pattern using PyO3 to embed a Python interpreter in Rust. The `Pyo3Executor` maintains a persistent Python globals dictionary across calls and captures all `print()` output via a `StringIO` buffer.

### Task 8.8 — `maestro-pai` Migration to SurrealDB v3 (v0.3.31)

The `LearningStore` in `maestro-pai` was initially implemented with SQLite (the last remaining SQLite dependency in the workspace). It was migrated to SurrealDB v3 in v0.3.31, completing the workspace-wide persistence strategy unification. This migration also surfaced and documented the canonical SurrealDB v3 query patterns: `CREATE type::record('table', $id) CONTENT $data` and `SELECT * OMIT id FROM table`.

---

## 15. Phase 9: The Hand System

Phase 9 implemented the autonomous `Hand` system — the core user-facing value proposition of OpenFang. A Hand is a self-contained, autonomous agent package that can be activated, scheduled, and managed independently.

### The `HAND.toml` Manifest

Every Hand is defined by a `HAND.toml` manifest that declares its identity, requirements, schedule, prompts, and guardrails:

```toml
[hand]
name = "Lead Generation Hand"
version = "1.0.0"
author = "ParadiseAI"
description = "Discovers and qualifies sales leads"

[requirements]
min_openfang_version = "0.3.30"
required_tools = ["web_search", "csv_export"]

[schedule]
cron = "0 9 * * 1-5"  # 9 AM on weekdays

[guardrails]
approval_required_actions = ["send_email", "post_to_crm"]
```

### The 7 Bundled Hands

The workspace ships with 7 production-ready Hands, each with a multi-phase system prompt, a domain-expertise SKILL.md, and explicit guardrails:

| Hand | Core Capability |
|---|---|
| **Clip** | YouTube → vertical shorts pipeline (FFmpeg + yt-dlp + 5 STT backends) |
| **Lead** | ICP-based prospect discovery, enrichment, and scoring (0-100) |
| **Collector** | OSINT-grade continuous intelligence with knowledge graph construction |
| **Predictor** | Superforecasting with calibrated reasoning chains and Brier score tracking |
| **Researcher** | Deep autonomous research with CRAAP criteria and APA citation formatting |
| **Twitter** | Autonomous X/Twitter account management with approval queue |
| **Browser** | Web automation with mandatory purchase approval gate |

### Hand Lifecycle & State Machine

The `HandInstance` struct tracks the lifecycle of each Hand through four states: `Inactive`, `Active`, `Paused`, and `Error`. The `HandRegistry` manages all Hand instances, and the `HandScheduler` integrates with the system scheduler to execute Hands on their defined cron or interval schedule.

---

## 16. Phase 10: Production Hardening

Phase 10 made the entire platform production-ready by adding a comprehensive integration test suite, health checks, graceful shutdown, and a multi-stage Docker build.

### Integration Test Suite

The `maestro-integration-tests` crate contains 44 black-box tests organized into 7 test binaries. Each binary tests a distinct subsystem by treating the kernel as a black box — it boots the kernel, makes API calls, and asserts on the results.

| Test Binary | Scope |
|---|---|
| `kernel_boot` | Kernel lifecycle, agent registry, boot sequence |
| `hand_lifecycle` | Hand activation, deactivation, pause, resume |
| `guardrails_pipeline` | PII scanning, injection detection, topic control |
| `observability_traces` | Span creation, metric recording, audit log |
| `eval_suite` | Scoring engine, suite runner, regression tracking |
| `algorithm_maestro` | MAESTRO pipeline phases, ISC generation |
| `fanghub_marketplace` | Publish, search, install flow (added in Phase 11) |

A critical constraint: all integration tests must run with `--test-threads=1` because they share an in-memory SurrealDB instance and cannot run concurrently.

### Health & Readiness Probes

Two HTTP endpoints were added to `openfang-api` for Kubernetes-compatible health management:

- **`GET /api/ready`** — Readiness probe. Returns HTTP 200 only when the kernel has fully booted and SurrealDB is reachable. Returns HTTP 503 with a structured reason during startup or degraded states.
- **`GET /api/health`** — Liveness probe. Always returns HTTP 200 with a JSON status object. Used by load balancers to detect crashed instances.

### Production Dockerfile

A 3-stage multi-stage Docker build was created:

1. **`deps` stage:** Builds and caches all Rust dependencies. This stage is only re-run when `Cargo.toml` or `Cargo.lock` changes.
2. **`builder` stage:** Compiles the application binary with `--release` and strips debug symbols.
3. **`runtime` stage:** Copies only the stripped binary into a minimal `debian:bookworm-slim` base image, running as a non-root `openfang` user.

The final image includes a `HEALTHCHECK` directive that calls `/api/ready` every 30 seconds.

### Graceful Shutdown

A `SIGTERM` handler was wired into the kernel's main loop. On receiving `SIGTERM`, the kernel initiates an ordered shutdown sequence: it stops accepting new requests, waits for in-flight requests to complete, flushes all pending writes to SurrealDB, and closes all database connections.

---

## 17. Phase 11: The FangHub Marketplace

Phase 11 built the **FangHub Marketplace**, a public registry for discovering, installing, and sharing `Hand` packages. This phase represented a significant architectural decision to pivot from a JavaScript-based frontend to a pure-Rust, full-stack solution.

### The Leptos Decision

The original Phase 11 blueprint specified a Vite + React + TypeScript frontend. During implementation, this was replaced with **Leptos**, a Rust-based reactive web framework. The rationale was compelling:

The primary driver was eliminating the **type synchronization problem** inherent in a split Rust/TypeScript codebase. With React, any change to a Rust API type requires a corresponding manual update to the TypeScript type definitions. With Leptos, the same Rust types are used on both the server and the client, and the compiler enforces consistency at build time. The secondary driver was deployment simplicity: Leptos's `leptos_axum` integration allows the SSR server and the API server to run in the same process, resulting in a single deployable binary.

### `fanghub-registry` Architecture

The `fanghub-registry` crate is a single binary that serves both the REST API and the SSR web UI. Its internal structure is:

| Module | Purpose |
|---|---|
| `main.rs` | Entry point, Axum router setup, Leptos SSR integration |
| `routes.rs` | REST API handlers: `POST /publish`, `GET /search`, `GET /packages/{id}` |
| `store.rs` | SurrealDB data access layer |
| `models.rs` | Shared data types (used by both API and UI) |
| `auth.rs` | GitHub OAuth authentication |
| `ui/pages/` | Leptos page components: home, search, package detail, dashboard |
| `ui/components/` | Leptos shared components: layout, package card, stats bar |

### `fang-cli` Architecture

The `fang-cli` crate provides the developer-facing command-line interface. Its commands are:

| Command | Purpose |
|---|---|
| `fang login` | Authenticate with FangHub via GitHub OAuth |
| `fang package` | Bundle a Hand directory into a signed `.fang` archive |
| `fang publish` | Upload a signed `.fang` archive to the FangHub registry |
| `fang search` | Search the registry for available Hands |
| `fang install` | Download and install a Hand from the registry |

Manifests are signed with GPG before publishing to ensure integrity.

### Kernel Integration

The `openfang-kernel` was enhanced with an `install_from_fanghub(hand_id: &str)` method that downloads a Hand from the FangHub registry, verifies its GPG signature, and registers it in the local `HandRegistry`.

---

## 18. The Memory Subsystem in Detail

The complete memory subsystem is a three-tier hierarchy:

```
Kernel
  └── Arc<CachingMemory>
        ├── L1: Moka Cache (in-process, <1ms)
        │     ├── kv_partition: Cache<String, serde_json::Value>
        │     ├── session_partition: Cache<String, Session>
        │     └── agent_partition: Cache<String, AgentEntry>
        ├── L2: RedisCache (distributed, ~1ms, optional)
        │     └── MultiplexedConnection (async, pooled)
        └── L3: SurrealMemorySubstrate (persistent, ~5-50ms)
              └── Surreal<Any> (SurrealKv engine)
                    ├── memories table
                    ├── sessions table
                    ├── kv_store table
                    ├── agents table
                    ├── paired_devices table
                    ├── tasks table
                    ├── usage_records table
                    └── llm_summaries table
```

The `MeteringEngine` uses a separate standalone SQLite connection for cost tracking, deliberately isolated from the main memory subsystem to prevent metering data from being affected by cache invalidation or SurrealDB operations.

---

## 19. The Async Architecture

The entire workspace is natively async as of v0.3.28. The async runtime (Tokio) is initialized once at each of seven well-defined entry points, and all code below those entry points uses `async fn` and `.await` natively.

| Entry Point | Pattern | Crate |
|---|---|---|
| CLI main | `#[tokio::main]` | `openfang-cli` |
| TUI event loop | `Runtime::block_on` | `openfang-cli/tui` |
| MCP backend init | `Runtime::block_on` | `openfang-cli/mcp.rs` |
| WASM host functions | `Runtime::block_on` | `openfang-runtime/host_functions.rs` |
| Desktop server lifecycle | `Runtime::new().block_on` | `openfang-desktop/server.rs` |
| Desktop commands | `Runtime::new().block_on` | `openfang-desktop/commands.rs` |
| API server | `#[tokio::main]` | `openfang-api` |

No `block_on` calls exist anywhere in library code. This is a strict invariant that must be maintained in all future work.

---

## 20. Key Technical Decisions and Rationale

### Decision 1: OpenFang as the Foundation (Not Maestro)

The original Maestro codebase was discarded in favor of OpenFang because OpenFang had 151K lines of working code versus Maestro's ~15K lines of stubs. This was not a close call. The project is now OpenFang evolved into an enterprise platform, not Maestro rebuilt on OpenFang.

### Decision 2: SurrealDB v3 over v2

SurrealDB v3 was a necessary upgrade because v2's synchronous API wrappers were incompatible with the workspace's async architecture. v3's fully async-native driver is the correct foundation for a production system. The upgrade cost (Tasks 4.3 and 4.4) was significant but unavoidable.

### Decision 3: SurrealDB for Operations, FalkorDB for Analytics

These two databases serve fundamentally different purposes and are not interchangeable. SurrealDB excels at multi-model operational storage (document + graph + KV + time-series in one database). FalkorDB excels at high-performance graph analytics using GraphBLAS. Using the right tool for each job is more important than minimizing the number of databases.

### Decision 4: Cache-Aside over Write-Through

The caching layer uses cache-aside (lazy population) rather than write-through (eager population) because cache-aside provides a simpler consistency model. Writes always go to SurrealDB first, and the cache is only populated on demand. This prevents the cache from holding stale data after failed writes.

### Decision 5: CachingMemory as a Drop-In Replacement

`CachingMemory` was designed to expose all `SurrealMemorySubstrate` methods, not just the `Memory` trait methods. This allowed the kernel type to be changed from `Arc<SurrealMemorySubstrate>` to `Arc<CachingMemory>` with zero changes to any call sites — a critical design choice that kept the integration clean and non-disruptive.

### Decision 6: Leptos over React/Vite/TypeScript for FangHub UI

The pivot from a JavaScript-based frontend to Leptos was driven by two factors. First, it eliminates the type synchronization problem: with React, API type changes require manual TypeScript updates; with Leptos, the compiler enforces consistency. Second, it enables single-binary deployment: Leptos's `leptos_axum` integration allows the SSR server and API to run in the same process, simplifying deployment and reducing operational complexity.

### Decision 7: Discovered Prerequisites Are Part of the Process

The SurrealDB v3 upgrade and async propagation were not in the original plan. They emerged during implementation. The project's response was to formalize the concept of "Discovered Prerequisites" in the roadmap documentation, update the phase/task numbering to reflect reality, and move forward. This approach — honest documentation of what actually happened — is more valuable than pretending the original plan was perfect.

---

## 21. References

[1]: https://docs.google.com/document/d/19XcoTCDGNw7E2XzasyjHQ9jHP5OCRDzYfKRG7Rp80kc/edit?usp=sharing "Google Doc — 11-Tab Research Compendium"

---

*This document was last updated on March 9, 2026. For the latest state of the project, refer to `HANDOFF.md`, `ROADMAP.md`, and `CHANGELOG.md` in the repository root, and the `maestro-development` skill in the Manus skills directory.*
