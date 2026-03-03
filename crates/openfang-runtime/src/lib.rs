//! Agent runtime and execution environment.
//!
//! Manages the agent execution loop, LLM driver abstraction,
//! tool execution, and WASM sandboxing for untrusted skill/plugin code.

pub mod a2a;
pub mod agent_loop;
pub mod apply_patch;
pub mod audit;
pub mod auth_cooldown;
pub mod browser;
pub mod command_lane;
pub mod compactor;
pub mod copilot_oauth;
pub mod context_budget;
pub mod context_overflow;
pub mod docker_sandbox;
pub mod drivers;
pub mod embedding;
pub mod graceful_shutdown;
pub mod hooks;
pub mod host_functions;
pub mod image_gen;
pub mod kernel_handle;
pub mod link_understanding;
pub mod llm_driver;
pub mod llm_errors;
pub mod loop_guard;
pub mod mcp;
pub mod mcp_server;
pub mod media_understanding;
pub mod model_catalog;
pub mod process_manager;
pub mod prompt_builder;
pub mod provider_health;
pub mod python_runtime;
pub mod reply_directives;
pub mod retry;
pub mod routing;
pub mod sandbox;
pub mod session_repair;
pub mod shell_bleed;
pub mod str_utils;
pub mod subprocess_sandbox;
pub mod tool_policy;
pub mod tool_runner;
pub mod tts;
pub mod web_cache;
pub mod web_content;
pub mod web_fetch;
pub mod web_search;
pub mod workspace_context;
pub mod workspace_sandbox;
