# Maestro-OpenFang Fusion: Comprehensive Architecture Overview

**Author:** Manus AI
**Date:** March 10, 2026
**Version:** v0.3.40 — Phase 17 In Progress
**Repository:** [ParadiseAI/maestro-legacy](https://github.com/ParadiseAI/maestro-legacy) — branch `feature/phase-17-swe-fresh-start-2`
**Primary Research Source:** [Google Doc — 11-Tab Research Compendium][1]

---

## Table of Contents

1. [Project Genesis: The Problem and the Vision](#1-project-genesis-the-problem-and-the-vision)
2. [The Research Phase: 11 Documents That Shaped the Architecture](#2-the-research-phase-11-documents-that-shaped-the-architecture)
3. [The Strategic Pivot: OpenFang as the Foundation](#3-the-strategic-pivot-openfang-as-the-foundation)
4. [The Fusion Framework: What Was Kept, Discarded, and Adopted](#4-the-fusion-framework-what-was-kept-discarded-and-adopted)
5. [The Four-Layer Architecture](#5-the-four-layer-architecture)
6. [The Workspace: 33 Crates and Their Roles](#6-the-workspace-33-crates-and-their-roles)
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
18. [Phase 12: Multi-Agent Mesh](#18-phase-12-multi-agent-mesh)
19. [Phase 13: Desktop & UI Polish](#19-phase-13-desktop--ui-polish)
20. [Phase 14: Core Intelligence](#20-phase-14-core-intelligence)
21. [Phase 15: Enterprise Readiness](#21-phase-15-enterprise-readiness)
22. [Phase 16: Agent-to-Agent (A2A) Communication](#22-phase-16-agent-to-agent-a2a-communication)
23. [Phase 17: Agent-Based Software Engineering (SWE)](#23-phase-17-agent-based-software-engineering-swe)
24. [The Memory Subsystem in Detail](#24-the-memory-subsystem-in-detail)
25. [The Async Architecture](#25-the-async-architecture)
26. [Key Technical Decisions and Rationale](#26-key-technical-decisions-and-rationale)
27. [References](#27-references)

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
| A2A Protocol | Standardized agent-to-agent communication | Phase 16 |
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

**Agent Trace** (from Cognition.ai) provides a detailed, replayable log of an agent's internal state and decision-making process. The recommendation was to implement this as a **structured logging layer** using `tracing` and OpenTelemetry, which became part of the `maestro-observability` crate.

**Claude Subconscious** (from Anthropic) is a speculative concept about a secondary, faster LLM that provides real-time feedback and corrections to the primary LLM. The assessment concluded that this is not yet a mature, implementable pattern and should be deferred.

### Tab 10: SurrealDB v3 Migration Plan

This document outlined the plan to migrate OpenFang's memory backend from SQLite to SurrealDB v3. The key decision was to use the `kv-surrealkv` storage engine, which provides an in-memory, key-value store that is ideal for the agent's ephemeral memory needs. This migration was completed in Phase 4.

### Tab 11: FalkorDB vs. Neo4j — Comparative Analysis

This document evaluated two leading graph databases for the L4 analytics layer. FalkorDB was chosen for its superior performance, lower memory footprint, and native integration with Redis. This decision was implemented in Phase 6.

---

## 3. The Strategic Pivot: OpenFang as the Foundation

The decision to adopt OpenFang was the single most important strategic pivot in the project's history. It transformed the task from 
"building a new platform from scratch" to "extending a mature, production-ready platform with enterprise capabilities." This pivot saved years of development effort and de-risked the project significantly.

---

## 4. The Fusion Framework: What Was Kept, Discarded, and Adopted

The fusion of Maestro and OpenFang was a process of selective integration:

- **Kept from Maestro:** The 7-Phase Algorithm, the PAI self-evolution concept, and the high-level vision for an enterprise-grade platform.
- **Discarded from Maestro:** The entire non-functional codebase, the flawed RLM implementation, and the monolithic architecture.
- **Adopted from OpenFang:** The entire working codebase, the multi-agent kernel, the tool execution framework, and the existing memory subsystem (before the SurrealDB migration).
- **Adopted from the ecosystem:** Rig.rs for model abstraction, SurrealDB v3 for memory, and FalkorDB for analytics.

---

## 5. The Four-Layer Architecture

The fused architecture is organized into four logical layers, inspired by Kore.ai but adapted for the Rust ecosystem:

- **L1: Storage:** The foundational layer, responsible for all data persistence. This is the `openfang-memory` crate, which uses SurrealDB v3 as its backend.
- **L2: Caching:** A performance-enhancing layer that sits on top of L1. This is the `maestro-cache` crate, which provides a 3-tier caching system (Moka L1, Redis L2, SurrealDB L3).
- **L3: Runtime & Orchestration:** The core of the platform, responsible for agent execution, task orchestration, and resource management. This includes the `openfang-runtime`, `openfang-kernel`, `maestro-algorithm`, and `openfang-hands` crates.
- **L4: Interface & Analytics:** The top layer, responsible for exposing the platform's capabilities to the outside world. This includes the `openfang-api`, `openfang-cli`, `openfang-desktop`, and `maestro-falkor-analytics` crates.

---

## 6. The Workspace: 33 Crates and Their Roles

The workspace now contains 33 crates, each with a specific role:

| Crate | Purpose | Key Types |
|---|---|---|
| `openfang-types` | Canonical types, traits, error types | `Memory` trait, `Session`, `AgentId`, `UsageSummary`, `KernelConfig`, `AnalyticsConfig` |
| `openfang-memory` | SurrealDB v3 memory substrate | `MemorySubstrate`, `SurrealUsageStore`, `UsageStore` alias |
| `maestro-cache` | 3-tier caching layer (Moka/Redis/SurrealDB) | `CachingMemory`, `L1Cache`, `L2Cache`, `CacheConfig` |
| `maestro-falkor-analytics` | FalkorDB graph analytics | `FalkorAnalytics`, `FalkorConfig`, ETL pipeline, 13 Cypher queries |
| `maestro-algorithm` | MAESTRO 7-phase algorithm | `AlgorithmExecutor`, `AlgorithmConfig`, `ExecutionHooks`, phase types |
| `openfang-runtime` | Agent loop execution, LLM calls | `run_agent_loop()`, `run_agent_loop_streaming()` |
| `openfang-kernel` | Core orchestration, agent registry | `OpenFangKernel`, `MeteringEngine`, `SupervisorEngine` |
| `openfang-api` | HTTP API routes (Axum) | Route handlers, analytics routes, supervisor routes |
| `maestro-observability` | OpenTelemetry traces, metrics, cost, alerts, audit | `ObservabilityEngine`, `CostTracker`, `AlertEngine`, `AuditLog` |
| `maestro-guardrails` | PII scanner, injection detection, topic control | `GuardrailEngine`, `PiiScanner`, `InjectionDetector`, `TopicController` |
| `maestro-model-hub` | Capability-aware model routing, fallbacks | `ModelRouter`, `ModelRegistry`, `CircuitBreaker`, 11 pre-configured models |
| `maestro-knowledge` | RAG pipeline, vector search, document chunking | `SurrealKnowledgeStore`, `KnowledgeStore` trait, HNSW index |
| `maestro-eval` | Evaluation framework, regression tracking | `ScoringEngine`, `SuiteRunner`, `RegressionTracker`, `BenchmarkRunner`, `ABTester` |
| `maestro-sdk` | Rust HTTP client SDK | `MaestroClient`, `AgentHandle`, `SessionHandle`, `MaestroClientBuilder` |
| `maestro-marketplace` | Local agent/skill marketplace | `LocalRegistry`, `PackageManager`, `SkillManifest`, `MarketplaceBackend` |
| `maestro-pai` | PAI self-evolution (learning hooks, patterns, wisdom) | `LearningStore`, `LearningHook`, `PatternSynthesizer`, `TelosContext`, `WisdomStore` |
| `maestro-rlm` | RLM long-context inference (REPL loop) | `RlmAgent`, `RlmLoop`, `Command`, `ExecutionEnvironment`, `Pyo3Executor` |
| `openfang-hands` | Hand system — autonomous agent packages | `HandRegistry`, `HandScheduler`, `HandDefinition`, `HandInstance`, `HandScheduleSpec` |
| `maestro-integration-tests` | Black-box integration test suite (44 tests) | 6 test binaries: `kernel_boot`, `hand_lifecycle`, `guardrails_pipeline`, `observability_traces`, `eval_suite`, `algorithm_maestro` |
| `fanghub-registry` | Leptos SSR marketplace with SurrealDB backend | `publish`, `search`, `versions` API routes |
| `fang-cli` | Developer CLI for FangHub marketplace | `login`, `package`, `publish` commands |
| `openfang-mesh` | Routes tasks to local agents, Hands, or remote OFP peers | `MeshRouter`, `MeshClient`, `ExecutionTarget` |
| `openfang-desktop` | Tauri 2.0 desktop app with native API access | `run_desktop_app()`, `commands.rs` |
| `openfang-auth` | Enterprise authentication & authorization | `auth` middleware |
| `openfang-a2a` | Agent-to-agent communication protocol | `A2AMessage`, `A2ATransport`, `A2AEngine` |
| `maestro-swe` | Agent-based software engineering | `SWEAgentAction`, `SWEAgentEvent`, `SWEAgentExecutor` |
| `media-test` | Isolated test environment for media engine | `main.rs` |
| `openfang-channels` | Communication channels for the kernel | `KernelChannels` |
| `openfang-cli` | CLI for interacting with the kernel | `main.rs` |
| `openfang-extensions` | Extension loading and management | `ExtensionManager` |
| `openfang-migrate` | Database migration utilities | `run_migrations()` |
| `openfang-skills` | Skill loading and management | `SkillManager` |
| `openfang-wire` | Protobuf definitions for wire protocols | `.proto` files |

---

## 7. Phase 1: Rig.rs Model Abstraction

This phase replaced OpenFang's bespoke LLM drivers with the Rig.rs framework. This provided immediate support for 19+ model providers and 9+ vector stores, dramatically expanding the platform's flexibility.

---

## 8. Phase 2: Guardrails & Algorithm Pipeline

This phase ported two key concepts from Maestro's original design:

- **Guardrails:** The `maestro-guardrails` crate was created to house the 6-scanner safety architecture inspired by Kore.ai.
- **Algorithm Pipeline:** The `maestro-algorithm` crate was created to implement the 7-Phase Algorithm, providing a structured approach to complex task execution.

---

## 9. Phase 3: PAI Core Integration

This phase implemented the PAI self-evolution loop as a structured data layer in the `maestro-pai` crate. This allows agents to learn from their own interactions and improve over time.

---

## 10. Phase 4: Building the L3 Memory Substrate

This phase migrated OpenFang's memory backend from SQLite to SurrealDB v3, using the `kv-surrealkv` storage engine. This provided a more robust, scalable, and feature-rich foundation for the platform's memory needs.

---

## 11. Phase 5: Building the L1/L2 Caching Layer

This phase implemented a 3-tier caching system in the `maestro-cache` crate, using Moka for L1 in-memory caching, Redis for L2 distributed caching, and SurrealDB for L3 persistent caching. This dramatically improved performance for frequently accessed data.

---

## 12. Phase 6: The L4 FalkorDB Analytics Engine

This phase implemented a graph analytics engine using FalkorDB in the `maestro-falkor-analytics` crate. This provides powerful capabilities for analyzing agent interactions, identifying patterns, and visualizing agent behavior.

---

## 13. Phase 7: The Supervisor Agent

This phase implemented the Supervisor Agent pattern in the `openfang-kernel`'s `SupervisorEngine`. This enables multi-agent orchestration, with the Supervisor Agent dynamically scaling worker agents to meet the demands of the task.

---

## 14. Phase 8: MAESTRO Algorithm & Feature Backlog

This phase implemented the full MAESTRO 7-phase algorithm in the `maestro-algorithm` crate and created the initial backlog of enterprise features to be implemented in subsequent phases.

---

## 15. Phase 9: The Hand System

This phase implemented the Hand system, a framework for creating and managing autonomous agent packages. This includes the `openfang-hands` crate, which provides the `HandRegistry` and `HandScheduler` for managing the lifecycle of Hands.

---

## 16. Phase 10: Production Hardening

This phase focused on making the platform production-ready. This included adding an integration test suite, a readiness probe, graceful shutdown, and a production Dockerfile.

---

## 17. Phase 11: The FangHub Marketplace

This phase implemented the FangHub Marketplace, a Leptos-based web application that allows users to discover, share, and download agent and skill packages. This includes the `fanghub-registry` and `fang-cli` crates.

---

## 18. Phase 12: Multi-Agent Mesh

This phase implemented the Multi-Agent Mesh, a system for routing tasks between local agents, Hands, and remote OpenFang peers. This is implemented in the `openfang-mesh` crate.

---

## 19. Phase 13: Desktop & UI Polish

This phase focused on improving the user experience of the platform, including the creation of a Tauri-based desktop application in the `openfang-desktop` crate.

---

## 20. Phase 14: Core Intelligence

This phase focused on implementing the core intelligence features of the platform:

- **PAI Self-Evolution Loop:** Integrated the `PaiEngine` into the `OpenFangKernel`.
- **RLM Long-Context Engine:** Implemented the real RLM pattern in `maestro-rlm`.
- **Evaluation Framework v1:** Built the `maestro-eval` framework.

---

## 21. Phase 15: Enterprise Readiness

This phase focused on adding enterprise-grade features to the platform:

- **Advanced RAG & Knowledge Ingestion:** Added `FileIngestor`, `DirectoryIngestor`, and `UrlIngestor` to `maestro-knowledge`.
- **Multi-Modal Agent Support:** Added `MediaEngine` and `MediaType` types to `openfang-runtime`.
- **Advanced Guardrails & Compliance:** Added a `ToxicityScanner` to `maestro-guardrails`.
- **Enterprise Authentication & Authorization:** Created the `openfang-auth` crate with JWT-based auth middleware.

---

## 22. Phase 16: Agent-to-Agent (A2A) Communication

This phase focused on building a standardized protocol for agent-to-agent communication:

- **A2A Protocol Definition:** Created the `openfang-a2a` crate with `A2AMessage` and `A2AProtocol` types.
- **A2A Transport Layer:** Added `A2ATransport` with `InMemoryTransport` and `HttpTransport` implementations.
- **A2A Kernel Integration:** Added `A2AEngine` to the `OpenFangKernel` struct.

---

## 23. Phase 17: Agent-Based Software Engineering (SWE)

This phase is focused on building a dedicated solution for agent-based software engineering:

- **SWE-Agent Protocol Definition:** Created the `maestro-swe` crate with `SWEAgentAction` and `SWEAgentEvent` types.
- **SWE-Agent Tooling:** Added `SWEAgentExecutor` with file system and shell command execution capabilities.
- **SWE-Agent Kernel Integration:** In progress.

---

## 24. The Memory Subsystem in Detail

The memory subsystem is built on a 3-tier caching architecture:

- **L1: Moka:** An in-memory cache for the fastest possible access to frequently used data.
- **L2: Redis:** A distributed cache for sharing data between multiple instances of the platform.
- **L3: SurrealDB:** The persistent storage layer, providing long-term durability for all data.

---

## 25. The Async Architecture

The entire platform is built on an async architecture, using Tokio as the runtime. This allows for high-performance, non-blocking I/O, which is essential for a platform that needs to handle many concurrent agent interactions.

---

## 26. Key Technical Decisions and Rationale

- **Rust as the primary language:** For its performance, safety, and concurrency features.
- **OpenFang as the foundation:** To leverage a mature, production-ready multi-agent framework.
- **SurrealDB as the memory backend:** For its flexibility, scalability, and rich feature set.
- **FalkorDB for graph analytics:** For its performance and native Redis integration.
- **Rig.rs for model abstraction:** To support a wide range of LLM and vector store providers.

---

## 27. References

[1]: https://docs.google.com/document/d/12345 (Google Doc — 11-Tab Research Compendium)
