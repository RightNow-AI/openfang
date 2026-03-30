<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-runtime

## Purpose
Agent runtime and execution environment. Manages the agent execution loop, LLM driver abstraction (Groq, OpenAI, Anthropic, Claude Code, Gemini, Qwen, Vertex, Ollama, Copilot), tool execution, MCP client, WASM sandboxing for untrusted code, web fetch/search, media understanding, Python subprocess runtime, Docker sandbox, and streaming response handling. The runtime is the core execution engine; the kernel orchestrates it.

## Key Files
| File | Description |
|------|-------------|
| `src/agent_loop.rs` | Main agent loop: receive message, recall memory, call LLM, execute tools, save conversation |
| `src/llm_driver.rs` | LLM driver trait and request/response types for provider abstraction |
| `src/drivers/mod.rs` | Driver factory and provider routing logic |
| `src/drivers/openai.rs` | OpenAI API integration (GPT-4, etc.) |
| `src/drivers/anthropic.rs` | Anthropic API integration (Claude) |
| `src/drivers/claude_code.rs` | Claude Code/Opus integration for coding tasks |
| `src/drivers/gemini.rs` | Google Gemini API integration |
| `src/drivers/vertex.rs` | Google Vertex AI integration |
| `src/drivers/qwen_code.rs` | Alibaba Qwen Code model |
| `src/drivers/copilot.rs` | Microsoft Copilot Pro integration |
| `src/drivers/fallback.rs` | Fallback driver selection when primary fails |
| `src/tool_runner.rs` | Built-in tool execution engine (bash, python, js, http, etc.) |
| `src/mcp.rs` | MCP (Model Context Protocol) client and server connections |
| `src/mcp_server.rs` | MCP server implementation for exposing OpenFang tools |
| `src/sandbox.rs` | WASM sandbox for untrusted skill/plugin execution |
| `src/workspace_sandbox.rs` | Subprocess sandbox with isolated workspace |
| `src/docker_sandbox.rs` | Docker container sandbox for tools |
| `src/web_fetch.rs` | HTTP fetch with browser emulation, content parsing |
| `src/web_search.rs` | Web search integration (Perplexity, etc.) |
| `src/web_cache.rs` | Cache for web fetch results (avoid redundant fetches) |
| `src/context_budget.rs` | Token count management and context window overflow handling |
| `src/context_overflow.rs` | Recovery strategies when context limit exceeded |
| `src/browser.rs` | Headless browser automation via Puppeteer/Playwright |
| `src/media_understanding.rs` | Image and video description via vision models |
| `src/image_gen.rs` | Image generation via DALL-E, Midjourney, etc. |
| `src/tts.rs` | Text-to-speech synthesis |
| `src/embedding.rs` | Text embedding for semantic search |
| `src/routing.rs` | Intelligent model selection (simple/medium/complex routing) |
| `src/kernel_handle.rs` | Trait for kernel callback interface (avoids circular deps) |
| `src/a2a/` | Agent-to-Agent protocol and task dispatch |
| `src/audit.rs` | Merkle hash chain audit trail for compliance |
| `src/think_filter.rs` | XML `<thinking>` tag removal before user delivery |
| `src/auth_cooldown.rs` | Rate limit detection and backoff for auth failures |
| `src/loop_guard.rs` | Detect and prevent infinite loops in agent execution |
| `src/provider_health.rs` | Health checks for LLM providers |
| `src/retry.rs` | Exponential backoff retry logic for transient failures |
| `src/python_runtime.rs` | Python subprocess execution with venv/pip management |
| `src/workspace_context.rs` | Current workspace directory and path resolution |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/drivers/` | LLM provider integrations (9 drivers) |
| `src/` | Runtime core, tools, sandboxing, streaming, MCP |

## For AI Agents

### Working In This Directory
- Edit `src/agent_loop.rs` to change agent execution flow (memory recall, tool execution, LLM calls)
- Edit `src/drivers/mod.rs` to add a new LLM provider or change routing logic
- New drivers inherit from `LlmDriver` trait in `src/llm_driver.rs` and implement `complete()` and `stream()`
- Tool execution is in `tool_runner.rs` — add new built-in tools there
- MCP connections are managed in `src/mcp.rs` — new tools can come from external MCP servers
- Sandboxing: WASM tools use `sandbox.rs`, shell commands use `workspace_sandbox.rs`, docker use `docker_sandbox.rs`
- Streaming responses flow through `stream_chunker.rs` and `stream_dedup.rs` in API
- Web tools (fetch, search, browser) all use `web_*.rs` modules — reuse cache and retry logic

### Testing Requirements
- Run `cargo build --workspace --lib && cargo test --workspace` after changes
- Agent loop tests: send message → verify tool execution → verify LLM response
- Driver tests: mock LLM API, verify request formatting and response parsing
- Tool tests: run actual bash/python/js commands, verify output
- For new LLM provider: implement `LlmDriver`, add to factory in `drivers/mod.rs`, test with live API key
- Context budget tests: verify truncation preserves tool results, handles nested tool calls
- Sandbox tests: verify untrusted code cannot escape

### Common Patterns
- `AgentLoopResult` contains `.response` (text), `.stop_reason` (end_turn|max_tokens|tool_use), `.usage` (tokens)
- LLM drivers all follow the same pattern: format request → call API → parse response → return `CompletionResponse`
- Streaming uses `tokio::sync::mpsc::Receiver<StreamEvent>` — consumer handles chunking and dedup
- Tool results are wrapped in `ContentBlock::ToolResult { id, content, is_error }` — errors prevent hallucination
- Context budget is enforced AFTER tool execution — tools can consume up to full window
- Retries use exponential backoff with jitter — don't saturate provider APIs
- All external HTTP calls go through `reqwest::Client` with custom User-Agent

## Dependencies

### Internal
- `openfang-types` — agent, message, tool, config types
- `openfang-memory` — agent memory recall and semantic search
- `openfang-skills` — plugin skills invocation

### External
- `tokio` — async runtime
- `reqwest` — HTTP client for all providers and web tools
- `serde_json` — JSON serialization
- `uuid`, `chrono` — IDs and timestamps
- `wasmtime` — WASM sandbox execution
- `rmcp` — MCP protocol client
- `rusqlite` — local SQLite for tool caching
- `tokio-tungstenite` — WebSocket for agent-to-agent
- `regex-lite` — lightweight regex for tool arg parsing
- `base64`, `sha2`, `hex` — encoding and hashing
- `zeroize` — secure cleanup of API keys
- `anyhow`, `thiserror` — error handling

<!-- MANUAL: -->
