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
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use anyhow::{Context, Result, anyhow, bail};
#[cfg(unix)]
use openfang_mcp_bridge::{
    Bridge, DispatchOk, ToolDispatchError, ToolDispatcher,
    protocol::{
        CallRequest, CallResult, Frame, Hello, HelloAck, PROTOCOL_VERSION, SOCKET_ENV_VAR,
        TOKEN_ENV_VAR, codec,
    },
};
#[cfg(unix)]
use rmcp::{ServiceExt, transport::stdio};
use tokio::io::BufReader;
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(unix)]
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing_subscriber::EnvFilter;

/// Env var carrying the parent agent id. Stub for ANAI-30; ANAI-31 derives
/// identity from the token so this becomes redundant.
#[cfg(unix)]
const AGENT_ID_ENV_VAR: &str = "OPENFANG_BRIDGE_AGENT_ID";

/// Env var with an optional comma-separated tool allowlist override. Default
/// is the ANAI-30 four-tool slice.
#[cfg(unix)]
const ALLOWED_ENV_VAR: &str = "OPENFANG_BRIDGE_ALLOWED";

/// Default tool allowlist when [`ALLOWED_ENV_VAR`] is unset. Mirrors the
/// daemon's `bridge_ipc::ALLOWED_TOOLS`. Tracks the bridge's `built_in_tools`
/// surface so a bridge spawned without per-agent gating (legacy/dev path)
/// still advertises everything it's capable of dispatching.
#[cfg(unix)]
const DEFAULT_ALLOWED: &[&str] = &[
    "file_read",
    "file_list",
    "agent_list",
    "channel_send",
    "agent_send",
];

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

    // --- Spawn IPC actor ---
    let dispatcher = spawn_ipc_actor(stream, agent_id.clone(), allowed_tools.clone());

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

    match codec::read_frame(&mut read_half).await.context("read HelloAck")? {
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

    async fn call(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<DispatchOk, ToolDispatchError> {
        if !self.allowed.iter().any(|a| a == tool_name) {
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

        self.tx.send(req).await.map_err(|_| {
            ToolDispatchError::Execution(anyhow!("bridge IPC actor has shut down"))
        })?;

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
}
