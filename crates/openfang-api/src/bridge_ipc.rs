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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

/// Tools the bridge IPC server is willing to dispatch. Anything outside
/// this set is rejected at the protocol layer (i.e. it never reaches
/// `execute_tool`). This is the daemon-side ceiling on the bridge surface
/// — the bridge subprocess's `built_in_tools` and per-spawn
/// `OPENFANG_BRIDGE_ALLOWED` are the layers that narrow it further per
/// agent (sourced from `agent.toml`).
///
/// Coverage exercises the full diversity of tool dependencies:
/// - `file_read` / `file_list` — workspace-scoped, no kernel needed
/// - `agent_list` — requires [`KernelHandle::list_agents`]
/// - `channel_send` — requires [`KernelHandle::send_channel_message`],
///   one of the OpenFang-only capabilities a CC subprocess wouldn't
///   otherwise have
/// - `agent_send` — inter-agent messaging via the kernel
pub const ALLOWED_TOOLS: &[&str] = &[
    "file_read",
    "file_list",
    "file_write",
    "create_directory",
    "web_fetch",
    "agent_list",
    "channel_send",
    "agent_send",
    "agent_spawn",
    "agent_kill",
    "memory_store",
    "memory_recall",
    "agent_activate",
    "agent_find",
    "shell_exec",
    "web_search",
    "apply_patch",
];

/// Subset of [`ALLOWED_TOOLS`] that operates on the agent's workspace
/// filesystem. These tools MUST be invoked with a sandbox-scoping
/// `workspace_root` — see the sandbox check in [`dispatch_call`].
///
/// History: prior to D-fix, the bridge passed `workspace_root: None` to
/// `execute_tool`, which fell through `resolve_file_path`'s "legacy"
/// branch and resolved paths against the daemon CWD (`~/.openfang`). That
/// let any agent with `file_read`/`file_list` advertised on its surface
/// read every sibling workspace plus `secrets.env` and the GCP service-
/// account JSON sitting at the openfang root. The fix below scopes every
/// FS call to the *authenticated* agent's workspace and refuses the call
/// outright when no workspace is registered.
/// `shell_exec` is included here because the command runs with
/// `current_dir(workspace_root)` (tool_runner.rs:1704-1707). Without a
/// registered workspace the shell would default to the daemon CWD
/// (`~/.openfang`), where `secrets.env` and the GCP service-account JSON
/// live — same sibling-leak surface the file tools had pre-D-fix. Refusing
/// the call when no workspace is registered keeps that closed.
/// `apply_patch` is included for the same reason: `tool_apply_patch`
/// resolves every patch-embedded path (Add / Update / Delete) against
/// `workspace_root`. Without a registered workspace, those paths fall
/// through to the daemon CWD and an attacker-crafted patch could touch
/// `secrets.env` or any sibling workspace. Fail-closed gate.
// Canonical FS-sandbox gate lives in `openfang_runtime::tool_runner` so
// the IPC and HTTP `/mcp` surfaces consult one source. Re-exported under
// the original module path to keep existing call sites (and tests at
// :1024,1035 pre-unification) compiling unchanged.
pub use openfang_runtime::tool_runner::FS_SANDBOXED_TOOLS;

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
///   for back-compat with non-unix builds and any caller that constructs a
///   kernel via `boot_with_config` (no issuer) — tests, desktop embeds, CLI
///   one-shots. Phase E closed the daemon-side boot ordering loophole, so on
///   a unix daemon every spawn site now sees an issuer and well-formed hex
///   tokens are the norm. In this legacy mode the bridge's claimed `agent_id`
///   is taken at face value — same trust model as ANAI-30. A future strict
///   mode can reject this arm outright once all supported deployments are on
///   the hardened path.
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
    // --- Gate 1: static bridge-surface allowlist ----------------------------
    // The hard ceiling on what the bridge will ever dispatch. Independent of
    // any agent's per-agent surface; an unknown tool never reaches identity
    // resolution.
    if !ALLOWED_TOOLS.iter().any(|t| *t == call.tool_name) {
        return CallResult::Error {
            message: format!(
                "tool '{}' not in bridge allowlist (permitted: {:?})",
                call.tool_name, ALLOWED_TOOLS
            ),
        };
    }

    // --- Identity resolution (fail-closed) ---------------------------------
    // Hardened path: handshake-bound AgentId from BridgeAuthority. Legacy
    // path: parse the bridge's self-claimed `call.agent_id`. Either way we
    // require a *registered* AgentId before proceeding — string identifiers
    // and unknown agents never feed authorization. Closes the ANAI-30
    // "trust the claimed string" loophole: a parseable but unregistered id
    // (random UUID-shaped value) now rejects instead of falling through.
    let resolved_agent_id: AgentId = match authenticated_agent_id {
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
            *authed
        }
        None => match call.agent_id.parse::<AgentId>() {
            Ok(aid) => aid,
            Err(_) => {
                warn!(
                    request_id = call.request_id,
                    tool = %call.tool_name,
                    claimed = %call.agent_id,
                    "bridge IPC: rejecting call — legacy lane and claimed agent_id \
                     does not parse as AgentId"
                );
                return CallResult::Error {
                    message: "unresolvable agent identity for bridge call".to_string(),
                };
            }
        },
    };
    let resolved_agent_id_string = resolved_agent_id.to_string();

    // Registry entry is the source of truth for capabilities and workspace.
    // Missing entry → fail closed (spoofed AgentId, dead spawn, etc.).
    let entry = match kernel.registry.get(resolved_agent_id) {
        Some(e) => e,
        None => {
            warn!(
                request_id = call.request_id,
                tool = %call.tool_name,
                agent = %resolved_agent_id_string,
                "bridge IPC: rejecting call — no registry entry for resolved agent"
            );
            return CallResult::Error {
                message: format!(
                    "agent '{resolved_agent_id_string}' has no registry entry; refusing call"
                ),
            };
        }
    };

    // --- Workspace-aware skill snapshot ------------------------------------
    // Mirrors the agent_loop pattern (kernel.rs:2192-2210): bundled + global
    // + workspace skills, in that override order. We reuse this snapshot for
    // BOTH the per-agent permission gate below AND `execute_tool` later — so
    // the permission decision and the runtime see the same tool universe.
    let workspace_path: Option<PathBuf> = entry.manifest.workspace.clone();
    let skill_snapshot = {
        let mut snapshot = kernel
            .skill_registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot();
        if let Some(ref workspace) = workspace_path {
            let ws_skills = workspace.join("skills");
            if ws_skills.exists() {
                if let Err(e) = snapshot.load_workspace_skills(&ws_skills) {
                    warn!(
                        agent = %resolved_agent_id_string,
                        error = %e,
                        "bridge IPC: failed to load workspace skills for permission gate"
                    );
                }
            }
        }
        snapshot
    };

    // --- Gate 2: per-agent execute-time permission gate (ANAI C) -----------
    // Belt-and-suspenders with the advertise-time `OPENFANG_BRIDGE_ALLOWED`
    // env var the bridge subprocess was spawned with. Uses the same kernel
    // resolver agent_loop uses to build the env (kernel.rs:2214) against
    // the same registry entry — so the two gates can't drift. Any tool not
    // in this agent's resolved surface (capabilities.tools narrowed by
    // profile, allowlist, blocklist, skills, mcp_servers, mode filter) is
    // rejected here with a logged trace, even if it survived gate 1.
    //
    // Runs *before* the workspace sandbox gate so a denied call never
    // touches the filesystem lookup.
    let permitted: Vec<openfang_types::tool::ToolDefinition> = {
        let resolved =
            kernel.available_tools_with_registry(resolved_agent_id, Some(&skill_snapshot));
        entry.mode.filter_tools(resolved)
    };
    if !permitted.iter().any(|t| t.name == call.tool_name) {
        warn!(
            request_id = call.request_id,
            tool = %call.tool_name,
            agent = %resolved_agent_id_string,
            mode = ?entry.mode,
            permitted_count = permitted.len(),
            "bridge IPC: rejecting tool not in agent's permitted set"
        );
        return CallResult::Error {
            message: format!(
                "tool '{}' not permitted for this agent",
                call.tool_name
            ),
        };
    }

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

    // --- Gate 3: workspace sandbox for filesystem tools (D-fix) ------------
    // Fail-closed for filesystem tools without a workspace. The runtime's
    // `resolve_file_path` falls through to a path-traversal-only check
    // when `workspace_root` is `None`, which is insufficient — absolute
    // paths bypass it and relative paths resolve against the daemon CWD.
    // Refusing the call here keeps the leak closed even if some future
    // call path forgets to pass workspace_root.
    if FS_SANDBOXED_TOOLS.contains(&call.tool_name.as_str()) && workspace_path.is_none() {
        warn!(
            request_id = call.request_id,
            tool = %call.tool_name,
            agent = %resolved_agent_id_string,
            "bridge IPC: refusing filesystem tool — no workspace registered for agent"
        );
        return CallResult::Error {
            message: format!(
                "tool '{}' requires an agent workspace, but no workspace is registered \
                 for agent '{}' — refusing to fall back to an unscoped filesystem view",
                call.tool_name, resolved_agent_id_string
            ),
        };
    }
    let workspace_root_arg: Option<&Path> = workspace_path.as_deref();

    // shell_exec needs both `exec_policy` (allowlist / full / deny decision)
    // and `allowed_env_vars` (hand-granted env passthrough). Resolution is
    // shared with the HTTP `/mcp` path via `AgentExecContext` so the two
    // surfaces apply identical scoping — see S3-01 in the bridge-v2 audit.
    // Every other bridge tool ignores both, so this is cheap to compute
    // unconditionally.
    let exec_ctx = openfang_runtime::agent_tool_context::AgentExecContext::from_manifest(
        &entry.manifest,
    );
    let effective_exec_policy = exec_ctx.exec_policy_ref();
    let allowed_env_arg: Option<&[String]> = exec_ctx.allowed_env();

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
        allowed_env_arg,
        workspace_root_arg, // scoped to the authenticated agent's workspace; gated above
        Some(&kernel.media_engine),
        effective_exec_policy,
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
                tool_name: "definitely_not_a_real_tool".into(), // deliberately not on the list
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
    fn allowlist_contains_web_search() {
        // 13d: native CC `WebSearch` is denied by the 13a deny set; restore
        // it through the bridge so researcher agents (medical, business)
        // keep their primary research surface. Zero new plumbing — kernel
        // `web_ctx` is already passed to `execute_tool` at this call site.
        assert!(
            ALLOWED_TOOLS.contains(&"web_search"),
            "web_search must be on the bridge allowlist post-13a"
        );
    }

    #[test]
    fn allowlist_contains_shell_exec() {
        // 13b: shell_exec reachable through the bridge so CC subprocesses
        // operating under the 13a native-deny set still have a path to the
        // shell (gated, sandboxed, exec_policy-enforced). Locking this in by
        // name so a refactor of `ALLOWED_TOOLS` doesn't silently drop it.
        assert!(
            ALLOWED_TOOLS.contains(&"shell_exec"),
            "shell_exec must be on the bridge allowlist post-13a"
        );
    }

    #[test]
    fn allowlist_contains_apply_patch() {
        // 13e: apply_patch reachable through the bridge as a surgical-edit
        // alternative to whole-file `file_write` rewrites. Mitigates the
        // token + drift cost of the missing CC `Edit` tool while we wait on a
        // native `string_edit` follow-up. Name-locked so a refactor can't
        // silently drop it.
        assert!(
            ALLOWED_TOOLS.contains(&"apply_patch"),
            "apply_patch must be on the bridge allowlist post-13e"
        );
    }

    /// **Drift-catcher: three-way correspondence.**
    ///
    /// Three lists must agree on the bridge tool surface:
    ///
    /// 1. `openfang_api::bridge_ipc::ALLOWED_TOOLS` — daemon-side dispatch
    ///    allowlist (the call-time gate).
    /// 2. `openfang_mcp_bridge::built_in_tools()` — MCP advertise surface
    ///    (what CC actually sees in `tools/list`).
    /// 3. `openfang_mcp_bridge::DEFAULT_ALLOWED` — bridge process default
    ///    when `OPENFANG_BRIDGE_ALLOWED` is unset (legacy/dev path).
    ///
    /// Lesson from 13b/13d: a tool can be daemon-dispatchable (in
    /// `ALLOWED_TOOLS`) but invisible to CC because someone forgot to add it
    /// to `built_in_tools()`. The smoke tests fire the IPC path directly and
    /// never hit `tools/list`, so the gap shipped silently in two commits in
    /// a row. This test fails loudly if any of the three sets drifts.
    ///
    /// If you're here because this test failed: a bridge tool add or
    /// remove must touch **all three files** — `crates/openfang-api/src/
    /// bridge_ipc.rs` (`ALLOWED_TOOLS`), `crates/openfang-mcp-bridge/src/
    /// lib.rs` (`built_in_tools` + `DEFAULT_ALLOWED`). Update both before
    /// landing the commit.
    #[test]
    fn allowlist_three_way_correspondence() {
        use openfang_mcp_bridge::{DEFAULT_ALLOWED, built_in_tools};
        use std::collections::BTreeSet;

        let daemon_set: BTreeSet<&str> = ALLOWED_TOOLS.iter().copied().collect();
        let advertise_set: BTreeSet<String> = built_in_tools()
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        let advertise_borrowed: BTreeSet<&str> =
            advertise_set.iter().map(|s| s.as_str()).collect();
        let default_set: BTreeSet<&str> = DEFAULT_ALLOWED.iter().copied().collect();

        assert_eq!(
            daemon_set, advertise_borrowed,
            "drift: ALLOWED_TOOLS (daemon dispatch) ≠ built_in_tools() (MCP advertise). \
             daemon-only: {:?}, advertise-only: {:?}",
            daemon_set.difference(&advertise_borrowed).collect::<Vec<_>>(),
            advertise_borrowed.difference(&daemon_set).collect::<Vec<_>>(),
        );
        assert_eq!(
            daemon_set, default_set,
            "drift: ALLOWED_TOOLS (daemon dispatch) ≠ DEFAULT_ALLOWED (bridge default). \
             daemon-only: {:?}, default-only: {:?}",
            daemon_set.difference(&default_set).collect::<Vec<_>>(),
            default_set.difference(&daemon_set).collect::<Vec<_>>(),
        );
    }

    /// Pins the tool-surface cardinality at 17. Bumps to this number are
    /// expected when a new bridge tool lands — update intentionally, in
    /// lockstep with the three sets exercised by
    /// [`allowlist_three_way_correspondence`].
    #[test]
    fn allowlist_count_is_seventeen() {
        use openfang_mcp_bridge::{DEFAULT_ALLOWED, built_in_tools};
        assert_eq!(ALLOWED_TOOLS.len(), 17, "ALLOWED_TOOLS surface cardinality");
        assert_eq!(
            built_in_tools().len(),
            17,
            "built_in_tools() advertise surface cardinality"
        );
        assert_eq!(
            DEFAULT_ALLOWED.len(),
            17,
            "DEFAULT_ALLOWED bridge-default cardinality"
        );
    }

    #[test]
    fn apply_patch_is_workspace_sandboxed() {
        // tool_apply_patch resolves every patch-embedded path against
        // workspace_root. Without sandbox membership a no-workspace agent
        // could ship a patch whose Add/Update/Delete targets fall through to
        // the daemon CWD (`~/.openfang`) — sibling-workspace + secrets leak.
        // Fail-closed gate.
        assert!(
            FS_SANDBOXED_TOOLS.contains(&"apply_patch"),
            "apply_patch must require a registered workspace"
        );
    }

    #[test]
    fn shell_exec_is_workspace_sandboxed() {
        // shell_exec uses workspace_root as cwd (tool_runner.rs:1704-1707).
        // Without sandbox membership, a no-workspace agent would shell out
        // in `~/.openfang` and see secrets.env. Fail-closed gate.
        assert!(
            FS_SANDBOXED_TOOLS.contains(&"shell_exec"),
            "shell_exec must require a registered workspace"
        );
    }

    /// Belt-and-braces: every tool the FS sandbox gates must also be on
    /// the daemon-dispatch allowlist. Catches "added an FS tool to
    /// `FS_SANDBOXED_TOOLS` without wiring it onto the bridge surface"
    /// (or the inverse: exposed a new FS tool without sandboxing it).
    #[test]
    fn fs_sandboxed_tools_subset_of_allowed_tools() {
        use std::collections::BTreeSet;
        let allowed: BTreeSet<&str> = ALLOWED_TOOLS.iter().copied().collect();
        let sandboxed: BTreeSet<&str> = FS_SANDBOXED_TOOLS.iter().copied().collect();
        let extras: Vec<&&str> = sandboxed.difference(&allowed).collect();
        assert!(
            extras.is_empty(),
            "FS_SANDBOXED_TOOLS contains tools missing from ALLOWED_TOOLS: {:?}",
            extras
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
