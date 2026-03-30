<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-memory

## Purpose
Memory substrate for the OpenFang Agent OS. Provides a unified `Memory` trait over three storage backends: structured store (SQLite key-value pairs, sessions, agent state), semantic store (text-based search with LIKE matching and optional Qdrant vector DB), and knowledge graph (SQLite entities and relations). Agents interact with a single `Memory` interface that abstracts over all three stores. Supports memory consolidation, decay, and optional HTTP remote memory API.

## Key Files
| File | Description |
|------|-------------|
| `src/substrate.rs` | `MemorySubstrate` struct composing all stores behind the `Memory` trait |
| `src/structured.rs` | Key-value store, agent state, configuration storage |
| `src/semantic.rs` | Text-based semantic search (LIKE fallback, Qdrant vector optional) |
| `src/knowledge.rs` | Knowledge graph: entities, relations, graph queries |
| `src/session.rs` | Session management, message history, turn tracking |
| `src/consolidation.rs` | Memory consolidation engine, relevance decay over time |
| `src/usage.rs` | Token usage tracking per agent/model/date |
| `src/migration.rs` | Database schema migrations on startup |
| `src/http_client.rs` | HTTP client for remote memory API (feature-gated) |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Memory stores and consolidation |

## For AI Agents

### Working In This Directory
- `MemorySubstrate::open()` opens the SQLite database and initializes all stores
- Implement new memory types by adding methods to one of the store modules
- Migrations go in `migration.rs` — they run on startup if schema is outdated
- Semantic search can route to HTTP backend if configured (see `src/substrate.rs`)
- Knowledge graph uses edge-based storage — entities are (type, name) tuples, edges have (subject, relation, object)
- Sessions store message history and are created per agent invocation
- Consolidation runs periodically (configurable interval) to decay old memories

### Testing Requirements
- Run `cargo test --package openfang-memory` after changes
- Structured store tests: write KV pair → read back → verify equals
- Semantic store tests: remember text → recall with query → verify relevance
- Knowledge graph tests: add entities/relations → query paths → verify graph traversal
- Session tests: create session → append messages → load session → verify history
- Migration tests: start with old schema → run migration → verify new schema
- For HTTP backend: mock HTTP server, test fallback to SQLite on error

### Common Patterns
- All stores share a single `Arc<Mutex<Connection>>` for thread safety
- `Memory` trait is `async_trait` — all methods are async (even SQLite blocks briefly)
- Semantic search returns `MemoryFragment` with score and source metadata
- Knowledge graph uses three SQLite tables: entities, relations, and edge index
- Sessions are identified by (agent_id, session_id) tuple
- Consolidation uses exponential decay: `relevance *= decay_rate` per time interval
- HTTP backend is optional feature (`http-memory`) — fallback to local SQLite if disabled

## Dependencies

### Internal
- `openfang-types` — agent, memory, config types

### External
- `tokio` — async runtime
- `rusqlite` — SQLite database
- `serde_json` — JSON serialization
- `rmp-serde` — MessagePack serialization for compact storage
- `chrono` — datetime types
- `uuid` — unique identifiers
- `thiserror` — error types
- `async-trait` — async trait support
- `tracing` — structured logging
- `reqwest` — HTTP client for remote memory API (optional feature)

<!-- MANUAL: -->
