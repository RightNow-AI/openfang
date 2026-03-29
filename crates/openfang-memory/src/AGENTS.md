<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-memory — Unified Memory Substrate

## Purpose

Implements the `Memory` trait as a unified async API over three storage backends:
- **Structured store** (SQLite): Key-value pairs, agent state, persistence
- **Semantic store**: Text-based search and vector embeddings (Phase 1: LIKE, Phase 2: Qdrant)
- **Knowledge graph** (SQLite): Entity-relation store for structured knowledge

Agents and workflows interact with a single `MemorySubstrate` that transparently routes to the appropriate backend.

## Key Files

| File | Purpose |
|------|---------|
| `substrate.rs` | `MemorySubstrate` — main implementation of `Memory` trait, orchestrates all stores |
| `structured.rs` | `StructuredStore` — SQLite KV store, agent state, session persistence |
| `semantic.rs` | `SemanticStore` — text search, vector embeddings, decay/consolidation hooks |
| `knowledge.rs` | `KnowledgeStore` — entity-relation graph, graph pattern matching, queries |
| `session.rs` | `SessionStore` — per-session isolation, thread-local context |
| `consolidation.rs` | `ConsolidationEngine` — memory pruning, summarization, old data cleanup |
| `migration.rs` | Schema migrations — database versioning |
| `usage.rs` | `UsageStore` — token/cost tracking, per-agent usage |
| `http_client.rs` | HTTP memory client (feature-gated) — remote semantic store via memory-api gateway |

## For AI Agents

**When to read:** Understand memory backend architecture, adding new memory stores, or modifying consolidation logic.

**Key interface:**
- `Memory` trait (in types) — async methods for remember/recall/consolidate/import/export
- `MemorySubstrate::open()` — initialize memory with SQLite + semantic backend

**Storage layout:**
- Structured KV + sessions → SQLite (fast local access)
- Semantic → local LIKE queries or remote HTTP if `http-memory` feature enabled
- Knowledge → SQLite with entity/relation tables

**Common tasks:**
- Adding new memory fields → modify structured schema + SessionState
- Implementing vector consolidation → `consolidation.rs`
- Adding new search filters → `semantic.rs` query builder
- Routing semantic to remote API → `http_client.rs`

**Architecture note:** `MemorySubstrate` holds a shared `Arc<Mutex<Connection>>` — all stores coordinate through a single SQLite connection for consistency.
