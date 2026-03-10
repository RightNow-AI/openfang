//! PeerNode — TCP server and client for the OpenFang Wire Protocol.
//!
//! A [`PeerNode`] binds a local TCP listener and accepts incoming connections
//! from other OpenFang kernels. It also connects outward to known peers. Each
//! connection performs a handshake to exchange identity and agent lists, then
//! enters a message dispatch loop.
//!
//! The [`PeerHandle`] trait abstracts the kernel's ability to respond to
//! remote requests (agent messages, discovery, etc.).

use crate::message::*;
use crate::registry::{PeerEntry, PeerRegistry, PeerState};

use async_trait::async_trait;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

type HmacSha256 = Hmac<Sha256>;

/// SECURITY: Time-windowed nonce tracker to prevent OFP handshake replay attacks.
///
/// Stores seen nonces with their timestamps. Nonces older than the window
/// are garbage-collected on insertion. A 5-minute window is used because
/// handshake nonces are single-use UUIDs.
#[derive(Clone)]
pub struct NonceTracker {
    seen: Arc<DashMap<String, Instant>>,
    window: Duration,
}

impl NonceTracker {
    /// Create a new nonce tracker with a 5-minute replay window.
    pub fn new() -> Self {
        Self {
            seen: Arc::new(DashMap::new()),
            window: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Check if a nonce has been seen before. If not, record it and return Ok.
    /// If already seen (replay), return Err.
    pub fn check_and_record(&self, nonce: &str) -> Result<(), String> {
        let now = Instant::now();

        // Garbage-collect expired nonces (older than window)
        self.seen.retain(|_, ts| now.duration_since(*ts) < self.window);

        // Check for replay
        if self.seen.contains_key(nonce) {
            return Err(format!("Nonce replay detected: {}", openfang_types::truncate_str(nonce, 16)));
        }

        // Record the nonce
        self.seen.insert(nonce.to_string(), now);
        Ok(())
    }
}

impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate HMAC-SHA256 signature for message authentication.
fn hmac_sign(secret: &str, data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

/// Verify HMAC-SHA256 signature using constant-time comparison.
fn hmac_verify(secret: &str, data: &[u8], signature: &str) -> bool {
    let expected = hmac_sign(secret, data);
    subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), signature.as_bytes()).into()
}

/// Errors from the wire protocol layer.
#[derive(Debug, Error)]
pub enum WireError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: u32, max: u32 },
    #[error("Protocol version mismatch: local={local}, remote={remote}")]
    VersionMismatch { local: u32, remote: u32 },
}

/// Maximum single message size (16 MB).
pub const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Configuration for a PeerNode.
#[derive(Debug, Clone)]
pub struct PeerConfig {
    /// Address to bind the listener on.
    pub listen_addr: SocketAddr,
    /// This node's unique ID.
    pub node_id: String,
    /// This node's human-readable name.
    pub node_name: String,
    /// Pre-shared key for HMAC-SHA256 authentication.
    /// Required — OFP refuses to start without it.
    pub shared_secret: String,
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: uuid::Uuid::new_v4().to_string(),
            node_name: "openfang-node".to_string(),
            shared_secret: String::new(),
        }
    }
}

/// Trait for the kernel to handle incoming remote requests.
///
/// The PeerNode calls these methods when it receives requests from
/// remote peers. The kernel implements this to route messages to
/// local agents.
#[async_trait]
pub trait PeerHandle: Send + Sync + 'static {
    /// List local agents as RemoteAgentInfo (for handshake and discovery).
    fn local_agents(&self) -> Vec<RemoteAgentInfo>;

    /// Send a message to a local agent and get the response.
    async fn handle_agent_message(
        &self,
        agent: &str,
        message: &str,
        sender: Option<&str>,
    ) -> Result<String, String>;

    /// Find local agents matching a query.
    fn discover_agents(&self, query: &str) -> Vec<RemoteAgentInfo>;

    /// Return the uptime of the local node in seconds.
    fn uptime_secs(&self) -> u64;
}

/// The local network node — listens for connections and connects to peers.
pub struct PeerNode {
    config: PeerConfig,
    registry: PeerRegistry,
    /// Actual bound address (useful when binding to port 0).
    local_addr: SocketAddr,
    /// Start time for uptime calculation (used by handle_request for Pong).
    #[allow(dead_code)]
    start_time: Instant,
    /// SECURITY: Tracks seen handshake nonces to prevent replay attacks.
    nonce_tracker: NonceTracker,
    /// SECURITY: Session key derived after handshake for per-message HMAC.
    #[allow(dead_code)]
    session_key: std::sync::Mutex<Option<String>>,
}

impl PeerNode {
    /// Create and start listening on the configured address.
    pub async fn start(
        config: PeerConfig,
        registry: PeerRegistry,
        handle: Arc<dyn PeerHandle>,
    ) -> Result<(Arc<Self>, tokio::task::JoinHandle<()>), WireError> {
        // SECURITY: Require shared_secret for OFP
        if config.shared_secret.is_empty() {
            return Err(WireError::HandshakeFailed(
                "OFP requires shared_secret. Set [network] shared_secret in config.toml".into(),
            ));
        }

        let listener = TcpListener::bind(config.listen_addr).await?;
        let local_addr = listener.local_addr()?;

        info!(
            "OFP: listening on {} (node_id={})",
            local_addr, config.node_id
        );

        let node = Arc::new(Self {
            config,
            registry: registry.clone(),
            local_addr,
            start_time: Instant::now(),
            nonce_tracker: NonceTracker::new(),
            session_key: std::sync::Mutex::new(None),
        });

        let node_clone = Arc::clone(&node);
        let accept_handle = tokio::spawn(async move {
            Self::accept_loop(listener, node_clone, registry, handle).await;
        });

        Ok((node, accept_handle))
    }

    /// Get the actual bound address.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Get the node ID.
    pub fn node_id(&self) -> &str {
        &self.config.node_id
    }

    /// Get a reference to the peer registry.
    pub fn registry(&self) -> &PeerRegistry {
        &self.registry
    }

    /// Connect to a remote peer and perform the handshake.
    pub async fn connect_to_peer(
        &self,
        addr: SocketAddr,
        handle: Arc<dyn PeerHandle>,
    ) -> Result<(), WireError> {
        info!("OFP: connecting to peer at {}", addr);
        let stream = TcpStream::connect(addr).await?;
        let (mut reader, mut writer) = stream.into_split();

        // Send our handshake with HMAC authentication
        let our_nonce = uuid::Uuid::new_v4().to_string();
        let auth_data = format!("{}{}", our_nonce, self.config.node_id);
        let auth_hmac = hmac_sign(&self.config.shared_secret, auth_data.as_bytes());

        let handshake = WireMessage {
            id: uuid::Uuid::new_v4().to_string(),
            kind: WireMessageKind::Request(WireRequest::Handshake {
                node_id: self.config.node_id.clone(),
                node_name: self.config.node_name.clone(),
                protocol_version: PROTOCOL_VERSION,
                agents: handle.local_agents(),
                nonce: our_nonce.clone(),
                auth_hmac,
            }),
        };
        write_message(&mut writer, &handshake).await?;

        // Read their handshake ack
        let response = read_message(&mut reader).await?;
        let sess_key = match &response.kind {
            WireMessageKind::Response(WireResponse::HandshakeAck {
                node_id,
                node_name,
                protocol_version,
                agents,
                nonce: ack_nonce,
                auth_hmac: ack_hmac,
            }) => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(WireError::VersionMismatch {
                        local: PROTOCOL_VERSION,
                        remote: *protocol_version,
                    });
                }

                // SECURITY: Check for nonce replay on the ack
                if let Err(replay_err) = self.nonce_tracker.check_and_record(ack_nonce) {
                    return Err(WireError::HandshakeFailed(replay_err));
                }

                // SECURITY: Verify the ack HMAC
                let expected_data = format!("{}{}", ack_nonce, node_id);
                if !hmac_verify(
                    &self.config.shared_secret,
                    expected_data.as_bytes(),
                    ack_hmac,
                ) {
                    return Err(WireError::HandshakeFailed(
                        "HMAC verification failed on HandshakeAck".into(),
                    ));
                }

                // SECURITY: Derive per-session key for authenticated messages
                let key = derive_session_key(
                    &self.config.shared_secret,
                    &our_nonce,
                    ack_nonce,
                );

                info!(
                    "OFP: handshake complete with {} ({}) — {} agents",
                    node_name,
                    node_id,
                    agents.len()
                );
                self.registry.add_peer(PeerEntry {
                    node_id: node_id.clone(),
                    node_name: node_name.clone(),
                    address: addr,
                    agents: agents.clone(),
                    state: PeerState::Connected,
                    connected_at: chrono::Utc::now(),
                    protocol_version: *protocol_version,
                });
                key
            }
            WireMessageKind::Response(WireResponse::Error { code, message }) => {
                return Err(WireError::HandshakeFailed(format!(
                    "Remote error {code}: {message}"
                )));
            }
            _ => {
                return Err(WireError::HandshakeFailed(
                    "Unexpected response to handshake".to_string(),
                ));
            }
        };

        // Extract the peer node_id for the connection loop
        let peer_node_id = match &response.kind {
            WireMessageKind::Response(WireResponse::HandshakeAck { node_id, .. }) => {
                node_id.clone()
            }
            _ => unreachable!(),
        };

        // Spawn a task to handle ongoing communication with per-message HMAC
        let registry = self.registry.clone();
        tokio::spawn(async move {
            if let Err(e) =
                connection_loop(&mut reader, &mut writer, &peer_node_id, &registry, &*handle, Some(&sess_key)).await
            {
                debug!("OFP: connection to {} ended: {}", peer_node_id, e);
            }
            registry.mark_disconnected(&peer_node_id);
        });

        Ok(())
    }

    /// Send a message to a specific peer and await the response.
    ///
    /// SECURITY: Opens a new connection to the peer, performs a full HMAC
    /// handshake, sends the agent message, and reads the response.
    pub async fn send_to_peer(
        &self,
        node_id: &str,
        agent: &str,
        message: &str,
        sender: Option<&str>,
        handle: Arc<dyn PeerHandle>,
    ) -> Result<String, WireError> {
        let peer = self
            .registry
            .get_peer(node_id)
            .ok_or_else(|| WireError::HandshakeFailed(format!("Unknown peer: {node_id}")))?;

        let stream = TcpStream::connect(peer.address).await?;
        let (mut reader, mut writer) = stream.into_split();

        // SECURITY: Perform HMAC handshake before sending any data
        let our_nonce = uuid::Uuid::new_v4().to_string();
        let auth_data = format!("{}{}", our_nonce, self.config.node_id);
        let auth_hmac = hmac_sign(&self.config.shared_secret, auth_data.as_bytes());

        let handshake = WireMessage {
            id: uuid::Uuid::new_v4().to_string(),
            kind: WireMessageKind::Request(WireRequest::Handshake {
                node_id: self.config.node_id.clone(),
                node_name: self.config.node_name.clone(),
                protocol_version: PROTOCOL_VERSION,
                agents: handle.local_agents(),
                nonce: our_nonce.clone(),
                auth_hmac,
            }),
        };
        write_message(&mut writer, &handshake).await?;

        // Verify handshake ack and derive session key
        let ack = read_message(&mut reader).await?;
        let session_key = match &ack.kind {
            WireMessageKind::Response(WireResponse::HandshakeAck {
                node_id: ack_node_id,
                nonce: ack_nonce,
                auth_hmac: ack_hmac,
                protocol_version,
                ..
            }) => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(WireError::VersionMismatch {
                        local: PROTOCOL_VERSION,
                        remote: *protocol_version,
                    });
                }
                // SECURITY: Check for nonce replay
                if let Err(replay_err) = self.nonce_tracker.check_and_record(ack_nonce) {
                    return Err(WireError::HandshakeFailed(replay_err));
                }
                let expected_data = format!("{}{}", ack_nonce, ack_node_id);
                if !hmac_verify(
                    &self.config.shared_secret,
                    expected_data.as_bytes(),
                    ack_hmac,
                ) {
                    return Err(WireError::HandshakeFailed(
                        "HMAC verification failed on HandshakeAck".into(),
                    ));
                }
                // SECURITY: Derive per-session key for authenticated post-handshake I/O
                derive_session_key(&self.config.shared_secret, &our_nonce, ack_nonce)
            }
            WireMessageKind::Response(WireResponse::Error { code, message }) => {
                return Err(WireError::HandshakeFailed(format!(
                    "Remote error {code}: {message}"
                )));
            }
            _ => {
                return Err(WireError::HandshakeFailed(
                    "Unexpected response to handshake".to_string(),
                ));
            }
        };

        // SECURITY: Send agent message with per-message HMAC authentication
        let msg = WireMessage {
            id: uuid::Uuid::new_v4().to_string(),
            kind: WireMessageKind::Request(WireRequest::AgentMessage {
                agent: agent.to_string(),
                message: message.to_string(),
                sender: sender.map(|s| s.to_string()),
            }),
        };
        write_message_authenticated(&mut writer, &msg, &session_key).await?;

        let response = read_message_authenticated(&mut reader, &session_key).await?;
        match response.kind {
            WireMessageKind::Response(WireResponse::AgentResponse { text }) => Ok(text),
            WireMessageKind::Response(WireResponse::Error { code, message }) => Err(
                WireError::HandshakeFailed(format!("Remote error {code}: {message}")),
            ),
            _ => Err(WireError::HandshakeFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Internal accept loop — runs in a spawned task.
    async fn accept_loop(
        listener: TcpListener,
        node: Arc<PeerNode>,
        registry: PeerRegistry,
        handle: Arc<dyn PeerHandle>,
    ) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("OFP: accepted connection from {}", addr);
                    let node = Arc::clone(&node);
                    let registry = registry.clone();
                    let handle = Arc::clone(&handle);
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_inbound(stream, addr, &node, &registry, &*handle).await
                        {
                            debug!("OFP: inbound connection from {} ended: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("OFP: accept error: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Handle a single inbound connection: perform handshake, then enter message loop.
    async fn handle_inbound(
        stream: TcpStream,
        addr: SocketAddr,
        node: &PeerNode,
        registry: &PeerRegistry,
        handle: &dyn PeerHandle,
    ) -> Result<(), WireError> {
        let (mut reader, mut writer) = stream.into_split();

        // Read the incoming handshake request
        let msg = read_message(&mut reader).await?;
        let (peer_node_id, session_key) = match &msg.kind {
            WireMessageKind::Request(WireRequest::Handshake {
                node_id,
                node_name,
                protocol_version,
                agents,
                nonce,
                auth_hmac,
            }) => {
                if *protocol_version != PROTOCOL_VERSION {
                    let err_resp = WireMessage {
                        id: msg.id.clone(),
                        kind: WireMessageKind::Response(WireResponse::Error {
                            code: 1,
                            message: format!(
                                "Protocol version mismatch: expected {}, got {}",
                                PROTOCOL_VERSION, protocol_version
                            ),
                        }),
                    };
                    write_message(&mut writer, &err_resp).await?;
                    return Err(WireError::VersionMismatch {
                        local: PROTOCOL_VERSION,
                        remote: *protocol_version,
                    });
                }

                // SECURITY: Check for nonce replay before verifying HMAC
                if let Err(replay_err) = node.nonce_tracker.check_and_record(nonce) {
                    let err_resp = WireMessage {
                        id: msg.id.clone(),
                        kind: WireMessageKind::Response(WireResponse::Error {
                            code: 403,
                            message: "Nonce replay rejected".to_string(),
                        }),
                    };
                    write_message(&mut writer, &err_resp).await?;
                    return Err(WireError::HandshakeFailed(replay_err));
                }

                // SECURITY: Verify the incoming HMAC
                let expected_data = format!("{}{}", nonce, node_id);
                if !hmac_verify(
                    &node.config.shared_secret,
                    expected_data.as_bytes(),
                    auth_hmac,
                ) {
                    let err_resp = WireMessage {
                        id: msg.id.clone(),
                        kind: WireMessageKind::Response(WireResponse::Error {
                            code: 403,
                            message: "HMAC authentication failed".to_string(),
                        }),
                    };
                    write_message(&mut writer, &err_resp).await?;
                    return Err(WireError::HandshakeFailed(
                        "HMAC verification failed on incoming Handshake".into(),
                    ));
                }

                // Send handshake ack with our own HMAC
                let ack_nonce = uuid::Uuid::new_v4().to_string();
                let ack_auth_data = format!("{}{}", ack_nonce, node.config.node_id);
                let ack_hmac = hmac_sign(&node.config.shared_secret, ack_auth_data.as_bytes());

                let ack = WireMessage {
                    id: msg.id.clone(),
                    kind: WireMessageKind::Response(WireResponse::HandshakeAck {
                        node_id: node.config.node_id.clone(),
                        node_name: node.config.node_name.clone(),
                        protocol_version: PROTOCOL_VERSION,
                        agents: handle.local_agents(),
                        nonce: ack_nonce.clone(),
                        auth_hmac: ack_hmac,
                    }),
                };
                write_message(&mut writer, &ack).await?;

                // SECURITY: Derive per-session key (server side: their nonce first, our nonce second)
                let session_key = derive_session_key(
                    &node.config.shared_secret,
                    nonce,        // client's nonce
                    &ack_nonce,   // our nonce
                );

                info!(
                    "OFP: handshake with {} ({}) from {} — {} agents",
                    node_name,
                    node_id,
                    addr,
                    agents.len()
                );

                // Register the peer
                registry.add_peer(PeerEntry {
                    node_id: node_id.clone(),
                    node_name: node_name.clone(),
                    address: addr,
                    agents: agents.clone(),
                    state: PeerState::Connected,
                    connected_at: chrono::Utc::now(),
                    protocol_version: *protocol_version,
                });

                (node_id.clone(), session_key)
            }
            // SECURITY: Reject all non-Handshake initial messages.
            // Clients MUST complete HMAC-authenticated handshake before sending
            // any requests (AgentMessage, Ping, Discover, etc.).
            _ => {
                warn!(
                    "OFP: rejected unauthenticated message from {} — handshake required",
                    addr
                );
                let err_resp = WireMessage {
                    id: msg.id.clone(),
                    kind: WireMessageKind::Response(WireResponse::Error {
                        code: 401,
                        message: "Authentication required: complete HMAC handshake first"
                            .to_string(),
                    }),
                };
                write_message(&mut writer, &err_resp).await?;
                return Err(WireError::HandshakeFailed(
                    "Rejected unauthenticated request — handshake required".into(),
                ));
            }
        };

        // Enter the message dispatch loop with per-message HMAC
        if let Err(e) =
            connection_loop(&mut reader, &mut writer, &peer_node_id, registry, handle, Some(&session_key)).await
        {
            debug!("OFP: connection with {} ended: {}", peer_node_id, e);
        }
        registry.mark_disconnected(&peer_node_id);

        Ok(())
    }
}

/// Handle a single request message and produce a response.
#[allow(dead_code)]
async fn handle_request(
    msg: &WireMessage,
    handle: &dyn PeerHandle,
    node: &PeerNode,
) -> WireMessage {
    let kind = match &msg.kind {
        WireMessageKind::Request(WireRequest::Ping) => {
            WireMessageKind::Response(WireResponse::Pong {
                uptime_secs: node.start_time.elapsed().as_secs(),
            })
        }
        WireMessageKind::Request(WireRequest::Discover { query }) => {
            let agents = handle.discover_agents(query);
            WireMessageKind::Response(WireResponse::DiscoverResult { agents })
        }
        WireMessageKind::Request(WireRequest::AgentMessage {
            agent,
            message,
            sender,
        }) => match handle
            .handle_agent_message(agent, message, sender.as_deref())
            .await
        {
            Ok(text) => WireMessageKind::Response(WireResponse::AgentResponse { text }),
            Err(e) => WireMessageKind::Response(WireResponse::Error {
                code: 500,
                message: e,
            }),
        },
        WireMessageKind::Request(WireRequest::Handshake { .. }) => {
            // Shouldn't get a second handshake in the message loop
            WireMessageKind::Response(WireResponse::Error {
                code: 400,
                message: "Already handshaked".to_string(),
            })
        }
        _ => WireMessageKind::Response(WireResponse::Error {
            code: 400,
            message: "Unexpected message type".to_string(),
        }),
    };

    WireMessage {
        id: msg.id.clone(),
        kind,
    }
}

/// Read/write message loop for an established connection.
///
/// If `session_key` is provided, all post-handshake messages use per-message HMAC.
async fn connection_loop(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    peer_node_id: &str,
    registry: &PeerRegistry,
    handle: &dyn PeerHandle,
    session_key: Option<&str>,
) -> Result<(), WireError> {
    loop {
        let msg = match if let Some(key) = session_key {
            read_message_authenticated(reader, key).await
        } else {
            read_message(reader).await
        } {
            Ok(m) => m,
            Err(WireError::ConnectionClosed) => return Ok(()),
            Err(e) => return Err(e),
        };

        match &msg.kind {
            // Handle notifications (no response needed)
            WireMessageKind::Notification(notif) => {
                handle_notification(peer_node_id, notif, registry);
            }
            // Handle requests (produce response)
            WireMessageKind::Request(_) => {
                let response = handle_request_in_loop(&msg, handle).await;
                if let Some(key) = session_key {
                    write_message_authenticated(writer, &response, key).await?;
                } else {
                    write_message(writer, &response).await?;
                }
            }
            // We don't expect to receive responses in the connection loop
            WireMessageKind::Response(_) => {
                warn!(
                    "OFP: unexpected response message from {}: {:?}",
                    peer_node_id, msg.id
                );
            }
        }
    }
}

/// Handle request inside the connection loop (no PeerNode reference needed for most ops).
async fn handle_request_in_loop(msg: &WireMessage, handle: &dyn PeerHandle) -> WireMessage {
    let kind = match &msg.kind {
        WireMessageKind::Request(WireRequest::Ping) => {
            WireMessageKind::Response(WireResponse::Pong {
                uptime_secs: handle.uptime_secs(),
            })
        }
        WireMessageKind::Request(WireRequest::Discover { query }) => {
            let agents = handle.discover_agents(query);
            WireMessageKind::Response(WireResponse::DiscoverResult { agents })
        }
        WireMessageKind::Request(WireRequest::AgentMessage {
            agent,
            message,
            sender,
        }) => match handle
            .handle_agent_message(agent, message, sender.as_deref())
            .await
        {
            Ok(text) => WireMessageKind::Response(WireResponse::AgentResponse { text }),
            Err(e) => WireMessageKind::Response(WireResponse::Error {
                code: 500,
                message: e,
            }),
        },
        _ => WireMessageKind::Response(WireResponse::Error {
            code: 400,
            message: "Unexpected request in connection loop".to_string(),
        }),
    };

    WireMessage {
        id: msg.id.clone(),
        kind,
    }
}

/// Process an incoming notification.
fn handle_notification(peer_node_id: &str, notif: &WireNotification, registry: &PeerRegistry) {
    match notif {
        WireNotification::AgentSpawned { agent } => {
            info!(
                "OFP: peer {} spawned agent {} ({})",
                peer_node_id, agent.name, agent.id
            );
            registry.add_agent(peer_node_id, agent.clone());
        }
        WireNotification::AgentTerminated { agent_id } => {
            info!("OFP: peer {} terminated agent {}", peer_node_id, agent_id);
            registry.remove_agent(peer_node_id, agent_id);
        }
        WireNotification::ShuttingDown => {
            info!("OFP: peer {} is shutting down", peer_node_id);
            registry.mark_disconnected(peer_node_id);
        }
    }
}

/// Derive a per-session HMAC key from the shared secret and both handshake nonces.
///
/// `session_key = HMAC-SHA256(shared_secret, our_nonce || their_nonce)`
///
/// This ensures each connection has a unique key even with the same shared secret.
pub fn derive_session_key(shared_secret: &str, our_nonce: &str, their_nonce: &str) -> String {
    let data = format!("{}{}", our_nonce, their_nonce);
    hmac_sign(shared_secret, data.as_bytes())
}

/// Write a framed message (4-byte length + JSON) to a TCP stream.
pub async fn write_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &WireMessage,
) -> Result<(), WireError> {
    let bytes = encode_message(msg)?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// SECURITY: Write a framed message with per-message HMAC appended.
///
/// Format: [4-byte length][JSON body][64-byte hex HMAC]
/// The HMAC covers the JSON body and prevents tampering on authenticated connections.
pub async fn write_message_authenticated(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &WireMessage,
    session_key: &str,
) -> Result<(), WireError> {
    let json_bytes = serde_json::to_vec(msg)?;
    let mac = hmac_sign(session_key, &json_bytes);
    let mac_bytes = mac.as_bytes(); // 64 hex chars

    // Total frame = JSON + 64-byte HMAC
    let total_len = json_bytes.len() + mac_bytes.len();
    let len_bytes = (total_len as u32).to_be_bytes();

    writer.write_all(&len_bytes).await?;
    writer.write_all(&json_bytes).await?;
    writer.write_all(mac_bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a framed message (4-byte length + JSON) from a TCP stream.
pub async fn read_message(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
) -> Result<WireMessage, WireError> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(WireError::ConnectionClosed);
        }
        Err(e) => return Err(WireError::Io(e)),
    }

    let len = decode_length(&header);
    if len > MAX_MESSAGE_SIZE {
        return Err(WireError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body).await?;

    let msg = decode_message(&body)?;
    Ok(msg)
}

/// SECURITY: Read a framed message and verify per-message HMAC.
///
/// Expected format: [4-byte length][JSON body][64-byte hex HMAC]
/// Returns error if HMAC verification fails (tampered or forged message).
pub async fn read_message_authenticated(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    session_key: &str,
) -> Result<WireMessage, WireError> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(WireError::ConnectionClosed);
        }
        Err(e) => return Err(WireError::Io(e)),
    }

    let len = decode_length(&header);
    if len > MAX_MESSAGE_SIZE {
        return Err(WireError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    // HMAC is 64 hex chars appended after JSON
    const HMAC_HEX_LEN: usize = 64;
    let total_len = len as usize;
    if total_len < HMAC_HEX_LEN + 2 {
        // Minimum: "{}" + 64 HMAC chars
        return Err(WireError::HandshakeFailed(
            "Message too short for authenticated frame".into(),
        ));
    }

    let mut frame = vec![0u8; total_len];
    reader.read_exact(&mut frame).await?;

    let json_len = total_len - HMAC_HEX_LEN;
    let json_bytes = &frame[..json_len];
    let received_mac = std::str::from_utf8(&frame[json_len..])
        .map_err(|_| WireError::HandshakeFailed("Invalid HMAC encoding".into()))?;

    // Verify HMAC
    if !hmac_verify(session_key, json_bytes, received_mac) {
        return Err(WireError::HandshakeFailed(
            "Per-message HMAC verification failed — message tampered or forged".into(),
        ));
    }

    let msg = serde_json::from_slice(json_bytes)?;
    Ok(msg)
}

/// Broadcast an HMAC-authenticated notification to all connected peers.
///
/// SECURITY: Each peer connection gets a unique HMAC signature derived from
/// the shared secret and a fresh nonce, preventing forgery and replay attacks.
pub async fn broadcast_notification(
    registry: &PeerRegistry,
    notification: WireNotification,
    shared_secret: &str,
) -> Vec<(String, WireError)> {
    let peers = registry.connected_peers();
    let mut errors = Vec::new();

    for peer in peers {
        let msg = WireMessage {
            id: uuid::Uuid::new_v4().to_string(),
            kind: WireMessageKind::Notification(notification.clone()),
        };

        match TcpStream::connect(peer.address).await {
            Ok(stream) => {
                let (_, mut writer) = stream.into_split();
                // SECURITY: Derive a per-message key from shared secret + fresh nonce
                let nonce = uuid::Uuid::new_v4().to_string();
                let session_key = hmac_sign(shared_secret, nonce.as_bytes());
                if let Err(e) = write_message_authenticated(&mut writer, &msg, &session_key).await
                {
                    errors.push((peer.node_id.clone(), e));
                }
            }
            Err(e) => {
                errors.push((peer.node_id.clone(), WireError::Io(e)));
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Minimal PeerHandle for testing.
    struct TestHandle {
        agents: Vec<RemoteAgentInfo>,
        uptime: AtomicU64,
    }

    impl TestHandle {
        fn new() -> Self {
            Self {
                agents: vec![RemoteAgentInfo {
                    id: "test-agent-1".to_string(),
                    name: "echo".to_string(),
                    description: "Echo agent".to_string(),
                    tags: vec!["test".to_string()],
                    tools: vec![],
                    state: "running".to_string(),
                }],
                uptime: AtomicU64::new(42),
            }
        }
    }

    #[async_trait]
    impl PeerHandle for TestHandle {
        fn local_agents(&self) -> Vec<RemoteAgentInfo> {
            self.agents.clone()
        }

        async fn handle_agent_message(
            &self,
            agent: &str,
            message: &str,
            _sender: Option<&str>,
        ) -> Result<String, String> {
            Ok(format!("Echo from {agent}: {message}"))
        }

        fn discover_agents(&self, query: &str) -> Vec<RemoteAgentInfo> {
            let q = query.to_lowercase();
            self.agents
                .iter()
                .filter(|a| a.name.to_lowercase().contains(&q))
                .cloned()
                .collect()
        }

        fn uptime_secs(&self) -> u64 {
            self.uptime.load(Ordering::Relaxed)
        }
    }

    #[tokio::test]
    async fn test_peer_start_and_connect() {
        let registry1 = PeerRegistry::new();
        let handle1 = Arc::new(TestHandle::new());

        let config1 = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "node-1".to_string(),
            node_name: "kernel-1".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node1, _task1) = PeerNode::start(config1, registry1.clone(), handle1.clone())
            .await
            .unwrap();

        // Start a second node and connect to the first
        let registry2 = PeerRegistry::new();
        let handle2 = Arc::new(TestHandle::new());
        let config2 = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "node-2".to_string(),
            node_name: "kernel-2".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node2, _task2) = PeerNode::start(config2, registry2.clone(), handle2.clone())
            .await
            .unwrap();

        // Node2 connects to Node1
        node2
            .connect_to_peer(node1.local_addr(), handle2)
            .await
            .unwrap();

        // Registry2 should now have node-1 as a connected peer
        assert_eq!(registry2.connected_count(), 1);
        let peer = registry2.get_peer("node-1").unwrap();
        assert_eq!(peer.node_name, "kernel-1");
        assert_eq!(peer.agents.len(), 1);
        assert_eq!(peer.agents[0].name, "echo");

        // Registry1 should have node-2 (from inbound handshake)
        // Give the accept loop a moment to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(registry1.connected_count(), 1);
    }

    #[tokio::test]
    async fn test_unauthenticated_agent_message_rejected() {
        let registry = PeerRegistry::new();
        let handle = Arc::new(TestHandle::new());

        let config = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "server".to_string(),
            node_name: "server-node".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node, _task) = PeerNode::start(config, registry.clone(), handle.clone())
            .await
            .unwrap();

        // SECURITY TEST: Sending an AgentMessage without handshake must be rejected
        let addr = node.local_addr();
        let stream = TcpStream::connect(addr).await.unwrap();
        let (mut reader, mut writer) = stream.into_split();

        let msg = WireMessage {
            id: "req-1".to_string(),
            kind: WireMessageKind::Request(WireRequest::AgentMessage {
                agent: "echo".to_string(),
                message: "Hello, world!".to_string(),
                sender: Some("client".to_string()),
            }),
        };
        write_message(&mut writer, &msg).await.unwrap();

        let response = read_message(&mut reader).await.unwrap();
        assert_eq!(response.id, "req-1");
        match response.kind {
            WireMessageKind::Response(WireResponse::Error { code, message }) => {
                assert_eq!(code, 401);
                assert!(
                    message.contains("handshake"),
                    "Expected handshake-required error, got: {message}"
                );
            }
            other => panic!("Expected Error(401), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_unauthenticated_ping_rejected() {
        let registry = PeerRegistry::new();
        let handle = Arc::new(TestHandle::new());

        let config = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "server".to_string(),
            node_name: "server-node".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node, _task) = PeerNode::start(config, registry, handle).await.unwrap();

        // SECURITY TEST: Sending a Ping without handshake must be rejected
        let stream = TcpStream::connect(node.local_addr()).await.unwrap();
        let (mut reader, mut writer) = stream.into_split();

        let msg = WireMessage {
            id: "ping-1".to_string(),
            kind: WireMessageKind::Request(WireRequest::Ping),
        };
        write_message(&mut writer, &msg).await.unwrap();

        let response = read_message(&mut reader).await.unwrap();
        match response.kind {
            WireMessageKind::Response(WireResponse::Error { code, .. }) => {
                assert_eq!(code, 401);
            }
            other => panic!("Expected Error(401), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_unauthenticated_discover_rejected() {
        let registry = PeerRegistry::new();
        let handle = Arc::new(TestHandle::new());

        let config = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "server".to_string(),
            node_name: "server-node".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node, _task) = PeerNode::start(config, registry, handle).await.unwrap();

        // SECURITY TEST: Sending a Discover without handshake must be rejected
        let stream = TcpStream::connect(node.local_addr()).await.unwrap();
        let (mut reader, mut writer) = stream.into_split();

        let msg = WireMessage {
            id: "disc-1".to_string(),
            kind: WireMessageKind::Request(WireRequest::Discover {
                query: "echo".to_string(),
            }),
        };
        write_message(&mut writer, &msg).await.unwrap();

        let response = read_message(&mut reader).await.unwrap();
        match response.kind {
            WireMessageKind::Response(WireResponse::Error { code, .. }) => {
                assert_eq!(code, 401);
            }
            other => panic!("Expected Error(401), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_handshake_and_message_loop() {
        let registry1 = PeerRegistry::new();
        let handle1 = Arc::new(TestHandle::new());

        let config1 = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "node-a".to_string(),
            node_name: "kernel-a".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node1, _task1) = PeerNode::start(config1, registry1.clone(), handle1.clone())
            .await
            .unwrap();

        let registry2 = PeerRegistry::new();
        let handle2 = Arc::new(TestHandle::new());
        let config2 = PeerConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            node_id: "node-b".to_string(),
            node_name: "kernel-b".to_string(),
            shared_secret: "test-secret-for-unit-tests".to_string(),
        };
        let (node2, _task2) = PeerNode::start(config2, registry2.clone(), handle2.clone())
            .await
            .unwrap();

        // Connect node2 → node1
        node2
            .connect_to_peer(node1.local_addr(), handle2)
            .await
            .unwrap();

        // Both should see each other
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(registry2.connected_count(), 1);
        assert_eq!(registry1.connected_count(), 1);

        // Verify agent discovery across the wire
        let remote_agents = registry2.find_agents("echo");
        assert_eq!(remote_agents.len(), 1);
        assert_eq!(remote_agents[0].peer_node_id, "node-a");
    }

    #[test]
    fn test_peer_config_default() {
        let config = PeerConfig::default();
        assert_eq!(config.node_name, "openfang-node");
        assert!(!config.node_id.is_empty());
    }

    // ── Nonce replay protection tests ────────────────────────────────────

    #[test]
    fn test_nonce_tracker_fresh_nonce_accepted() {
        let tracker = NonceTracker::new();
        assert!(tracker.check_and_record("nonce-1").is_ok());
        assert!(tracker.check_and_record("nonce-2").is_ok());
        assert!(tracker.check_and_record("nonce-3").is_ok());
    }

    #[test]
    fn test_nonce_tracker_replay_rejected() {
        let tracker = NonceTracker::new();
        assert!(tracker.check_and_record("nonce-1").is_ok());
        // Second use of same nonce = replay attack
        let result = tracker.check_and_record("nonce-1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("replay"));
    }

    #[test]
    fn test_nonce_tracker_different_nonces_ok() {
        let tracker = NonceTracker::new();
        for i in 0..100 {
            assert!(tracker.check_and_record(&format!("unique-{i}")).is_ok());
        }
    }

    // ── Per-message HMAC tests ───────────────────────────────────────────

    #[test]
    fn test_derive_session_key_deterministic() {
        let key1 = derive_session_key("secret", "nonce-a", "nonce-b");
        let key2 = derive_session_key("secret", "nonce-a", "nonce-b");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_derive_session_key_different_nonces() {
        let key1 = derive_session_key("secret", "nonce-a", "nonce-b");
        let key2 = derive_session_key("secret", "nonce-c", "nonce-d");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_session_key_order_matters() {
        let key1 = derive_session_key("secret", "nonce-a", "nonce-b");
        let key2 = derive_session_key("secret", "nonce-b", "nonce-a");
        assert_ne!(key1, key2);
    }
}
