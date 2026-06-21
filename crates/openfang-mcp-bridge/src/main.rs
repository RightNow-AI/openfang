//! Stdio entrypoint for the OpenFang MCP bridge.
//!
//! ## Topology (ANAI-30 step 3)
//!
//! ```text
//! daemon
//!   └── claude (per-prompt subprocess)
//!         └── openfang-mcp-bridge   ← this binary
//!                ├── stdio  ── MCP ──► claude (parent)
//!                └── unix sock ─ IPC ─► daemon (BridgeIpcServer)
//! ```
//!
//! The bridge speaks MCP over stdio to its CC parent, and forwards each
//! `tools/call` over a unix-domain socket to the daemon, which actually
//! invokes `tool_runner::execute_tool`. The daemon socket path and per-spawn
//! auth token come in as env vars set by the daemon at CC-spawn time.
//!
//! ## Env vars
//!
//! | Var                          | Required | Notes                              |
//! |------------------------------|----------|------------------------------------|
//! | `OPENFANG_BRIDGE_SOCKET`     | yes      | absolute path to daemon's unix sock |
//! | `OPENFANG_BRIDGE_TOKEN`      | yes      | per-spawn auth token (any non-empty in ANAI-30) |
//! | `OPENFANG_BRIDGE_AGENT_ID`   | yes      | parent agent id (stub for ANAI-30; ANAI-31 derives from token) |
//! | `OPENFANG_BRIDGE_ALLOWED`    | no       | comma-separated tool allowlist; defaults to the four ANAI-30 tools |
//!
//! ## Concurrency
//!
//! The IPC connection is driven by an actor task (see [`spawn_ipc_actor`])
//! that owns the read+write halves of the socket. `IpcDispatcher::call`
//! sends a `(CallRequest, oneshot::Sender<CallResult>)` over an mpsc channel
//! and awaits the response. Pending requests are correlated by `request_id`.
//! This keeps the wire serial without serializing tool calls at the dispatcher
//! layer — multiple concurrent `tools/call` invocations get distinct ids and
//! are matched up as responses arrive.
//!
//! ## Shutdown
//!
//! When the IPC socket closes (daemon went away, or peer hung up) the actor
//! task exits and the bridge process terminates. CC will be torn down by the
//! daemon shortly after, which also signals our death.

// The MCP bridge IPC is unix-domain-socket-only. On non-unix platforms this
// crate ships as a no-op stub binary (see the `#[cfg(not(unix))] fn main`
// at the bottom of this file). Proper Windows transport (named pipes / TCP
// loopback) is a follow-up.

#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
use anyhow::{anyhow, bail, Context, Result};
#[cfg(unix)]
use openfang_mcp_bridge::{
    protocol::{
        codec, CallRequest, CallResult, Frame, Hello, HelloAck, ListUpstreamRequest,
        UpstreamListResult, UpstreamToolDef, PROTOCOL_VERSION, SOCKET_ENV_VAR, TOKEN_ENV_VAR,
    },
    Bridge, DispatchOk, ToolDispatchError, ToolDispatcher, DEFAULT_ALLOWED,
};
#[cfg(unix)]
use rmcp::{transport::stdio, ServiceExt};
#[cfg(unix)]
use tokio::io::BufReader;
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(unix)]
use tokio::sync::{mpsc, oneshot, Mutex};
#[cfg(unix)]
use tracing_subscriber::EnvFilter;

/// Env var carrying the parent agent id. Stub for ANAI-30; ANAI-31 derives
/// identity from the token so this becomes redundant.
#[cfg(unix)]
const AGENT_ID_ENV_VAR: &str = "OPENFANG_BRIDGE_AGENT_ID";

/// Env var with an optional comma-separated tool allowlist override. Default
/// is the ANAI-30 four-tool slice.
#[cfg(unix)]
const ALLOWED_ENV_VAR: &str = "OPENFANG_BRIDGE_ALLOWED";

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<()> {
    // Tracing → stderr. Stdout is the MCP transport; do not pollute it.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("openfang_mcp_bridge=info,rmcp=warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let socket_path = std::env::var(SOCKET_ENV_VAR)
        .with_context(|| format!("missing required env var {SOCKET_ENV_VAR}"))?;
    let token = std::env::var(TOKEN_ENV_VAR)
        .with_context(|| format!("missing required env var {TOKEN_ENV_VAR}"))?;
    let agent_id = std::env::var(AGENT_ID_ENV_VAR)
        .with_context(|| format!("missing required env var {AGENT_ID_ENV_VAR}"))?;

    let allowed_tools: Vec<String> = match std::env::var(ALLOWED_ENV_VAR) {
        Ok(v) => v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => DEFAULT_ALLOWED.iter().map(|s| (*s).to_string()).collect(),
    };

    tracing::info!(
        socket = %socket_path,
        agent = %agent_id,
        allowed = ?allowed_tools,
        "openfang-mcp-bridge starting"
    );

    // --- Connect + handshake ---
    let mut stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("connect to daemon socket {socket_path}"))?;

    handshake(&mut stream, &token).await?;

    // --- List upstream MCP tools (best-effort) ---
    //
    // Round-trip happens here, with sole ownership of the stream, so
    // it is sequenced before the actor takes the read/write halves.
    // A protocol-level error from the daemon (agent identity not
    // resolvable, registry unavailable) is logged but does NOT abort
    // the bridge: built-in tools still work; the agent simply sees
    // no `mcp_*` surface this session.
    let upstream_tools = list_upstream(&mut stream).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "list_upstream failed; bridge continues without upstream MCP surface");
        Vec::new()
    });
    tracing::info!(
        count = upstream_tools.len(),
        "upstream MCP tools advertised"
    );

    // --- Spawn IPC actor ---
    let dispatcher = spawn_ipc_actor(
        stream,
        agent_id.clone(),
        allowed_tools.clone(),
        upstream_tools,
    );

    // --- Run MCP server over stdio ---
    let service = Bridge::new(Arc::new(dispatcher))
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!(error = ?e, "bridge serve failed"))?;

    service.waiting().await?;
    Ok(())
}

/// Send Hello, await HelloAck. Errors on rejection or wire issues.
#[cfg(unix)]
async fn handshake(stream: &mut UnixStream, token: &str) -> Result<()> {
    let (read_half, mut write_half) = stream.split();
    let mut read_half = BufReader::new(read_half);

    let hello = Frame::Hello(Hello {
        protocol_version: PROTOCOL_VERSION,
        token: token.to_string(),
        bridge_version: env!("CARGO_PKG_VERSION").to_string(),
    });
    codec::write_frame(&mut write_half, &hello)
        .await
        .context("write Hello")?;

    match codec::read_frame(&mut read_half)
        .await
        .context("read HelloAck")?
    {
        Frame::HelloAck(HelloAck::Ok { daemon_version }) => {
            tracing::info!(daemon_version, "bridge IPC handshake ok");
            Ok(())
        }
        Frame::HelloAck(HelloAck::Rejected { reason }) => {
            bail!("daemon rejected handshake: {reason}")
        }
        other => bail!("expected HelloAck, got {other:?}"),
    }
}

/// Maximum time to wait for the per-agent upstream MCP tool list from
/// the daemon. Bounds bridge startup so a wedged daemon or a slow
/// upstream MCP server (`mcp_connections` lock held during a slow
/// `tools/list`) cannot stall every new bridge session indefinitely.
/// On timeout the caller downgrades to "no upstream surface this
/// session" — same failure mode as a protocol-level refusal.
#[cfg(unix)]
const LIST_UPSTREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Bridge → daemon: ask for the per-agent upstream MCP tool list.
///
/// Sent once, immediately after a successful handshake while the
/// caller still owns the stream end-to-end. Returns the advertised
/// upstream tools (possibly empty) on success, or an error that the
/// caller may downgrade to "no upstream surface this session".
///
/// Bounded by [`LIST_UPSTREAM_TIMEOUT`].
#[cfg(unix)]
async fn list_upstream(stream: &mut UnixStream) -> Result<Vec<UpstreamToolDef>> {
    tokio::time::timeout(LIST_UPSTREAM_TIMEOUT, list_upstream_inner(stream))
        .await
        .map_err(|_| anyhow!("list_upstream timed out after {:?}", LIST_UPSTREAM_TIMEOUT))?
}

#[cfg(unix)]
async fn list_upstream_inner(stream: &mut UnixStream) -> Result<Vec<UpstreamToolDef>> {
    let (read_half, mut write_half) = stream.split();
    let mut read_half = BufReader::new(read_half);

    let req = Frame::ListUpstream(ListUpstreamRequest { request_id: 0 });
    codec::write_frame(&mut write_half, &req)
        .await
        .context("write ListUpstream")?;

    match codec::read_frame(&mut read_half)
        .await
        .context("read UpstreamList")?
    {
        Frame::UpstreamList(resp) => match resp.result {
            UpstreamListResult::Ok { tools } => Ok(tools),
            UpstreamListResult::Error { message } => {
                Err(anyhow!("daemon refused list_upstream: {message}"))
            }
        },
        other => Err(anyhow!("expected UpstreamList, got {other:?}")),
    }
}

/// One pending request: the slot the actor will fill when its response frame
/// arrives over the wire.
#[cfg(unix)]
type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<CallResult>>>>;

/// Message dispatcher → actor: a tool call to put on the wire, plus a
/// oneshot to fill with the response.
#[cfg(unix)]
struct IpcRequest {
    call: CallRequest,
    reply: oneshot::Sender<CallResult>,
}

/// Bridge-side `ToolDispatcher` impl. Forwards each call to the actor task
/// over an mpsc and awaits the correlated response.
#[cfg(unix)]
pub struct IpcDispatcher {
    agent_id: String,
    allowed: Vec<String>,
    /// Cached snapshot of upstream MCP tools advertised by the daemon
    /// at session start (one-shot `ListUpstream` round-trip). Refreshes
    /// require restarting the bridge — by design for now, since the
    /// daemon also restarts agents on `agent.toml mcp_servers` changes.
    upstream: Vec<UpstreamToolDef>,
    tx: mpsc::Sender<IpcRequest>,
    next_id: AtomicU64,
}

#[cfg(unix)]
#[async_trait::async_trait]
impl ToolDispatcher for IpcDispatcher {
    fn agent_id(&self) -> &str {
        &self.agent_id
    }

    fn allowed_tools(&self) -> Vec<String> {
        self.allowed.clone()
    }

    fn upstream_tools(&self) -> Vec<UpstreamToolDef> {
        self.upstream.clone()
    }

    async fn call(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<DispatchOk, ToolDispatchError> {
        // Two gates here:
        // - Built-in tools must be in `OPENFANG_BRIDGE_ALLOWED` /
        //   `DEFAULT_ALLOWED`.
        // - `mcp_*` upstream tools must have been advertised by the
        //   daemon at list-upstream time. Server-side `mcp_servers`
        //   gating in the daemon is the source of truth; this is
        //   just an early-exit hygiene check so we never ship a
        //   bogus `mcp_*` name across the wire.
        let is_builtin_allowed = self.allowed.iter().any(|a| a == tool_name);
        let is_advertised_upstream =
            tool_name.starts_with("mcp_") && self.upstream.iter().any(|t| t.name == tool_name);
        if !is_builtin_allowed && !is_advertised_upstream {
            return Err(ToolDispatchError::NotPermitted(tool_name.to_string()));
        }

        let request_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();

        let req = IpcRequest {
            call: CallRequest {
                request_id,
                agent_id: self.agent_id.clone(),
                tool_name: tool_name.to_string(),
                args,
            },
            reply: reply_tx,
        };

        self.tx
            .send(req)
            .await
            .map_err(|_| ToolDispatchError::Execution(anyhow!("bridge IPC actor has shut down")))?;

        let result = reply_rx
            .await
            .map_err(|_| ToolDispatchError::Execution(anyhow!("IPC response dropped")))?;

        match result {
            CallResult::Ok { content, is_error } => Ok(DispatchOk { content, is_error }),
            CallResult::Error { message } => Err(ToolDispatchError::Execution(anyhow!(message))),
        }
    }
}

/// Spawn the IPC actor task that owns the connected stream and pumps
/// requests/responses.
///
/// Reads and writes live in two sibling tasks sharing a [`PendingMap`]:
/// - **writer task** drains the mpsc, writes each [`CallRequest`] frame
/// - **reader task** reads frames forever, looks up the matching pending
///   oneshot by `request_id`, and fulfills it
///
/// Either side exiting causes the other to wind down — the channel closes
/// on drop and the stream closes on EOF.
#[cfg(unix)]
pub fn spawn_ipc_actor(
    stream: UnixStream,
    agent_id: String,
    allowed: Vec<String>,
    upstream: Vec<UpstreamToolDef>,
) -> IpcDispatcher {
    let (tx, mut rx) = mpsc::channel::<IpcRequest>(32);
    let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

    let (read_half, write_half) = stream.into_split();
    let mut read_half = BufReader::new(read_half);
    let write_half = Arc::new(Mutex::new(write_half));

    // Writer task — drains the mpsc, registers pending oneshots, writes frames.
    {
        let pending = pending.clone();
        let write_half = write_half.clone();
        tokio::spawn(async move {
            while let Some(IpcRequest { call, reply }) = rx.recv().await {
                {
                    let mut p = pending.lock().await;
                    p.insert(call.request_id, reply);
                }
                let frame = Frame::Call(call);
                let mut w = write_half.lock().await;
                if let Err(e) = codec::write_frame(&mut *w, &frame).await {
                    tracing::error!(error = %e, "bridge IPC: write_frame failed; shutting down writer");
                    break;
                }
            }
            tracing::debug!("bridge IPC writer task exiting");
        });
    }

    // Reader task — reads response frames, dispatches to pending oneshots.
    {
        let pending = pending.clone();
        tokio::spawn(async move {
            loop {
                let frame = match codec::read_frame(&mut read_half).await {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        tracing::info!("bridge IPC: daemon closed connection");
                        break;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "bridge IPC read_frame failed");
                        break;
                    }
                };
                match frame {
                    Frame::Response(resp) => {
                        let slot = {
                            let mut p = pending.lock().await;
                            p.remove(&resp.request_id)
                        };
                        if let Some(tx) = slot {
                            let _ = tx.send(resp.result);
                        } else {
                            tracing::warn!(
                                request_id = resp.request_id,
                                "bridge IPC: response for unknown request_id (dropped)"
                            );
                        }
                    }
                    other => {
                        tracing::warn!(?other, "bridge IPC: unexpected non-Response frame");
                    }
                }
            }
            // Drain pending: best-effort, so dispatcher.call doesn't hang
            // forever when the daemon goes away mid-flight.
            let mut p = pending.lock().await;
            for (_, tx) in p.drain() {
                let _ = tx.send(CallResult::Error {
                    message: "bridge IPC connection closed before response".into(),
                });
            }
            tracing::debug!("bridge IPC reader task exiting");
            // Production: force the process down — without an
            // MCP-transport-aware way to signal the rmcp service to stop,
            // exiting here is the simplest correct behavior; the parent CC
            // will be torn down by the daemon shortly anyway. Skipped under
            // `cfg(test)` so unit tests don't tear down the test runner.
            #[cfg(not(test))]
            std::process::exit(0);
        });
    }

    IpcDispatcher {
        agent_id,
        allowed,
        upstream,
        tx,
        next_id: AtomicU64::new(1),
    }
}

/// Stub entrypoint for non-unix platforms. The bridge requires unix-domain
/// sockets to talk to the daemon; on Windows it ships as this no-op binary
/// so the workspace builds cleanly and operators get a clear runtime error
/// rather than a compile failure.
#[cfg(not(unix))]
fn main() {
    eprintln!(
        "openfang-mcp-bridge requires unix-domain sockets and is not supported \
         on this platform. Daemon will run without bridge IPC; CC subprocesses \
         spawn without --mcp-config. Track the upstream follow-up issue for \
         Windows transport (named pipes / TCP loopback)."
    );
    std::process::exit(1);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use openfang_mcp_bridge::protocol::{CallResponse, Hello, HelloAck};
    use tokio::net::UnixListener;

    /// End-to-end: spin up a fake daemon listener, run handshake +
    /// spawn_ipc_actor, send two concurrent calls, verify each gets the
    /// right correlated response.
    #[tokio::test]
    async fn ipc_dispatcher_round_trip_and_correlation() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("bridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        // Fake daemon: accept, handshake-ok, then echo each call as Ok with
        // content = tool_name (so the test can verify per-id correlation).
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (rh, mut wh) = stream.split();
            let mut rh = BufReader::new(rh);
            // Handshake.
            match codec::read_frame(&mut rh).await.unwrap() {
                Frame::Hello(_) => {}
                _ => panic!("expected Hello"),
            }
            codec::write_frame(
                &mut wh,
                &Frame::HelloAck(HelloAck::Ok {
                    daemon_version: "test".into(),
                }),
            )
            .await
            .unwrap();
            // Read N calls and reply.
            for _ in 0..2 {
                let frame = codec::read_frame(&mut rh).await.unwrap();
                let call = match frame {
                    Frame::Call(c) => c,
                    _ => panic!("expected Call"),
                };
                codec::write_frame(
                    &mut wh,
                    &Frame::Response(CallResponse {
                        request_id: call.request_id,
                        result: CallResult::Ok {
                            content: call.tool_name,
                            is_error: false,
                        },
                    }),
                )
                .await
                .unwrap();
            }
        });

        let mut client = UnixStream::connect(&sock).await.unwrap();

        // Inline handshake (matches `handshake()` in main, factored for test).
        {
            let (rh, mut wh) = client.split();
            let mut rh = BufReader::new(rh);
            codec::write_frame(
                &mut wh,
                &Frame::Hello(Hello {
                    protocol_version: PROTOCOL_VERSION,
                    token: "t".into(),
                    bridge_version: "test".into(),
                }),
            )
            .await
            .unwrap();
            match codec::read_frame(&mut rh).await.unwrap() {
                Frame::HelloAck(HelloAck::Ok { .. }) => {}
                other => panic!("bad ack: {other:?}"),
            }
        }

        let dispatcher = spawn_ipc_actor(
            client,
            "agent-x".into(),
            vec!["file_read".into(), "file_list".into()],
            Vec::new(),
        );

        // Concurrent calls — exercise the correlation map.
        let (a, b) = tokio::join!(
            dispatcher.call("file_read", serde_json::json!({"path": "a"})),
            dispatcher.call("file_list", serde_json::json!({"path": "b"})),
        );
        let a = a.expect("file_read dispatch");
        let b = b.expect("file_list dispatch");
        assert_eq!(a.content, "file_read");
        assert_eq!(b.content, "file_list");
        assert!(!a.is_error && !b.is_error);

        // Permission gate.
        let denied = dispatcher
            .call("shell_exec", serde_json::json!({}))
            .await
            .expect_err("disallowed tool must error");
        match denied {
            ToolDispatchError::NotPermitted(t) => assert_eq!(t, "shell_exec"),
            other => panic!("expected NotPermitted, got {other:?}"),
        }

        let _ = server.await;
    }

    /// End-to-end list_upstream + mcp_* dispatch:
    /// - fake daemon answers Hello/HelloAck,
    /// - then responds to ListUpstream with a one-tool catalog,
    /// - bridge dispatcher gets that cache,
    /// - a `mcp_linear_getteams` call passes the bridge-side gate
    ///   (NOT in `allowed`) and round-trips back with the tool name.
    /// - a `mcp_unknown_tool` call (not in upstream cache) is
    ///   rejected client-side with NotPermitted.
    #[tokio::test]
    async fn list_upstream_handshake_and_mcp_dispatch() {
        use openfang_mcp_bridge::protocol::{
            CallResponse, UpstreamListResponse, UpstreamListResult,
        };

        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("bridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (rh, mut wh) = stream.split();
            let mut rh = BufReader::new(rh);
            // Hello.
            match codec::read_frame(&mut rh).await.unwrap() {
                Frame::Hello(_) => {}
                _ => panic!("expected Hello"),
            }
            codec::write_frame(
                &mut wh,
                &Frame::HelloAck(HelloAck::Ok {
                    daemon_version: "test".into(),
                }),
            )
            .await
            .unwrap();
            // ListUpstream.
            let req = match codec::read_frame(&mut rh).await.unwrap() {
                Frame::ListUpstream(r) => r,
                other => panic!("expected ListUpstream, got {other:?}"),
            };
            codec::write_frame(
                &mut wh,
                &Frame::UpstreamList(UpstreamListResponse {
                    request_id: req.request_id,
                    result: UpstreamListResult::Ok {
                        tools: vec![UpstreamToolDef {
                            name: "mcp_linear_getteams".into(),
                            server: "linear".into(),
                            description: Some("List Linear teams".into()),
                            input_schema: serde_json::json!({"type":"object"}),
                        }],
                    },
                }),
            )
            .await
            .unwrap();
            // One mcp_* Call → echo tool_name back as Ok content.
            let call = match codec::read_frame(&mut rh).await.unwrap() {
                Frame::Call(c) => c,
                other => panic!("expected Call, got {other:?}"),
            };
            codec::write_frame(
                &mut wh,
                &Frame::Response(CallResponse {
                    request_id: call.request_id,
                    result: CallResult::Ok {
                        content: call.tool_name,
                        is_error: false,
                    },
                }),
            )
            .await
            .unwrap();
        });

        let mut client = UnixStream::connect(&sock).await.unwrap();

        // Inline handshake.
        {
            let (rh, mut wh) = client.split();
            let mut rh = BufReader::new(rh);
            codec::write_frame(
                &mut wh,
                &Frame::Hello(Hello {
                    protocol_version: PROTOCOL_VERSION,
                    token: "t".into(),
                    bridge_version: "test".into(),
                }),
            )
            .await
            .unwrap();
            match codec::read_frame(&mut rh).await.unwrap() {
                Frame::HelloAck(HelloAck::Ok { .. }) => {}
                other => panic!("bad ack: {other:?}"),
            }
        }

        // list_upstream round-trip via the production helper.
        let upstream = list_upstream(&mut client).await.expect("list_upstream ok");
        assert_eq!(upstream.len(), 1);
        assert_eq!(upstream[0].name, "mcp_linear_getteams");

        let dispatcher = spawn_ipc_actor(
            client,
            "agent-x".into(),
            // mcp_* is NOT in the built-in allowlist — only the
            // upstream cache should grant it.
            vec!["file_read".into()],
            upstream,
        );

        // Allowed via upstream cache.
        let ok = dispatcher
            .call("mcp_linear_getteams", serde_json::json!({}))
            .await
            .expect("mcp dispatch ok");
        assert_eq!(ok.content, "mcp_linear_getteams");
        assert!(!ok.is_error);

        // Not in upstream cache, not in allowed — must be denied locally.
        let denied = dispatcher
            .call("mcp_linear_notallowed", serde_json::json!({}))
            .await
            .expect_err("unadvertised mcp_* tool must be denied");
        match denied {
            ToolDispatchError::NotPermitted(t) => assert_eq!(t, "mcp_linear_notallowed"),
            other => panic!("expected NotPermitted, got {other:?}"),
        }

        let _ = server.await;
    }

    /// list_upstream() surfaces protocol-layer errors as Err — the
    /// production caller in main() downgrades these to "no upstream
    /// surface this session" via unwrap_or_else.
    #[tokio::test]
    async fn list_upstream_propagates_error_result() {
        use openfang_mcp_bridge::protocol::{UpstreamListResponse, UpstreamListResult};

        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("bridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (rh, mut wh) = stream.split();
            let mut rh = BufReader::new(rh);
            // Skip Hello (the test doesn't perform it; client below
            // sends ListUpstream directly).
            let req = match codec::read_frame(&mut rh).await.unwrap() {
                Frame::ListUpstream(r) => r,
                other => panic!("expected ListUpstream, got {other:?}"),
            };
            codec::write_frame(
                &mut wh,
                &Frame::UpstreamList(UpstreamListResponse {
                    request_id: req.request_id,
                    result: UpstreamListResult::Error {
                        message: "agent identity not resolvable".into(),
                    },
                }),
            )
            .await
            .unwrap();
        });

        let mut client = UnixStream::connect(&sock).await.unwrap();
        let err = list_upstream(&mut client)
            .await
            .expect_err("Error result must propagate");
        assert!(err.to_string().contains("not resolvable"));
        let _ = server.await;
    }
}
