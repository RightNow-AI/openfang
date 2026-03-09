# Phase 8 Blueprint — Stub Crate & Feature Backlog Implementation

**Date:** 2026-03-09  
**Branch:** `feature/phase-8-stub-crate-implementation`  
**Goal:** Flesh out the remaining stub crates and close the feature backlog before v0.1.0 release.

---

## Current State Assessment

After Phases 4–7, the workspace has 10 stub crates with `todo!()` bodies and 3 major feature gaps:

| Crate / Feature | Lines | `todo!()` count | Priority |
|----------------|-------|-----------------|----------|
| `maestro-observability` | 126 | 7 | HIGH — needed for production ops |
| `maestro-guardrails` | 206 | 10 | HIGH — needed for safety/compliance |
| `maestro-model-hub` | 195 | 7 | HIGH — unifies LLM routing |
| `maestro-rlm` | 189 | 7 | MEDIUM — RLM inference pattern |
| `maestro-eval` | 83 | 5 | MEDIUM — agent quality scoring |
| `maestro-sdk` | 53 | 2 | MEDIUM — Rust embedding SDK |
| `maestro-marketplace` | 79 | 3 | MEDIUM — agent/skill discovery |
| `maestro-knowledge` | 90 | 5 | MEDIUM — RAG pipeline |
| `maestro-pai` | 147 | 5 | LOW — PAI self-evolution |
| Vector Search (8.5) | — | — | HIGH — `recall_with_embedding_async` is a stub |
| A2A Protocol (8.6) | — | — | MEDIUM — routes exist, wire protocol is a stub |

---

## Task Breakdown

### Task 8.1 — Observability Suite (`maestro-observability`)

**What it does:** OpenTelemetry-based tracing, metrics, and alerting for every agent interaction.

**Deliverables:**
- `TracingLayer` — wraps agent interactions with parent/child spans (OpenTelemetry OTLP export)
- `MetricsDashboard` — real-time counters: latency, tokens, cost, errors per agent
- `AlertEngine` — configurable rules (e.g., "alert if p99 latency > 2s") with webhook/email notification
- `AuditLog` — append-only compliance log (SurrealDB-backed)
- Integration with `openfang-kernel` — kernel publishes events, observability subscribes

**Key files:** `src/tracing.rs`, `src/metrics.rs`, `src/alerts.rs`, `src/audit.rs`, `src/lib.rs`  
**Est. LOC:** ~900  
**Dependencies:** `opentelemetry`, `opentelemetry-otlp`, `tracing-opentelemetry`

---

### Task 8.2 — Guardrails Engine (`maestro-guardrails`)

**What it does:** Safety and compliance layer that intercepts agent inputs/outputs.

**Deliverables:**
- `PiiScanner` — regex + NER-based detection of SSN, credit cards, emails, phone numbers
- `ContentFilter` — configurable blocklist/allowlist with severity levels
- `RateLimitGuard` — per-agent, per-user, per-session token budget enforcement
- `PromptInjectionDetector` — extends existing `scan_prompt_content()` in openfang-skills
- `GuardrailMiddleware` — Axum middleware layer that wraps every request/response
- Integration with `openfang-kernel` — kernel checks guardrails before executing agent tools

**Key files:** `src/pii.rs`, `src/content.rs`, `src/rate_limit.rs`, `src/injection.rs`, `src/middleware.rs`  
**Est. LOC:** ~800  
**Dependencies:** `regex`, `once_cell`

---

### Task 8.3 — Model Hub (`maestro-model-hub`)

**What it does:** Intelligent, capability-aware LLM routing that replaces the static `LlmDriver` selection.

**Deliverables:**
- `ModelRouter` — selects the best model based on task complexity, cost budget, and latency SLA
- `CapabilityMatrix` — maps models to capabilities (vision, code, reasoning, long-context, etc.)
- `CostOptimizer` — routes cheap tasks to fast/cheap models, complex tasks to frontier models
- `FallbackChain` — automatic failover when a provider is down
- Integration with `openfang-kernel` — kernel uses `ModelRouter` instead of static driver selection

**Key files:** `src/router.rs`, `src/capabilities.rs`, `src/cost.rs`, `src/fallback.rs`  
**Est. LOC:** ~700  
**Dependencies:** `rig-core` (already in workspace)

---

### Task 8.4 — Vector Search (`openfang-memory`)

**What it does:** Implements the two stub methods in the `Memory` trait: `recall_with_embedding_async` and `remember_with_embedding_async`.

**Deliverables:**
- SurrealDB vector index (`DEFINE INDEX ... HNSW DIMENSION 1536`) on `memory_fragments.embedding`
- `recall_with_embedding_async` — cosine similarity search using SurrealDB's `vector::similarity::cosine()`
- `remember_with_embedding_async` — stores fragment with embedding vector
- `EmbeddingDriver` integration — auto-generates embeddings via OpenAI `text-embedding-3-small` when storing
- `maestro-knowledge` RAG pipeline — uses vector search for document chunking, ingestion, and retrieval

**Key files:** `crates/openfang-memory/src/substrate.rs`, `crates/maestro-knowledge/src/lib.rs`  
**Est. LOC:** ~500  
**Dependencies:** `async-openai` or `rig-core` embeddings

---

### Task 8.5 — Evaluation Framework (`maestro-eval`)

**What it does:** Automated quality scoring for agent responses.

**Deliverables:**
- `EvalSuite` — runs a set of test cases against a live agent
- `ScoringEngine` — LLM-as-judge scoring with configurable rubrics (accuracy, helpfulness, safety, format)
- `RegressionTracker` — compares scores across versions, flags regressions
- `BenchmarkRunner` — runs standard benchmarks (MMLU subset, HumanEval subset)
- API route: `POST /api/eval/run`, `GET /api/eval/results`

**Key files:** `src/suite.rs`, `src/scoring.rs`, `src/regression.rs`, `src/benchmarks.rs`  
**Est. LOC:** ~600  
**Dependencies:** `rig-core`

---

### Task 8.6 — SDK & Marketplace (`maestro-sdk`, `maestro-marketplace`)

**What it does:** Rust embedding SDK and local-first agent/skill marketplace.

**Deliverables (SDK):**
- `OpenFangClient` — Rust client for the HTTP API (mirrors JS/Python SDKs)
- `AgentHandle` — typed handle for spawning, chatting, and killing agents
- `StreamingResponse` — async stream of SSE events
- Published as `maestro-sdk` crate

**Deliverables (Marketplace):**
- `MarketplaceRegistry` — indexes all agents in `agents/` and skills in `skills/`
- `PackageManager` — install/update/remove agents and skills from FangHub
- `VersionManager` — semantic versioning for agent packages
- API routes: `GET /api/marketplace/agents`, `POST /api/marketplace/install`

**Est. LOC:** ~700  
**Dependencies:** `reqwest`, `tokio-stream`

---

### Task 8.7 — PAI & RLM (`maestro-pai`, `maestro-rlm`)

**What it does:** PAI self-evolution framework and RLM long-context inference.

**Deliverables (PAI):**
- `TelosContext` — 10 markdown files capturing user identity, goals, and values
- `LearningHook` — appends structured JSONL feedback after every agent interaction
- `EvolutionEngine` — mines feedback logs and proposes algorithm changes
- `SkillEvolver` — auto-generates new skills from repeated task patterns

**Deliverables (RLM):**
- `RlmExecutor` — Python REPL (via `pyo3`) for symbolic LLM interaction
- `ContextCompressor` — recursively summarizes long contexts
- `SymbolicInteraction` — structured query/response loop with the REPL

**Est. LOC:** ~800  
**Dependencies:** `pyo3` (RLM), `serde_json` (PAI)

---

## Recommended Execution Order

```
8.1 Observability  ──→  8.2 Guardrails  ──→  8.3 Model Hub
                                                    │
8.4 Vector Search  ──→  (maestro-knowledge RAG)    │
                                                    ▼
8.5 Eval Framework ──→  8.6 SDK & Marketplace  ──→  8.7 PAI & RLM
```

**Critical path:** 8.1 → 8.2 → 8.3 (these three unlock production readiness)  
**High-value quick wins:** 8.4 (vector search closes the biggest Memory trait gap)

---

## Total Estimate

| Task | LOC | Complexity |
|------|-----|------------|
| 8.1 Observability | ~900 | High |
| 8.2 Guardrails | ~800 | High |
| 8.3 Model Hub | ~700 | Medium |
| 8.4 Vector Search | ~500 | Medium |
| 8.5 Eval Framework | ~600 | Medium |
| 8.6 SDK & Marketplace | ~700 | Medium |
| 8.7 PAI & RLM | ~800 | High |
| **Total** | **~5,000** | |

**Estimated sessions:** 3–4 (similar to Phase 7)
