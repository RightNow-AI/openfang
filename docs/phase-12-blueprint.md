# Phase 12 Blueprint — Multi-Agent Mesh

**Version target:** v0.3.35  
**Branch:** `feature/phase-8-stub-implementation`  
**Status:** In Progress

---

## Overview

Phase 12 upgrades OpenFang from a single-supervisor, sequential-execution model to a true **multi-agent mesh** where the Supervisor can dispatch work to Hands as sub-agents, parallelizable steps execute concurrently, and external agents can discover and communicate with any individual agent via the A2A protocol.

---

## Goals

1. **Parallel EXECUTE** — Steps marked `parallelizable: true` in the PLAN output run concurrently via `tokio::task::JoinSet`, bounded by a configurable `max_parallel_workers` limit.
2. **Hand-as-SubAgent dispatch** — `SupervisorHooks::delegate_to_agent` gains a capability-aware path that matches required capabilities against active Hand instances before falling back to spawning a new agent.
3. **A2A per-agent routing** — `POST /a2a/tasks/send` reads an optional `agentId` field from `params` and routes to the specified agent, not always `agents[0]`.
4. **A2A SSE streaming** — New `POST /a2a/tasks/sendSubscribe` endpoint returns a Server-Sent Events stream of task progress (status changes and partial text deltas).
5. **Per-agent A2A cards** — New `GET /a2a/agents/{id}` endpoint returns the Agent Card for a specific agent.
6. **`openfang-mesh` crate** — New thin crate providing `MeshRouter` (routes tasks to local agents, Hands, or remote OFP peers based on capability matching) and `MeshClient` (sends tasks to remote peers via OFP wire protocol).
7. **Integration tests** — 8 new tests covering all of the above.

---

## Task Breakdown

### Task 12.1 — Parallel EXECUTE Phase (`maestro-algorithm`)

**What it does:** Upgrades the sequential step loop in `phases.rs` to group steps by `parallelizable` flag. Non-parallelizable steps still run sequentially. Parallelizable steps are collected into a `tokio::task::JoinSet` and awaited concurrently, with results merged back in step order.

**Key changes:**
- `phases.rs`: Replace sequential `for step in &plan.steps` loop with a two-pass approach:
  1. Collect sequential steps and run them one by one
  2. Collect parallelizable steps and run them via `JoinSet::spawn`
- `executor.rs`: Add `max_parallel_workers: usize` to `AlgorithmConfig` (default: 4)
- `types.rs`: No changes needed — `ExecutionStep.parallelizable` already exists

**Est. LOC:** ~80 changed

---

### Task 12.2 — Hand-Aware Delegation (`openfang-kernel`)

**What it does:** Upgrades `SupervisorHooks::delegate_to_agent` to check active Hand instances before spawning a new agent. If a Hand with matching capabilities is active, its agent receives the task directly.

**Key changes:**
- `supervisor_engine.rs`: In `delegate_to_agent`, before checking `list_agents()` for running agents, check `kernel.hand_registry.list_instances()` for active Hands whose `tools` overlap with the required `capabilities`
- Add `dispatch_to_hand(hand_id, task)` helper on `SupervisorEngine` for direct Hand dispatch by ID

**Est. LOC:** ~60 changed

---

### Task 12.3 — A2A Per-Agent Routing (`openfang-api`)

**What it does:** Upgrades `a2a_send_task` to support an optional `agentId` field in `params`. If present, routes to the named/ID-specified agent. If absent, falls back to the first available agent (existing behavior).

**Key changes:**
- `routes.rs`: In `a2a_send_task`, extract `params.agentId` and resolve it via `registry.list()` by name or UUID
- Add `GET /a2a/agents/{id}` route for per-agent card discovery
- Register new route in `server.rs`

**Est. LOC:** ~60 changed

---

### Task 12.4 — A2A SSE Streaming (`openfang-api`)

**What it does:** Adds `POST /a2a/tasks/sendSubscribe` which submits a task and immediately returns an SSE stream. The stream emits:
- `status_update` events as the task transitions through `Working → Completed/Failed`
- `text_delta` events for partial text as the agent streams its response
- A final `task_complete` event with the full task object

**Key changes:**
- `routes.rs`: Add `a2a_send_task_subscribe` handler using `axum::response::sse::Sse`
- `server.rs`: Register `POST /a2a/tasks/sendSubscribe`
- Uses the existing `send_message_streaming` kernel method

**Est. LOC:** ~120 new

---

### Task 12.5 — `openfang-mesh` Crate

**What it does:** New crate providing two types:
- `MeshRouter` — given a task and a set of required capabilities, selects the best execution target: (1) active Hand with matching tools, (2) running local agent with matching tags, (3) remote OFP peer with matching agents, (4) spawn new agent
- `MeshClient` — thin async client for sending tasks to remote OFP peers via the wire protocol

**Key files:**
- `crates/openfang-mesh/src/lib.rs`
- `crates/openfang-mesh/src/router.rs`
- `crates/openfang-mesh/src/client.rs`
- `crates/openfang-mesh/Cargo.toml`

**Est. LOC:** ~350 new

---

### Task 12.6 — Integration Tests (`maestro-integration-tests`)

**What it does:** 8 new integration tests:
1. `test_parallel_execute_runs_concurrent_steps` — verifies parallelizable steps complete faster than sequential
2. `test_sequential_steps_run_in_order` — verifies non-parallelizable steps maintain order
3. `test_hand_dispatch_routes_to_active_hand` — verifies delegation finds active Hand by capability
4. `test_a2a_per_agent_routing` — verifies `agentId` field routes to correct agent
5. `test_a2a_per_agent_card` — verifies `GET /a2a/agents/{id}` returns correct card
6. `test_a2a_send_subscribe_streams_events` — verifies SSE stream emits events
7. `test_mesh_router_selects_hand_first` — verifies MeshRouter priority order
8. `test_mesh_router_falls_back_to_spawn` — verifies fallback to spawn when no match

**Est. LOC:** ~400 new

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    SupervisorEngine                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  AlgorithmExecutor (MAESTRO 7-phase)                    │   │
│  │                                                          │   │
│  │  PLAN phase → produces ExecutionStep[] with              │   │
│  │               parallelizable: bool                       │   │
│  │                                                          │   │
│  │  EXECUTE phase (NEW):                                    │   │
│  │  ┌──────────────────────────────────────────────────┐   │   │
│  │  │  Sequential steps → run one by one               │   │   │
│  │  │  Parallel steps   → JoinSet::spawn (max 4)       │   │   │
│  │  └──────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  SupervisorHooks::delegate_to_agent (NEW priority order):       │
│  1. Active Hand with matching capability tags                    │
│  2. Running local agent with matching tags                       │
│  3. Remote OFP peer via MeshClient                              │
│  4. Spawn new worker agent                                       │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    A2A Protocol (upgraded)                       │
│                                                                  │
│  GET  /.well-known/agent.json      → primary agent card         │
│  GET  /a2a/agents                  → all agent cards            │
│  GET  /a2a/agents/{id}    (NEW)    → per-agent card             │
│  POST /a2a/tasks/send              → route to agentId (NEW)     │
│  POST /a2a/tasks/sendSubscribe (NEW) → SSE stream               │
│  GET  /a2a/tasks/{id}              → task status                │
│  POST /a2a/tasks/{id}/cancel       → cancel task                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    openfang-mesh (NEW crate)                     │
│                                                                  │
│  MeshRouter                                                      │
│  ├── route(task, capabilities) → ExecutionTarget                │
│  │   ├── Hand(hand_id, agent_id)                                │
│  │   ├── LocalAgent(agent_id)                                   │
│  │   ├── RemotePeer(node_id, agent_id)                          │
│  │   └── SpawnNew(manifest)                                     │
│  └── score_target(target, capabilities) → f32                   │
│                                                                  │
│  MeshClient                                                      │
│  └── send_task(peer_addr, agent_id, task) → Result<String>      │
└─────────────────────────────────────────────────────────────────┘
```

---

## Dependency Graph

```
openfang-mesh
  ├── openfang-types
  ├── openfang-wire
  └── openfang-hands

openfang-kernel (updated)
  └── openfang-mesh (new dep)

maestro-algorithm (updated)
  └── tokio (already in workspace)
```

---

## Success Criteria

- [ ] Parallelizable steps in EXECUTE phase run concurrently
- [ ] `SupervisorHooks::delegate_to_agent` routes to active Hands first
- [ ] `POST /a2a/tasks/send` with `agentId` routes to the correct agent
- [ ] `GET /a2a/agents/{id}` returns the correct agent card
- [ ] `POST /a2a/tasks/sendSubscribe` returns an SSE stream
- [ ] `openfang-mesh` crate compiles and exports `MeshRouter` + `MeshClient`
- [ ] All 8 new integration tests pass
- [ ] `cargo check --workspace` passes with zero errors
- [ ] Total test count ≥ 2,055 (2,047 + 8 new)
