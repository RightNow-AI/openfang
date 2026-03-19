/*
 * @Author             : Felix
 * @Email              : 307253927@qq.com
 * @Date               : 2026-03-13 15:59:53
 * @LastEditors        : Felix
 * @LastEditTime       : 2026-03-19 14:09:20
 */
//! HTTP/WebSocket API server for the OpenFang Agent OS daemon.
//!
//! Exposes agent management, status, and chat via JSON REST endpoints.
//! The kernel runs in-process; the CLI connects over HTTP.

pub mod channel_bridge;
pub mod middleware;
pub mod openai_compat;
pub mod rate_limiter;
pub mod routes;
pub mod server;
pub mod session_auth;
pub mod stream_chunker;
pub mod stream_dedup;
pub mod types;
pub mod uni_agent;
pub mod uni_util;
pub mod unigpt;
pub mod webchat;
pub mod ws;
