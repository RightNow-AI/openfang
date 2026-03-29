<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# Runtime Source Code

## Purpose
Core agent execution runtime. Implements the agent loop, tool execution, LLM integration, context management, memory, auth cooldowns, loop guards, browser automation, sandboxing, embeddings, and provider-specific drivers.

## Key Files
| File | Purpose |
|------|---------|
| `agent_loop.rs` | Core execution loop: receive message, recall memories, call LLM, execute tools, save conversation (186KB, largest file) |
| `tool_runner.rs` | Execute tool calls, handle timeouts, stream results |
| `mcp.rs` | Model Context Protocol server integration |
| `a2a.rs` | Agent-to-Agent communication: send tasks to external agents |
| `llm_driver.rs` | LLM driver abstraction, completion requests, streaming |
| `llm_errors.rs` | Comprehensive LLM error handling, categorization, recovery strategies |
| `loop_guard.rs` | Prevent infinite loops: detect hallucinations, rate-limit continuations |
| `auth_cooldown.rs` | Track rate-limit verdicts per provider, exponential backoff |
| `context_budget.rs` | Token counting, message truncation to fit context window |
| `context_overflow.rs` | Recover from context overflow: drop memories, truncate history |
| `browser.rs` | Selenium/WebDriver automation for web scraping and interaction |
| `docker_sandbox.rs` | Docker container isolation for untrusted code execution |
| `embedding.rs` | Generate embeddings via driver for semantic search |
| `media_understanding.rs` | Vision: analyze images, PDFs, video frames |
| `image_gen.rs` | Image generation via Stable Diffusion, DALL-E |
| `mcp_server.rs` | MCP server lifecycle and protocol handling |
| `hooks.rs` | User-defined hooks for agent behavior customization |
| `host_functions.rs` | Host-provided functions for sandboxed agents |
| `link_understanding.rs` | Extract and summarize links in messages |
| `graceful_shutdown.rs` | Coordinated shutdown of runtime components |
| `kernel_handle.rs` | Trait for kernel access without circular dependencies |
| `audit.rs` | Track agent actions for compliance/transparency |
| `compactor.rs` | Compress old memories to free context |
| `apply_patch.rs` | Apply code patches from LLM output |
| `command_lane.rs` | Priority execution lane for critical commands |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `drivers/` | LLM provider implementations (Anthropic, Gemini, OpenAI, etc.) |

## For AI Agents
When modifying the runtime:
- Agent loop is the heart: changes here affect all agents globally
- Tool execution is sandboxed: browser, Docker, or local depending on config
- Context management prevents token overflow: understand `ContextBudget` and truncation
- Each LLM provider has its own driver in `drivers/` for protocol quirks
- Loop guards prevent hallucinations: beware of MAX_ITERATIONS and MAX_CONTINUATIONS
- Memory recall uses embeddings: changes to embedding driver affect all memory search
- Test with live agent: spawn agent, send message, verify tool execution and response
