# Dimension 1: Trait & Core Boundary Integrity

## Summary

The OpenFang crate graph is well-layered overall: `openfang-types` is a leaf with zero internal dependencies, all Tier-1 implementation crates (channels, memory, skills, hands, extensions, wire, migrate) depend only on types, and no Tier-1 crate depends on another Tier-1 crate. The main boundary concern is two "God traits" — `KernelHandle` (27 methods) and `ChannelBridgeHandle` (29 methods) — that violate Interface Segregation and will impose increasing implementation burden on anyone who needs to provide an alternative. The `openfang-api` crate also bypasses the kernel abstraction by pulling concrete types from Tier-1 crates (channels, skills, hands, extensions) rather than going through traits.

## Findings

### Finding T1: `KernelHandle` is a God Trait (27 methods, 7 concern areas)
| Field | Content |
|-------|---------|
| **Issue** | `KernelHandle` mixes agent lifecycle, memory, task queue, knowledge graph, cron scheduling, approval workflow, Hands, and A2A discovery into one trait |
| **Evidence** | `crates\openfang-runtime\src\kernel_handle.rs:26-189` — methods span spawn/kill, memory_store/recall, task_post/claim/complete/list, knowledge_add_entity/add_relation/query, cron_create/list/cancel, requires_approval/request_approval, hand_list/activate/status/deactivate, list_a2a_agents/get_a2a_agent_url, spawn_agent_checked |
| **Risk Tier** | High |
| **Impact** | Any alternative kernel implementation must implement 27 methods. Testing requires massive mocks. Adding one concern (e.g., cron) forces recompilation of every consumer. The default-body pattern hides missing implementations at compile time. |
| **Direction** | Decompose into sub-traits: `AgentSpawner`, `SharedMemory`, `TaskQueue`, `KnowledgeGraph`, `CronScheduler`, `ApprovalGate`, `HandManager`, `A2aDiscovery`. Compose them via a super-trait or pass individually. |

### Finding T2: `ChannelBridgeHandle` is a God Trait (29 methods, 6 concern areas)
| Field | Content |
|-------|---------|
| **Issue** | `ChannelBridgeHandle` mixes agent messaging, session management, model switching, automation (workflows/triggers/schedules/approvals), budget, and network into one trait |
| **Evidence** | `crates\openfang-channels\src\bridge.rs:24-199` — 29 methods across: core messaging (send_message, find_agent_by_name, list_agents, spawn_agent_by_name), session ops (reset_session, compact_session, session_usage), model ops (set_model, set_thinking), display ops (list_models_text, list_providers_text, list_skills_text, list_hands_text), automation (list_workflows_text, run_workflow_text, list_triggers_text, create_trigger_text, delete_trigger_text, list_schedules_text, manage_schedule_text, list_approvals_text, resolve_approval_text), network (budget_text, peers_text, a2a_agents_text), delivery tracking, auth, auto-reply |
| **Risk Tier** | High |
| **Impact** | Every new channel adapter's bridge impl must handle 29 methods. The `_text` suffix methods embed presentation logic in the trait, coupling bridge abstraction to human-readable formatting. |
| **Direction** | Split into `ChannelMessaging` (core 4-5 methods) + `ChannelAdmin` (session/model/display) + `ChannelAutomation` (workflows/triggers/schedules). Move `_text` formatting to the callers; return structured data from the trait. |

### Finding T3: `Memory` trait mixes three storage paradigms
| Field | Content |
|-------|---------|
| **Issue** | The `Memory` trait combines key-value operations (get/set/delete), semantic operations (remember/recall/forget), knowledge graph operations (add_entity/add_relation/query_graph), and maintenance operations (consolidate/export/import) into 12 methods |
| **Evidence** | `crates\openfang-types\src\memory.rs:263-335` — comments explicitly mark sections: "Key-value operations", "Semantic operations", "Knowledge graph operations", "Maintenance" |
| **Risk Tier** | Medium |
| **Impact** | Implementations that only need KV storage must stub 9 other methods. The knowledge graph and embedding concerns are fundamentally different storage paradigms. Adding a new memory backend (e.g., Redis KV) requires implementing graph and semantic methods. |
| **Direction** | Split into `KvStore`, `SemanticMemory`, `KnowledgeGraph`, and `MemoryMaintenance`. Compose them in a `MemorySubstrate` struct. The existing `MemorySubstrate` in `openfang-memory` already hints at this composition. |

### Finding T4: `openfang-api` bypasses kernel abstraction for Tier-1 crates
| Field | Content |
|-------|---------|
| **Issue** | The API crate directly imports 36+ concrete adapter types from `openfang-channels` (every individual adapter struct) plus types from `openfang-skills`, `openfang-hands`, and `openfang-extensions`, rather than going through the kernel |
| **Evidence** | `crates\openfang-api\src\channel_bridge.rs:6-52` — imports `DiscordAdapter`, `SlackAdapter`, `TelegramAdapter`, `TeamsAdapter`, `MatrixAdapter`, `IrcAdapter`, `MattermostAdapter`, `SignalAdapter`, `WhatsAppAdapter`, `EmailAdapter`, plus 26 more concrete adapters. Also `crates\openfang-api\Cargo.toml:13-18` lists `openfang-channels`, `openfang-skills`, `openfang-hands`, `openfang-extensions` as direct dependencies. |
| **Risk Tier** | Medium |
| **Impact** | Adding a new channel adapter requires modifying `openfang-api`. The API layer is coupled to concrete types rather than the `ChannelAdapter` trait. This makes the API non-pluggable — you can't swap channel implementations without touching the API crate. |
| **Direction** | Use a registry pattern: each adapter registers itself at startup. The API should only know about `Box<dyn ChannelAdapter>`, and channel selection should be config-driven, not hardcoded in the API. |

### Finding T5: `openfang-runtime` depends on `openfang-memory` and `openfang-skills` (Tier-1 → Tier-1)
| Field | Content |
|-------|---------|
| **Issue** | The runtime crate depends on two Tier-1 implementation crates (`openfang-memory`, `openfang-skills`) rather than depending only on traits from `openfang-types` |
| **Evidence** | `crates\openfang-runtime\Cargo.toml:10-11` — `openfang-memory = { path = "../openfang-memory" }`, `openfang-skills = { path = "../openfang-skills" }`. Used concretely: `agent_loop.rs:17-18` imports `openfang_memory::session::Session` and `openfang_memory::MemorySubstrate`; `agent_loop.rs:19` imports `openfang_skills::registry::SkillRegistry`. |
| **Risk Tier** | Medium |
| **Impact** | The runtime is coupled to the specific SQLite-based memory implementation and the file-based skill registry. Alternative memory backends or skill loaders cannot be plugged in without modifying the runtime crate. This also means `openfang-runtime` isn't truly Tier-2 — it's partially Tier-1. |
| **Direction** | Extract traits for `Session`, `MemorySubstrate`, and `SkillRegistry` into `openfang-types`. Pass them as trait objects into the agent loop. The `Memory` trait already exists in types — use it. |

### Finding T6: `LlmDriver` and `EmbeddingDriver` are defined in `openfang-runtime`, not `openfang-types`
| Field | Content |
|-------|---------|
| **Issue** | Core abstraction traits `LlmDriver` and `EmbeddingDriver` live in the runtime crate rather than the types crate, even though they define interfaces that other crates need to depend on |
| **Evidence** | `crates\openfang-runtime\src\llm_driver.rs:129` — `pub trait LlmDriver: Send + Sync`. `crates\openfang-runtime\src\embedding.rs:44` — `pub trait EmbeddingDriver: Send + Sync`. The kernel imports these from runtime: `crates\openfang-kernel\src\kernel.rs:21` — `use openfang_runtime::llm_driver::{..., LlmDriver, ...}` |
| **Risk Tier** | Low |
| **Impact** | Any crate wanting to provide an LLM driver must depend on the entire runtime crate (with wasmtime, sandbox, etc.) just for the trait definition. This inflates dependency trees. The traits themselves are narrow and well-designed (2-3 methods each). |
| **Direction** | Move `LlmDriver`, `CompletionRequest`, `CompletionResponse`, `StreamEvent` and `EmbeddingDriver`, `EmbeddingError` type definitions to `openfang-types`. Keep the implementations (OpenAI driver, etc.) in runtime. |

### Finding T7: `KernelHandle` uses `String` errors instead of typed errors
| Field | Content |
|-------|---------|
| **Issue** | All 27 methods on `KernelHandle` return `Result<T, String>` instead of a typed error enum |
| **Evidence** | `crates\openfang-runtime\src\kernel_handle.rs:30-188` — every method signature uses `Result<_, String>`. Example: `async fn spawn_agent(...) -> Result<(String, String), String>`, `async fn task_post(...) -> Result<String, String>` |
| **Risk Tier** | Low |
| **Impact** | Callers cannot match on error variants for programmatic error handling. Error messages are stringly-typed, making it easy to break error-matching code with text changes. Structured errors would enable better UX (e.g., "quota exceeded" vs. generic "error"). |
| **Direction** | Define a `KernelError` enum in `openfang-types` with variants like `AgentNotFound`, `QuotaExceeded`, `Unauthorized`, etc. Migrate `KernelHandle` to use `Result<T, KernelError>`. |

## Strengths

- **Clean Tier-0 foundation**: `openfang-types` has zero internal dependencies and cleanly separates data definitions from business logic. Its `lib.rs` contains no business logic, only `pub mod` declarations.
- **Tier-1 isolation is excellent**: None of the 7 Tier-1 crates (channels, memory, skills, hands, extensions, wire, migrate) depend on each other. They all depend only on `openfang-types`. This is textbook plugin architecture.
- **Dependency direction is correct at the macro level**: The graph flows types → {impl crates} → runtime → kernel → api → cli/desktop. No circular dependencies exist.
- **Narrow interface traits**: `LlmDriver` (2 methods), `EmbeddingDriver` (2 methods), `HookHandler` (1 method), `PeerHandle` (4 methods), and `ChannelAdapter` (9 methods, 4 required + 5 optional) are well-scoped and follow ISP.
- **Good use of default method bodies**: `KernelHandle` and `ChannelBridgeHandle` provide sensible defaults for optional capabilities (cron, hands, A2A), reducing impl burden even though the traits are large.
- **Callback pattern over dynamic loading**: The `HookRegistry` in runtime uses a safe, typed callback pattern rather than unsafe dynamic linking for extensibility.
- **Circular dependency avoidance**: `ChannelBridgeHandle` exists specifically to break what would be a circular dep between channels and kernel. The doc comment at `bridge.rs:21` makes this intentional design explicit.
