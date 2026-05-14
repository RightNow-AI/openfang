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

use crate::bridge_auth::BridgeAuthority;
use openfang_kernel::OpenFangKernel;
use openfang_mcp_bridge::protocol::{
    CallRequest, CallResponse, CallResult, Frame, Hello, HelloAck, PROTOCOL_VERSION,
    SOCKET_RELATIVE_PATH, codec,
};
use openfang_types::agent::AgentId;
use openfang_types::bridge_auth::Token;
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
    ///
    /// `authority` is the daemon's [`BridgeAuthority`], cloned into each
    /// accepted connection so the handshake can resolve presented tokens to
    /// the [`AgentId`] they were issued for. See [`authenticate_hello`].
    pub async fn start(
        kernel: Arc<OpenFangKernel>,
        authority: Arc<BridgeAuthority>,
    ) -> std::io::Result<Self> {
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
        let accept_authority = authority.clone();

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
                                let conn_authority = accept_authority.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, conn_kernel, conn_authority).await {
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

/// Resolved identity for a connected bridge after a successful handshake.
///
/// - `agent_id == Some(_)` is the **hardened path**: the bridge presented a
///   well-formed token that the [`BridgeAuthority`] resolved to a live spawn.
///   `dispatch_call` will substitute this id for the (untrusted)
///   `CallRequest::agent_id` field on every subsequent call on the
///   connection.
/// - `agent_id == None` is the **legacy path**: the token is non-empty but
///   not in 64-hex form, so it can't be a daemon-issued token. We accept it
///   for back-compat with drivers built before [`TokenIssuer`] wiring
///   reached every spawn site (the daemon's boot-time `create_driver`
///   calls in `boot_with_config` still emit legacy UUIDs because they run
///   before `set_token_issuer`). In this mode the bridge's claimed
///   `agent_id` is taken at face value — same trust model as ANAI-30. The
///   legacy lane is closed in the next phase by fixing boot ordering and
///   then making `authenticate_hello` strict.
///
/// `token_fingerprint` is the first 32 bits of the resolved token, suitable
/// for log correlation. `None` on the legacy path.
#[derive(Debug)]
struct HandshakeIdentity {
    agent_id: Option<AgentId>,
    token_fingerprint: Option<String>,
}

/// Handle a single bridge connection: Hello/HelloAck handshake, then a loop
/// of CallRequest → CallResponse frames until the peer closes.
async fn handle_connection(
    mut stream: UnixStream,
    kernel: Arc<OpenFangKernel>,
    authority: Arc<BridgeAuthority>,
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

    let identity = match authenticate_hello(&hello, &authority) {
        Ok(id) => id,
        Err(reason) => {
            let ack = Frame::HelloAck(HelloAck::Rejected {
                reason: reason.clone(),
            });
            let _ = codec::write_frame(&mut write_half, &ack).await;
            warn!(reason, "bridge IPC: rejected handshake");
            return Ok(());
        }
    };

    let ack = Frame::HelloAck(HelloAck::Ok {
        daemon_version: daemon_version(),
    });
    codec::write_frame(&mut write_half, &ack).await?;
    let authed_display = identity
        .agent_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "<legacy-unauthenticated>".to_string());
    let fingerprint_display = identity
        .token_fingerprint
        .clone()
        .unwrap_or_else(|| "<legacy>".to_string());
    info!(
        bridge_version = %hello.bridge_version,
        token_fingerprint = %fingerprint_display,
        authenticated_agent = %authed_display,
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

        let result = dispatch_call(&call, &kernel, identity.agent_id.as_ref()).await;
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
async fn dispatch_call(
    call: &CallRequest,
    kernel: &Arc<OpenFangKernel>,
    authenticated_agent_id: Option<&AgentId>,
) -> CallResult {
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

    // Identity selection:
    // - Hardened path (`authenticated_agent_id == Some`): use the agent_id
    //   the [`BridgeAuthority`] resolved from the handshake token. The
    //   bridge's claimed `CallRequest::agent_id` is *ignored* for
    //   authorization; we only `warn!` if it disagrees with the resolved
    //   identity, since the disagreement is either a bug in the bridge or
    //   a spoofing attempt.
    // - Legacy path (`authenticated_agent_id == None`): no daemon-issued
    //   token was presented, so we fall back to the ANAI-30 behavior of
    //   trusting the bridge's claimed agent_id. This lane closes in the
    //   next phase once every spawn site issues a real token.
    let resolved_agent_id_string: String = match authenticated_agent_id {
        Some(authed) => {
            let authed_str = authed.to_string();
            if authed_str != call.agent_id {
                warn!(
                    request_id = call.request_id,
                    tool = %call.tool_name,
                    claimed = %call.agent_id,
                    authenticated = %authed_str,
                    "bridge IPC: claimed agent_id disagrees with authenticated identity; \
                     using authenticated identity"
                );
            }
            authed_str
        }
        None => call.agent_id.clone(),
    };

    let result = openfang_runtime::tool_runner::execute_tool(
        &format!("bridge-{}", call.request_id),
        &call.tool_name,
        &call.args,
        Some(&kernel_handle),
        Some(&allowed_tools_owned),
        Some(resolved_agent_id_string.as_str()),
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

/// Authenticate the bridge's Hello against the daemon's [`BridgeAuthority`].
///
/// Decision tree:
/// - Version mismatch → `Err` (existing rejection).
/// - Empty/whitespace token → `Err` (existing rejection).
/// - Token parses as 64-hex (`Token::from_hex`) → resolve via authority:
///   - `Some(agent_id)` → `Ok(HandshakeIdentity { agent_id: Some(_), ... })`
///     — hardened path; this is a daemon-issued, live token.
///   - `None` → `Err` — well-formed hex that the authority never issued.
///     This is the attacker / replay / stale-token rejection.
/// - Token is non-empty but not 64-hex → `Ok(HandshakeIdentity { agent_id:
///   None, .. })` — legacy back-compat lane. Logs `debug!` so the operator
///   can see how many legacy handshakes are still happening.
fn authenticate_hello(
    hello: &Hello,
    authority: &BridgeAuthority,
) -> Result<HandshakeIdentity, String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol version mismatch: bridge={} daemon={}",
            hello.protocol_version, PROTOCOL_VERSION
        ));
    }
    let presented = hello.token.trim();
    if presented.is_empty() {
        return Err("empty auth token".to_string());
    }

    match Token::from_hex(presented) {
        Ok(token) => {
            let fingerprint = token.fingerprint();
            match authority.resolve(&token) {
                Some(agent_id) => Ok(HandshakeIdentity {
                    agent_id: Some(agent_id),
                    token_fingerprint: Some(fingerprint),
                }),
                None => Err(format!(
                    "unknown bridge token (fingerprint={fingerprint}); \
                     the daemon never issued this token or its spawn has terminated"
                )),
            }
        }
        Err(_) => {
            // Non-hex tokens are still accepted for back-compat with drivers
            // built before TokenIssuer wiring reached every spawn site. The
            // bridge's claimed agent_id is taken at face value in this mode.
            debug!(
                "bridge IPC: legacy-format auth token (not 64-hex); \
                 falling back to self-claimed agent_id"
            );
            Ok(HandshakeIdentity {
                agent_id: None,
                token_fingerprint: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_mcp_bridge::protocol::{CallRequest, CallResult};
    use openfang_runtime::bridge_auth::TokenIssuer;
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

        // Spin up a real authority and issue a token for an agent. The twin
        // resolves the handshake token through it, exercising the hardened
        // auth path end-to-end (handshake → resolve → AgentId binding).
        let authority = BridgeAuthority::new();
        let agent_id = AgentId::new();
        let guard = authority.issue(agent_id);
        let presented_token = guard.token().to_hex();

        let server_authority = authority.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection_test_twin(stream, server_authority)
                .await
                .unwrap();
        });

        let mut client = ClientStream::connect(&sock).await.unwrap();
        let (cr, mut cw) = client.split();
        let mut cr = BufReader::new(cr);

        // Handshake — real hex token resolves to `agent_id` via authority.
        let hello = Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            token: presented_token,
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

        // Token guard outlives the twin's reads. Drop now so the spawn
        // table empties before we exit the test (sanity check on lifetimes).
        drop(guard);
        assert_eq!(authority.live_spawn_count(), 0);
    }

    /// Test-only twin of [`handle_connection`].
    ///
    /// Mirrors the production handler's *wire* behavior (handshake +
    /// request loop + allowlist gate) but stubs the runtime dispatch
    /// because we can't synthesize an `OpenFangKernel` in unit tests.
    /// If the production handler's wire shape diverges, update this twin.
    async fn handle_connection_test_twin(
        mut stream: UnixStream,
        authority: Arc<BridgeAuthority>,
    ) -> std::io::Result<()> {
        let (read_half, mut write_half) = stream.split();
        let mut read_half = BufReader::new(read_half);

        let hello = match codec::read_frame(&mut read_half).await? {
            Frame::Hello(h) => h,
            _ => return Ok(()),
        };
        if let Err(reason) = authenticate_hello(&hello, &authority) {
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
    fn authenticate_hello_rejects_version_mismatch() {
        let authority = BridgeAuthority::new();
        let h = Hello {
            protocol_version: 999,
            token: "x".into(),
            bridge_version: "t".into(),
        };
        assert!(authenticate_hello(&h, &authority).is_err());
    }

    #[test]
    fn authenticate_hello_rejects_empty_token() {
        let authority = BridgeAuthority::new();
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "".into(),
            bridge_version: "t".into(),
        };
        assert!(authenticate_hello(&h, &authority).is_err());
    }

    #[test]
    fn authenticate_hello_resolves_authority_token() {
        // Hardened path: hex-encoded daemon-issued token → AgentId.
        let authority = BridgeAuthority::new();
        let agent_id = AgentId::new();
        let guard = authority.issue(agent_id);
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: guard.token().to_hex(),
            bridge_version: "t".into(),
        };
        let identity =
            authenticate_hello(&h, &authority).expect("hardened path should succeed");
        assert_eq!(identity.agent_id, Some(agent_id));
        assert_eq!(identity.token_fingerprint, Some(guard.fingerprint()));
    }

    #[test]
    fn authenticate_hello_rejects_unknown_hex_token() {
        // Well-formed 64-hex token the authority never issued — attacker /
        // replay / stale-spawn case. Must be rejected outright; no legacy
        // fallback for well-formed-but-unknown tokens.
        let authority = BridgeAuthority::new();
        let stranger = Token::generate();
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: stranger.to_hex(),
            bridge_version: "t".into(),
        };
        let err = authenticate_hello(&h, &authority).expect_err("unknown hex must reject");
        assert!(
            err.contains("unknown bridge token"),
            "expected unknown-token rejection, got: {err}"
        );
    }

    #[test]
    fn authenticate_hello_accepts_legacy_non_hex_token() {
        // Back-compat lane: a non-hex non-empty token (e.g. legacy UUID)
        // resolves to `agent_id: None`, signaling the dispatcher to fall
        // back to the bridge's self-claimed agent_id. Closes when every
        // spawn site issues real tokens.
        let authority = BridgeAuthority::new();
        let h = Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "550e8400-e29b-41d4-a716-446655440000".into(),
            bridge_version: "t".into(),
        };
        let identity =
            authenticate_hello(&h, &authority).expect("legacy path should succeed");
        assert!(identity.agent_id.is_none(), "legacy path must not bind agent_id");
        assert!(
            identity.token_fingerprint.is_none(),
            "legacy path has no fingerprint"
        );
    }
}
