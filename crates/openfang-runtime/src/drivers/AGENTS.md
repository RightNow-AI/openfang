<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# LLM Provider Drivers

## Purpose
Provider-specific LLM drivers implementing the unified `LlmDriver` trait. Each driver handles protocol details, streaming formats, error handling, and provider-specific features (vision, function calling, etc.).

## Key Files
| File | Purpose |
|------|---------|
| `mod.rs` | Driver registry and factory: maps provider name to driver instance, manages API keys and base URLs |
| `anthropic.rs` | Anthropic Claude API: streaming, vision, function calling, batches |
| `gemini.rs` | Google Gemini API: multi-modal, function calling, safety settings |
| `openai.rs` | OpenAI GPT API: chat completions, vision, function calling, streaming |
| `claude_code.rs` | Anthropic Claude Code API: extended token limits, code generation focus |
| `qwen_code.rs` | Alibaba Qwen Code API: code-specialized model |
| `vertex.rs` | Google Vertex AI: managed Claude API variant |
| `copilot.rs` | Microsoft Copilot API: authentication, model mapping |
| `fallback.rs` | Fallback driver: graceful degradation when primary fails |

## Supported Providers (via `mod.rs` and OpenAI-compatible routing)
- **Native drivers:** Anthropic, Gemini, OpenAI, Claude Code, Qwen Code, Vertex, Copilot, Fallback
- **OpenAI-compatible:** Groq, DeepSeek, Together, Mistral, Fireworks, OpenRouter, Ollama, vLLM, LM Studio, and 20+ others via base URL configuration

## Subdirectories
None — single-file drivers per provider for modularity.

## For AI Agents
When adding a new LLM provider:
1. Create `providers/<name>.rs` or add to `openai.rs` if compatible
2. Implement `LlmDriver` trait: `stream_completion()`, `parse_stream_event()`, error mapping
3. Register in `mod.rs` factory function with base URL and API key env var
4. Handle streaming format: JSON Lines, SSE, or custom delimiters
5. Map provider errors to `LlmError` enum for unified error handling
6. Add vision support if applicable (Claude, Gemini, GPT-4V)
7. Test live: set API key env var, send `agent_send` command, verify response streaming
8. Document required env vars and model names in config examples
