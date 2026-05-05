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
//! ## Status — ANAI-30 step 1
//!
//! This module currently:
//! - Listens on `<home_dir>/run/bridge.sock`.
//! - Accepts the protocol [`Hello`](openfang_mcp_bridge::protocol::Hello)
//!   handshake (any non-empty token; real auth in ANAI-31).
//! - Decodes [`CallRequest`](openfang_mcp_bridge::protocol::CallRequest)
//!   frames and returns a stub `Error { message: "dispatch not yet wired" }`
//!   response. **Step 2** of the ANAI-30 plan replaces this with a real
//!   call into `tool_runner::execute_tool`, scoped to the four tools
//!   `file_read`, `file_list`, `agent_list`, `channel_send`.
//!
//! Keeping step 1 a clean stub makes the wire shape independently testable
//! before we tangle in execute_tool's 17-argument context bundle.

use openfang_kernel::OpenFangKernel;
use openfang_mcp_bridge::protocol::{
    CallResponse, CallResult, Frame, Hello, HelloAck, PROTOCOL_VERSION, SOCKET_RELATIVE_PATH,
    codec,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

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
    _kernel: Arc<OpenFangKernel>,
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
    debug!(
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

        debug!(
            request_id = call.request_id,
            tool = %call.tool_name,
            agent = %call.agent_id,
            "bridge IPC: received call (step-1 stub will not dispatch)"
        );

        // ANAI-30 step 1: handler is stubbed. Step 2 wires this to
        // openfang_runtime::tool_runner::execute_tool with the four-tool
        // allowlist (file_read, file_list, agent_list, channel_send).
        let response = Frame::Response(CallResponse {
            request_id: call.request_id,
            result: CallResult::Error {
                message: format!(
                    "tool dispatch not yet wired in daemon (ANAI-30 step 1 stub); requested tool='{}'",
                    call.tool_name
                ),
            },
        });
        codec::write_frame(&mut write_half, &response).await?;
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

    /// End-to-end round trip test: bind a listener at a tempfile path,
    /// connect, do the handshake, send a CallRequest, expect the step-1
    /// stub error response.
    ///
    /// We don't go through `BridgeIpcServer::start` here because that needs
    /// a full `OpenFangKernel`. Instead we exercise `handle_connection`
    /// directly by spawning the accept loop manually.
    #[tokio::test]
    async fn ipc_handshake_and_stub_dispatch() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("bridge.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // We can't synthesize a real OpenFangKernel here; the connection
            // handler doesn't actually dereference it in step 1 (kernel use
            // lands in step 2 alongside execute_tool wiring). Build a minimal
            // proxy by inlining the relevant logic.
            handle_connection_no_kernel(stream).await.unwrap();
        });

        let mut client = ClientStream::connect(&sock).await.unwrap();
        let (cr, mut cw) = client.split();
        let mut cr = BufReader::new(cr);

        // Send Hello
        let hello = Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            token: "stub-token".into(),
            bridge_version: "test".into(),
        });
        codec::write_frame(&mut cw, &hello).await.unwrap();

        // Receive HelloAck
        match codec::read_frame(&mut cr).await.unwrap() {
            Frame::HelloAck(HelloAck::Ok { .. }) => {}
            other => panic!("expected HelloAck::Ok, got {other:?}"),
        }

        // Send a Call
        let call = Frame::Call(CallRequest {
            request_id: 1,
            agent_id: "test-agent".into(),
            tool_name: "file_read".into(),
            args: serde_json::json!({"path": "x"}),
        });
        codec::write_frame(&mut cw, &call).await.unwrap();

        // Receive stub Response
        match codec::read_frame(&mut cr).await.unwrap() {
            Frame::Response(CallResponse {
                request_id: 1,
                result: CallResult::Error { message },
            }) => {
                assert!(message.contains("not yet wired"));
            }
            other => panic!("unexpected response: {other:?}"),
        }

        drop(client);
        server.await.unwrap();
    }

    /// Test-only twin of [`handle_connection`] that doesn't take a kernel.
    /// Kept in lockstep with the real handler's wire behavior; if the
    /// production handler diverges, update this too.
    async fn handle_connection_no_kernel(mut stream: UnixStream) -> std::io::Result<()> {
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
            let response = Frame::Response(CallResponse {
                request_id: call.request_id,
                result: CallResult::Error {
                    message: format!(
                        "tool dispatch not yet wired in daemon (ANAI-30 step 1 stub); requested tool='{}'",
                        call.tool_name
                    ),
                },
            });
            codec::write_frame(&mut write_half, &response).await?;
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
