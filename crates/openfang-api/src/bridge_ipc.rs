//! Daemon-side IPC server for the MCP bridge.
//!
//! ## Topology
//!
//! The bridge runs as a *grandchild* of the daemon:
//!
//! ```text
//! daemon (this process)
//!   └── claude            (CC subprocess, one per prompt)
//!         └── openfang-mcp-bridge   (CC spawns this from --mcp-config)
//!               └── ───── unix socket ─────► daemon (BridgeIpcServer)
//! ```
//!
//! Tools that need [`KernelHandle`](openfang_runtime::kernel_handle::KernelHandle)
//! (e.g. `agent_list`, `channel_send`) cannot run inside the bridge process;
//! it doesn't hold the kernel. The bridge forwards each MCP `tools/call`
//! over a unix-domain socket back here, where we dispatch into
//! `openfang_runtime::tool_runner::execute_tool` and ship the result back.
//!
//! ## Status — ANAI-30 step 2
//!
//! This module currently:
//! - Listens on `<home_dir>/run/bridge.sock`.
//! - Accepts the protocol [`Hello`](openfang_mcp_bridge::protocol::Hello)
//!   handshake (any non-empty token; real auth in ANAI-31).
//! - Decodes [`CallRequest`](openfang_mcp_bridge::protocol::CallRequest)
//!   frames, enforces the four-tool allowlist
//!   ([`ALLOWED_TOOLS`]: `file_read`, `file_list`, `agent_list`,
//!   `channel_send`), and dispatches into
//!   [`openfang_runtime::tool_runner::execute_tool`] with the kernel-bound
//!   context bundle. The shape mirrors the HTTP `/mcp` endpoint in
//!   `routes.rs` so the two execution paths stay in lockstep.
//!
//! Identity (`caller_agent_id`) is currently taken at face value from the
//! [`CallRequest::agent_id`] field. ANAI-31 replaces this with
//! token-derived identity bound at daemon-spawn time. Per-agent
//! capability gating (replacing the static [`ALLOWED_TOOLS`] allowlist
//! with `agent.toml` lookups) lands in the same ticket.

use openfang_kernel::OpenFangKernel;
use openfang_mcp_bridge::protocol::{
    CallRequest, CallResponse, CallResult, Frame, Hello, HelloAck, PROTOCOL_VERSION,
    SOCKET_RELATIVE_PATH, codec,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

/// Tools the bridge IPC server is willing to dispatch in the ANAI-30
/// validation slice. Anything outside this set is rejected at the
/// protocol layer (i.e. it never reaches `execute_tool`). ANAI-31 will
/// replace this static allowlist with per-agent capability lookups
/// driven by `agent.toml`.
///
/// The chosen four exercise the full diversity of tool dependencies:
/// - `file_read` / `file_list` — workspace-scoped, no kernel needed
/// - `agent_list` — requires [`KernelHandle::list_agents`]
/// - `channel_send` — requires [`KernelHandle::send_channel_message`],
///   one of the OpenFang-only capabilities a CC subprocess wouldn't
///   otherwise have
pub const ALLOWED_TOOLS: &[&str] = &["file_read", "file_list", "agent_list", "channel_send"];

/// Daemon-version string sent in [`HelloAck::Ok`].
fn daemon_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Resolve the bridge socket path under `home_dir`. Ensures the parent
/// directory exists.
pub fn socket_path(home_dir: &std::path::Path) -> std::io::Result<PathBuf> {
    let path = home_dir.join(SOCKET_RELATIVE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(path)
}

/// Handle to a running bridge IPC server. Drop / call [`BridgeIpcServer::shutdown`]
/// to stop accepting connections and remove the socket file.
pub struct BridgeIpcServer {
    socket_path: PathBuf,
    shutdown: Arc<Notify>,
}

impl BridgeIpcServer {
    /// Start the IPC listener. Returns once the socket is bound; the accept
    /// loop runs in a detached tokio task until shutdown is signaled.
    pub async fn start(kernel: Arc<OpenFangKernel>) -> std::io::Result<Self> {
        let socket_path = socket_path(&kernel.config.home_dir)?;

        // Remove any stale socket from a prior unclean shutdown. UnixListener
        // refuses to bind if the path exists, even if no one's listening.
        if socket_path.exists() {
            warn!(path = %socket_path.display(), "removing stale bridge socket");
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path)?;
        // Restrict to user-only — the socket is loopback to ourselves; no
        // reason for any other uid to connect.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&socket_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&socket_path, perms);
            }
        }

        info!(path = %socket_path.display(), "bridge IPC server listening");

        let shutdown = Arc::new(Notify::new());
        let accept_shutdown = shutdown.clone();
        let _accept_kernel = kernel.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = accept_shutdown.notified() => {
                        debug!("bridge IPC: accept loop shutting down");
                        break;
                    }
                    res = listener.accept() => {
                        match res {
                            Ok((stream, _addr)) => {
                                info!("bridge IPC: accepted connection");
                                let conn_kernel = _accept_kernel.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, conn_kernel).await {
                                        debug!(error = %e, "bridge IPC connection ended with error");
                                    }
                                });
                            }
                            Err(e) => {
                                error!(error = %e, "bridge IPC accept failed");
                                // Brief backoff to avoid spinning on a persistent error.
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            socket_path,
            shutdown,
        })
    }

    /// Path to the unix socket the bridge listens on. Used by the daemon
    /// to publish `OPENFANG_BRIDGE_SOCKET` for subprocess drivers (Claude
    /// Code, etc.) so they can wire CC's `--mcp-config` to point bridges
    /// back here.
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    /// Signal the accept loop to stop and remove the socket file.
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

impl Drop for BridgeIpcServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Handle a single bridge connection: Hello/HelloAck handshake, then a loop
/// of CallRequest → CallResponse frames until the peer closes.
async fn handle_connection(
    mut stream: UnixStream,
    kernel: Arc<OpenFangKernel>,
) -> std::io::Result<()> {
    let (read_half, mut write_half) = stream.split();
    let mut read_half = tokio::io::BufReader::new(read_half);

    // --- Handshake ---
    let hello = match codec::read_frame(&mut read_half).await? {
        Frame::Hello(h) => h,
        other => {
            warn!(?other, "bridge IPC: first frame was not Hello, closing");
            return Ok(());
        }
    };

    if let Err(reason) = validate_hello(&hello) {
        let ack = Frame::HelloAck(HelloAck::Rejected {
            reason: reason.clone(),
        });
        let _ = codec::write_frame(&mut write_half, &ack).await;
        warn!(reason, "bridge IPC: rejected handshake");
        return Ok(());
    }

    let ack = Frame::HelloAck(HelloAck::Ok {
        daemon_version: daemon_version(),
    });
    codec::write_frame(&mut write_half, &ack).await?;
    info!(
        bridge_version = %hello.bridge_version,
        "bridge IPC: handshake complete"
    );

    // --- Request loop ---
    loop {
        let frame = match codec::read_frame(&mut read_half).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                debug!("bridge IPC: peer closed");
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let call = match frame {
            Frame::Call(c) => c,
            other => {
                warn!(?other, "bridge IPC: unexpected frame in request loop");
                continue;
            }
        };

        info!(
            request_id = call.request_id,
            tool = %call.tool_name,
            agent = %call.agent_id,
            "bridge IPC: dispatching call"
        );

        let result = dispatch_call(&call, &kernel).await;
        let result_kind = match &result {
            CallResult::Ok { is_error: false, .. } => "ok",
            CallResult::Ok { is_error: true, .. } => "tool_error",
            CallResult::Error { .. } => "dispatch_error",
        };
        info!(
            request_id = call.request_id,
            tool = %call.tool_name,
            outcome = result_kind,
            "bridge IPC: call complete"
        );
        let response = Frame::Response(CallResponse {
            request_id: call.request_id,
            result,
        });
        codec::write_frame(&mut write_half, &response).await?;
    }
}

/// Dispatch a single bridge tool call to the runtime.
///
/// Enforces the [`ALLOWED_TOOLS`] allowlist before invoking
/// [`openfang_runtime::tool_runner::execute_tool`]. The argument bundle
/// mirrors the HTTP `/mcp` endpoint in `routes.rs` — keep them in sync;
/// they share semantics intentionally.
///
/// Returns:
/// - [`CallResult::Error`] for protocol-layer rejections (unknown tool,
///   not on the allowlist).
/// - [`CallResult::Ok`] for anything `execute_tool` returned, with
///   `is_error` propagated. A tool that ran but returned an error to
///   the model is `Ok { is_error: true }`, **not** `Error` — the latter
///   means the bridge couldn't even attempt dispatch.
async fn dispatch_call(call: &CallRequest, kernel: &Arc<OpenFangKernel>) -> CallResult {
    if !ALLOWED_TOOLS.iter().any(|t| *t == call.tool_name) {
        return CallResult::Error {
            message: format!(
                "tool '{}' not in bridge allowlist (permitted: {:?})",
                call.tool_name, ALLOWED_TOOLS
            ),
        };
    }

    // Snapshot the skill registry before crossing the await — its read
    // guard is !Send and execute_tool spans `.await` points internally.
    let skill_snapshot = kernel
        .skill_registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot();

    // Build the kernel handle. Cloning the Arc is cheap; the cast to
    // `dyn KernelHandle` is the same upcast the HTTP /mcp endpoint
    // performs.
    let kernel_handle: Arc<dyn openfang_runtime::kernel_handle::KernelHandle> =
        kernel.clone() as Arc<dyn openfang_runtime::kernel_handle::KernelHandle>;

    // execute_tool also enforces an allowlist via its `allowed_tools`
    // parameter; passing our four-tool set makes the runtime's check
    // belt-and-suspenders with ours. If the two ever drift, the runtime's
    // is authoritative — it sits closer to the actual tool implementations.
    let allowed_tools_owned: Vec<String> =
        ALLOWED_TOOLS.iter().map(|s| (*s).to_string()).collect();

    let result = openfang_runtime::tool_runner::execute_tool(
        &format!("bridge-{}", call.request_id),
        &call.tool_name,
        &call.args,
        Some(&kernel_handle),
        Some(&allowed_tools_owned),
        // Identity stub for ANAI-30: trust the bridge's claimed agent_id.
        // ANAI-31 replaces this with token-derived identity bound at
        // daemon-spawn time.
        Some(call.agent_id.as_str()),
        Some(&skill_snapshot),
        Some(&kernel.mcp_connections),
        Some(&kernel.web_ctx),
        Some(&kernel.browser_ctx),
        None, // allowed_env_vars — unused by the four allowlisted tools
        None, // workspace_root — file_read/file_list use input-relative paths
        Some(&kernel.media_engine),
        None, // exec_policy — shell tools not in allowlist
        if kernel.config.tts.enabled {
            Some(&kernel.tts_engine)
        } else {
            None
        },
        if kernel.config.docker.enabled {
            Some(&kernel.config.docker)
        } else {
            None
        },
        Some(&*kernel.process_manager),
    )
    .await;

    CallResult::Ok {
        content: result.content,
        is_error: result.is_error,
    }
}

/// Validate the bridge's Hello. Returns Err with a human-readable reason on
/// rejection. **Stub for ANAI-30**: we accept any non-empty token. ANAI-31
/// will replace this with a per-spawn token table populated when the daemon
/// spawns the parent CC subprocess.
fn validate_hello(hello: &Hello) -> Result<(), String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol version mismatch: bridge={} daemon={}",
            hello.protocol_version, PROTOCOL_VERSION
        ));
    }
    if hello.token.trim().is_empty() {
        return Err("empty auth token".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_mcp_bridge::protocol::{CallRequest, CallResult};
    use tokio::io::BufReader;
    use tokio::net::UnixStream as ClientStream;

    /// End-to-end wire-shape test: bind a listener at a tempfile path,
    /// connect, do the handshake, send two CallRequests:
    ///   1. A non-allowlisted tool — expect `CallResult::Error` from the
    ///      step-2 allowlist check.
    ///   2. An allowlisted tool — expect a canned `CallResult::Ok` from
    ///      the test twin (the real handler would dispatch into
    ///      `execute_tool`; we can't synthesize an `OpenFangKernel` here).
    ///
    /// What this test guarantees:
    /// - The Hello/HelloAck handshake stays correct.
    /// - The allowlist gate fires *before* dispatch (no kernel touched).
    /// - The wire framing for `CallResponse::Ok` and `CallResponse::Error`
    ///   round-trips cleanly.
    ///
    /// What this test does NOT cover (intentionally — needs a real kernel):
    /// - That `execute_tool` is invoked with the right argument bundle.
    /// - That tool results are correctly mapped to `CallResult::Ok`.
    /// Those land as integration tests once the daemon side spawns the
    /// bridge for real (ANAI-31).
    #[tokio::test]
    async fn ipc_handshake_and_allowlist_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("bridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection_test_twin(stream).await.unwrap();
        });

        let mut client = ClientStream::connect(&sock).await.unwrap();
        let (cr, mut cw) = client.split();
        let mut cr = BufReader::new(cr);

        // Handshake.
        let hello = Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "stub-token".into(),
            bridge_version: "test".into(),
        });
        codec::write_frame(&mut cw, &hello).await.unwrap();
        match codec::read_frame(&mut cr).await.unwrap() {
            Frame::HelloAck(HelloAck::Ok { .. }) => {}
            other => panic!("expected HelloAck::Ok, got {other:?}"),
        }

        // 1. Non-allowlisted tool → allowlist Error.
        codec::write_frame(
            &mut cw,
            &Frame::Call(CallRequest {
                request_id: 1,
                agent_id: "test-agent".into(),
                tool_name: "shell_exec".into(), // deliberately not on the list
                args: serde_json::json!({"cmd": "rm -rf /"}),
            }),
        )
        .await
        .unwrap();
        match codec::read_frame(&mut cr).await.unwrap() {
            Frame::Response(CallResponse {
                request_id: 1,
                result: CallResult::Error { message },
            }) => {
                assert!(
                    message.contains("not in bridge allowlist"),
                    "expected allowlist rejection, got: {message}"
                );
            }
            other => panic!("unexpected response to disallowed tool: {other:?}"),
        }

        // 2. Allowlisted tool → twin returns canned Ok.
        codec::write_frame(
            &mut cw,
            &Frame::Call(CallRequest {
                request_id: 2,
                agent_id: "test-agent".into(),
                tool_name: "file_read".into(),
                args: serde_json::json!({"path": "x"}),
            }),
        )
        .await
        .unwrap();
        match codec::read_frame(&mut cr).await.unwrap() {
            Frame::Response(CallResponse {
                request_id: 2,
                result: CallResult::Ok { is_error, .. },
            }) => {
                // Twin canned response is a non-error Ok; the real handler
                // would set `is_error` from `execute_tool`'s ToolResult.
                assert!(!is_error);
            }
            other => panic!("unexpected response to allowed tool: {other:?}"),
        }

        drop(client);
        server.await.unwrap();
    }

    /// Test-only twin of [`handle_connection`].
    ///
    /// Mirrors the production handler's *wire* behavior (handshake +
    /// request loop + allowlist gate) but stubs the runtime dispatch
    /// because we can't synthesize an `OpenFangKernel` in unit tests.
    /// If the production handler's wire shape diverges, update this twin.
    async fn handle_connection_test_twin(mut stream: UnixStream) -> std::io::Result<()> {
        let (read_half, mut write_half) = stream.split();
        let mut read_half = BufReader::new(read_half);

        let hello = match codec::read_frame(&mut read_half).await? {
            Frame::Hello(h) => h,
            _ => return Ok(()),
        };
        if let Err(reason) = validate_hello(&hello) {
            let _ = codec::write_frame(
                &mut write_half,
                &Frame::HelloAck(HelloAck::Rejected { reason }),
            )
            .await;
            return Ok(());
        }
        codec::write_frame(
            &mut write_half,
            &Frame::HelloAck(HelloAck::Ok {
                daemon_version: daemon_version(),
            }),
        )
        .await?;

        loop {
            let frame = match codec::read_frame(&mut read_half).await {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e),
            };
            let call = match frame {
                Frame::Call(c) => c,
                _ => continue,
            };

            // Mirror production allowlist logic.
            let result = if !ALLOWED_TOOLS.iter().any(|t| *t == call.tool_name) {
                CallResult::Error {
                    message: format!(
                        "tool '{}' not in bridge allowlist (permitted: {:?})",
                        call.tool_name, ALLOWED_TOOLS
                    ),
                }
            } else {
                // Canned Ok stand-in for `execute_tool` — kernel-free tests
                // can't exercise the real dispatch path.
                CallResult::Ok {
                    content: format!("[test-twin canned ok for {}]", call.tool_name),
                    is_error: false,
                }
            };

            codec::write_frame(
                &mut write_half,
                &Frame::Response(CallResponse {
                    request_id: call.request_id,
                    result,
                }),
            )
            .await?;
        }
    }

    #[test]
    fn validate_hello_rejects_version_mismatch() {
        let h = Hello {
            protocol_version: 999,
            token: "x".into(),
            bridge_version: "t".into(),
        };
        assert!(validate_hello(&h).is_err());
    }

    #[test]
    fn validate_hello_rejects_empty_token() {
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "".into(),
            bridge_version: "t".into(),
        };
        assert!(validate_hello(&h).is_err());
    }

    #[test]
    fn validate_hello_accepts_nonempty_token() {
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "tok".into(),
            bridge_version: "t".into(),
        };
        assert!(validate_hello(&h).is_ok());
    }
}
