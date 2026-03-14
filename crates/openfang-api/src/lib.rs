//! HTTP/WebSocket API server for the OpenFang Agent OS daemon.
//!
//! Exposes agent management, status, and chat via JSON REST endpoints.
//! The kernel runs in-process; the CLI connects over HTTP.

pub mod api_response;
pub mod auth;
pub mod channel_bridge;
pub mod local_inference;
pub mod middleware;
pub mod openai_compat;
pub mod openapi;
pub mod rate_limiter;
pub mod request_context;
pub mod routes;
pub mod security;
pub mod server;
pub mod stream_chunker;
pub mod stream_dedup;
pub mod types;
pub mod webchat;
pub mod ws;
