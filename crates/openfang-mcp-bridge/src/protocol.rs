//! Wire protocol for daemon ↔ bridge IPC.
//!
//! The bridge runs as a grandchild of the daemon (daemon → claude → bridge).
//! Tools that need kernel access (`agent_list`, `channel_send`, etc.) cannot
//! be served from the bridge process directly — it doesn't hold a
//! [`KernelHandle`]. Instead the bridge forwards each call over a unix-domain
//! socket back to the daemon, which dispatches into
//! `openfang_runtime::tool_runner::execute_tool` and ships the result back.
//!
//! This module defines the wire shape of that exchange. It is intentionally
//! the *only* surface shared between the bridge crate and the daemon — both
//! sides depend on these types and nothing else.
//!
//! ## Framing
//!
//! Each message is a 4-byte big-endian length prefix followed by that many
//! bytes of UTF-8 JSON. No nested length fields, no streaming. Messages are
//! capped at [`MAX_FRAME_BYTES`] to bound memory; oversized frames are an
//! error and the connection is closed.
//!
//! ## Versioning
//!
//! [`PROTOCOL_VERSION`] is sent in the [`Hello`] message at connection start.
//! Mismatches close the connection. The protocol is private to OpenFang —
//! versioning here is for our own evolution, not external compatibility.

use serde::{Deserialize, Serialize};

/// Wire protocol version. Bumped on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum size of a single framed message, in bytes (1 MiB).
///
/// Tool results that exceed this are truncated by the daemon before framing.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Default unix socket path, relative to the OpenFang home directory.
///
/// Resolved at runtime as `<home_dir>/run/bridge.sock`.
pub const SOCKET_RELATIVE_PATH: &str = "run/bridge.sock";

/// Environment variable name used to pass the socket path from daemon to bridge.
pub const SOCKET_ENV_VAR: &str = "OPENFANG_BRIDGE_SOCKET";

/// Environment variable name used to pass the per-spawn auth token from
/// daemon to bridge. (Identity binding lands in ANAI-31; for ANAI-30 the
/// agent_id is sent in-band in [`CallRequest`] as a stub.)
pub const TOKEN_ENV_VAR: &str = "OPENFANG_BRIDGE_TOKEN";

/// Bridge → daemon: opening message on a fresh connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol_version: u32,
    /// Per-spawn auth token. Validated by daemon against an in-memory map
    /// populated when the daemon spawned the parent CC subprocess. Stubbed
    /// for ANAI-30 — daemon currently accepts any non-empty token and
    /// expects [`CallRequest::agent_id`] to identify the caller.
    pub token: String,
    /// Bridge build version, for debug/audit.
    pub bridge_version: String,
}

/// Daemon → bridge: response to [`Hello`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HelloAck {
    Ok {
        daemon_version: String,
    },
    /// Connection rejected. Bridge should log and exit.
    Rejected {
        reason: String,
    },
}

/// Bridge → daemon: a single tool call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRequest {
    /// Caller-assigned correlation id. Daemon echoes in [`CallResponse::request_id`].
    pub request_id: u64,
    /// Identity of the caller agent (the parent that spawned this CC subprocess).
    ///
    /// **Stub for ANAI-30.** ANAI-31 replaces this with token-derived identity
    /// validated server-side; do not rely on this field for security.
    pub agent_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

/// Daemon → bridge: response to a [`CallRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResponse {
    pub request_id: u64,
    pub result: CallResult,
}

/// Outcome of a tool dispatch. Maps directly onto MCP's `CallToolResult`
/// shape (text content + `isError` flag) so the bridge can forward without
/// further translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallResult {
    /// Tool executed; `is_error` follows OpenFang's `ToolResult` semantics
    /// (true ⇒ tool ran but reported an error to the LLM).
    Ok { content: String, is_error: bool },
    /// Tool dispatch failed at the protocol layer (unknown tool, not
    /// permitted, malformed args, internal panic). Distinct from `Ok { is_error: true }`,
    /// which means the tool itself returned an error result.
    Error { message: String },
}

/// Bridge → daemon: request the list of upstream MCP tools the calling
/// agent is permitted to invoke.
///
/// Sent once per session, immediately after a successful [`Hello`]/[`HelloAck::Ok`]
/// handshake. The daemon answers with [`UpstreamListResponse`].
///
/// Identity is taken from the authenticated [`Hello::token`] (resolved
/// server-side to an `agent_id`); the bridge does not name the agent in
/// this frame. Per-agent gating is enforced server-side against the
/// agent's `agent.toml mcp_servers` allowlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUpstreamRequest {
    /// Caller-assigned correlation id. Daemon echoes in
    /// [`UpstreamListResponse::request_id`].
    pub request_id: u64,
}

/// One forwarded upstream MCP tool, as advertised to the bridge.
///
/// The `name` is already namespaced (`mcp_{server}_{tool}`) by the daemon
/// at MCP-discovery time; the bridge surfaces this name verbatim in its
/// own `tools/list` and routes invocations of the same name back across
/// the IPC.
///
/// `input_schema` is the upstream server's JSON Schema for the tool, passed
/// through opaquely — the daemon does not validate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamToolDef {
    /// Already-namespaced tool name (e.g. `mcp_linear_getteams`).
    pub name: String,
    /// Logical server name from `config.toml` (e.g. `linear`). Used by
    /// the bridge for grouping/debug; not parsed from `name` on receive.
    pub server: String,
    /// Human-readable description, forwarded from the upstream MCP server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Opaque JSON Schema for the tool's input. Forwarded verbatim.
    pub input_schema: serde_json::Value,
}

/// Daemon → bridge: response to [`ListUpstreamRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamListResponse {
    pub request_id: u64,
    pub result: UpstreamListResult,
}

/// Outcome of an upstream tool-list dispatch.
///
/// `Error` is reserved for protocol-layer failures (agent identity not
/// resolvable, registry unavailable). An agent that simply has no upstream
/// servers configured receives `Ok { tools: vec![] }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamListResult {
    Ok { tools: Vec<UpstreamToolDef> },
    Error { message: String },
}

/// Top-level frame type, for connections that may carry multiple message kinds.
///
/// At present we only multiplex Hello/HelloAck on connection start and
/// CallRequest/CallResponse thereafter. A single enum keeps the framing
/// uniform and leaves room for future message types (e.g. cancel, ping)
/// without renegotiating the wire shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    Hello(Hello),
    HelloAck(HelloAck),
    Call(CallRequest),
    Response(CallResponse),
    /// Bridge → daemon: request the per-agent upstream MCP tool list.
    ///
    /// Added in protocol v1 as a non-breaking variant; older builds that
    /// predate this variant reject the frame as an unknown tag and close
    /// the connection. Daemon and bridge ship from the same workspace,
    /// so version skew here would indicate a build-system error.
    ListUpstream(ListUpstreamRequest),
    /// Daemon → bridge: response to [`Frame::ListUpstream`].
    UpstreamList(UpstreamListResponse),
}

#[cfg(feature = "ipc-codec")]
pub mod codec {
    //! Async length-prefixed framing helpers. Gated behind the `ipc-codec`
    //! feature so the bare protocol types stay usable in `no-tokio` contexts
    //! (tests, type-only consumers).

    use super::{Frame, MAX_FRAME_BYTES};
    use std::io;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Read one length-prefixed JSON frame from `r`.
    pub async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> io::Result<Frame> {
        let len = r.read_u32().await? as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame size {len} out of bounds (max {MAX_FRAME_BYTES})"),
            ));
        }
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decode: {e}")))
    }

    /// Write one length-prefixed JSON frame to `w`.
    pub async fn write_frame<W: AsyncWriteExt + Unpin>(w: &mut W, frame: &Frame) -> io::Result<()> {
        let bytes = serde_json::to_vec(frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("encode: {e}")))?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "frame size {} exceeds MAX_FRAME_BYTES {}",
                    bytes.len(),
                    MAX_FRAME_BYTES
                ),
            ));
        }
        w.write_u32(bytes.len() as u32).await?;
        w.write_all(&bytes).await?;
        w.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_call() {
        let frame = Frame::Call(CallRequest {
            request_id: 42,
            agent_id: "coder-openfang".to_string(),
            tool_name: "file_read".to_string(),
            args: serde_json::json!({ "path": "Cargo.toml" }),
        });
        let json = serde_json::to_string(&frame).unwrap();
        let back: Frame = serde_json::from_str(&json).unwrap();
        match back {
            Frame::Call(c) => {
                assert_eq!(c.request_id, 42);
                assert_eq!(c.tool_name, "file_read");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn frame_roundtrip_response_ok() {
        let frame = Frame::Response(CallResponse {
            request_id: 7,
            result: CallResult::Ok {
                content: "hello".into(),
                is_error: false,
            },
        });
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"ok\""));
        let back: Frame = serde_json::from_str(&s).unwrap();
        if let Frame::Response(r) = back {
            assert_eq!(r.request_id, 7);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn hello_ack_rejected_serializes() {
        let f = Frame::HelloAck(HelloAck::Rejected {
            reason: "bad token".into(),
        });
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("rejected"));
        assert!(s.contains("bad token"));
    }

    #[test]
    fn frame_roundtrip_list_upstream_request() {
        let frame = Frame::ListUpstream(ListUpstreamRequest { request_id: 99 });
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("list_upstream"));
        let back: Frame = serde_json::from_str(&json).unwrap();
        match back {
            Frame::ListUpstream(r) => assert_eq!(r.request_id, 99),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn frame_roundtrip_upstream_list_ok() {
        let frame = Frame::UpstreamList(UpstreamListResponse {
            request_id: 99,
            result: UpstreamListResult::Ok {
                tools: vec![UpstreamToolDef {
                    name: "mcp_linear_getteams".into(),
                    server: "linear".into(),
                    description: Some("List Linear teams".into()),
                    input_schema: serde_json::json!({ "type": "object" }),
                }],
            },
        });
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("upstream_list"));
        assert!(json.contains("mcp_linear_getteams"));
        let back: Frame = serde_json::from_str(&json).unwrap();
        match back {
            Frame::UpstreamList(r) => {
                assert_eq!(r.request_id, 99);
                match r.result {
                    UpstreamListResult::Ok { tools } => {
                        assert_eq!(tools.len(), 1);
                        assert_eq!(tools[0].server, "linear");
                    }
                    _ => panic!("wrong result variant"),
                }
            }
            _ => panic!("wrong frame variant"),
        }
    }

    #[test]
    fn frame_roundtrip_upstream_list_empty() {
        // Agent with no upstream MCP servers configured receives an empty
        // Ok list, not an Error. Guards against the bridge treating
        // "no servers" as a fatal handshake failure.
        let frame = Frame::UpstreamList(UpstreamListResponse {
            request_id: 1,
            result: UpstreamListResult::Ok { tools: vec![] },
        });
        let json = serde_json::to_string(&frame).unwrap();
        let back: Frame = serde_json::from_str(&json).unwrap();
        if let Frame::UpstreamList(r) = back {
            match r.result {
                UpstreamListResult::Ok { tools } => assert!(tools.is_empty()),
                _ => panic!("wrong result variant"),
            }
        } else {
            panic!("wrong frame variant");
        }
    }

    #[test]
    fn frame_roundtrip_upstream_list_error() {
        let frame = Frame::UpstreamList(UpstreamListResponse {
            request_id: 7,
            result: UpstreamListResult::Error {
                message: "agent identity not resolvable".into(),
            },
        });
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("error"));
        let back: Frame = serde_json::from_str(&json).unwrap();
        if let Frame::UpstreamList(r) = back {
            match r.result {
                UpstreamListResult::Error { message } => {
                    assert!(message.contains("not resolvable"));
                }
                _ => panic!("wrong result variant"),
            }
        } else {
            panic!("wrong frame variant");
        }
    }

    #[test]
    fn upstream_tool_def_omits_none_description() {
        let def = UpstreamToolDef {
            name: "mcp_notion_search".into(),
            server: "notion".into(),
            description: None,
            input_schema: serde_json::json!({}),
        };
        let json = serde_json::to_string(&def).unwrap();
        // skip_serializing_if drops the field entirely when None.
        assert!(!json.contains("description"));
        // And it round-trips back as None.
        let back: UpstreamToolDef = serde_json::from_str(&json).unwrap();
        assert!(back.description.is_none());
    }

    #[test]
    fn frame_unknown_variant_is_rejected_cleanly() {
        // Forward-compat guard: a frame with an unknown `type` tag must
        // produce a serde error, not a panic. The IPC reader maps this
        // to an io::Error and closes the connection — but the test here
        // is just that decode fails without unwinding.
        let json = r#"{"type":"future_kind","payload":{}}"#;
        let result: Result<Frame, _> = serde_json::from_str(json);
        assert!(result.is_err(), "expected decode failure on unknown tag");
    }
}
