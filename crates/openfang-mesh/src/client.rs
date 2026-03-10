//! [`MeshClient`] — sends tasks to remote OFP peers via the wire protocol.
//!
//! The `MeshClient` is a thin wrapper around the OFP wire protocol that
//! provides a high-level `send_task` API for dispatching tasks to remote
//! agents. It handles the HMAC handshake, message framing, and response
//! parsing transparently.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use openfang_wire::{
    message::{WireMessage, WireMessageKind, WireRequest, WireResponse, PROTOCOL_VERSION},
    peer::{read_message, write_message},
};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::MeshError;

/// Configuration for the [`MeshClient`].
#[derive(Debug, Clone)]
pub struct MeshClientConfig {
    /// This node's ID (used in the OFP handshake).
    pub node_id: String,
    /// This node's display name.
    pub node_name: String,
    /// Shared secret for HMAC authentication.
    pub shared_secret: String,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Request timeout (after connection is established).
    pub request_timeout: Duration,
}

impl Default for MeshClientConfig {
    fn default() -> Self {
        Self {
            node_id: Uuid::new_v4().to_string(),
            node_name: "openfang-mesh-client".to_string(),
            shared_secret: String::new(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(60),
        }
    }
}

/// A thin async client for sending tasks to remote OFP peers.
///
/// Each `send_task` call opens a new TCP connection, performs the HMAC
/// handshake, sends the task, waits for the response, and closes the
/// connection. Connection pooling is a future enhancement.
pub struct MeshClient {
    config: Arc<MeshClientConfig>,
}

impl MeshClient {
    /// Create a new [`MeshClient`] with the given configuration.
    pub fn new(config: MeshClientConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Send a task to a specific agent on a remote peer.
    ///
    /// # Arguments
    ///
    /// * `peer_addr` — the TCP address of the remote peer (e.g. `"127.0.0.1:50052"`)
    /// * `agent_id` — the name or ID of the target agent on the remote peer
    /// * `task` — the task description to send
    ///
    /// # Returns
    ///
    /// The agent's response text on success, or a [`MeshError`] on failure.
    pub async fn send_task(
        &self,
        peer_addr: &str,
        agent_id: &str,
        task: &str,
    ) -> Result<String, MeshError> {
        let addr: SocketAddr = peer_addr
            .parse()
            .map_err(|e| MeshError::RemotePeer(format!("Invalid peer address '{peer_addr}': {e}")))?;

        debug!(peer = %peer_addr, agent = %agent_id, "MeshClient: connecting to peer");

        // Connect with timeout
        let stream = timeout(self.config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| MeshError::RemotePeer(format!("Connection to {peer_addr} timed out")))?
            .map_err(|e| MeshError::Io(e))?;

        let (mut reader, mut writer) = stream.into_split();

        // ── HMAC Handshake ───────────────────────────────────────────────────
        let nonce = Uuid::new_v4().to_string();
        let auth_data = format!("{}{}", nonce, self.config.node_id);
        let auth_hmac = hmac_sign(&self.config.shared_secret, auth_data.as_bytes());

        let handshake = WireMessage {
            id: Uuid::new_v4().to_string(),
            kind: WireMessageKind::Request(WireRequest::Handshake {
                node_id: self.config.node_id.clone(),
                node_name: self.config.node_name.clone(),
                protocol_version: PROTOCOL_VERSION,
                agents: vec![],
                nonce,
                auth_hmac,
            }),
        };

        timeout(self.config.request_timeout, write_message(&mut writer, &handshake))
            .await
            .map_err(|_| MeshError::RemotePeer("Handshake write timed out".to_string()))?
            .map_err(|e| MeshError::Wire(e.to_string()))?;

        let ack = timeout(self.config.request_timeout, read_message(&mut reader))
            .await
            .map_err(|_| MeshError::RemotePeer("Handshake ack timed out".to_string()))?
            .map_err(|e| MeshError::Wire(e.to_string()))?;

        match &ack.kind {
            WireMessageKind::Response(WireResponse::HandshakeAck { protocol_version, .. }) => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(MeshError::RemotePeer(format!(
                        "Protocol version mismatch: local={PROTOCOL_VERSION}, remote={protocol_version}"
                    )));
                }
            }
            WireMessageKind::Response(WireResponse::Error { message, .. }) => {
                return Err(MeshError::RemotePeer(format!("Handshake rejected: {message}")));
            }
            _ => {
                return Err(MeshError::RemotePeer(
                    "Unexpected response to handshake".to_string(),
                ));
            }
        }

        // ── Send task ────────────────────────────────────────────────────────
        let msg = WireMessage {
            id: Uuid::new_v4().to_string(),
            kind: WireMessageKind::Request(WireRequest::AgentMessage {
                agent: agent_id.to_string(),
                message: task.to_string(),
                sender: Some(self.config.node_id.clone()),
            }),
        };

        timeout(self.config.request_timeout, write_message(&mut writer, &msg))
            .await
            .map_err(|_| MeshError::RemotePeer("Task write timed out".to_string()))?
            .map_err(|e| MeshError::Wire(e.to_string()))?;

        let response = timeout(self.config.request_timeout, read_message(&mut reader))
            .await
            .map_err(|_| MeshError::RemotePeer("Task response timed out".to_string()))?
            .map_err(|e| MeshError::Wire(e.to_string()))?;

        match response.kind {
            WireMessageKind::Response(WireResponse::AgentResponse { text }) => {
                debug!(peer = %peer_addr, agent = %agent_id, "MeshClient: received response");
                Ok(text)
            }
            WireMessageKind::Response(WireResponse::Error { message, .. }) => {
                warn!(peer = %peer_addr, agent = %agent_id, error = %message, "MeshClient: peer returned error");
                Err(MeshError::RemotePeer(message))
            }
            _ => Err(MeshError::RemotePeer(
                "Unexpected response to agent message".to_string(),
            )),
        }
    }
}

// ── HMAC helper ──────────────────────────────────────────────────────────────

/// Compute HMAC-SHA256(key, data) and return as lowercase hex.
fn hmac_sign(key: &str, data: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .unwrap_or_else(|_| HmacSha256::new_from_slice(b"default-key").unwrap());
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_client_config_defaults() {
        let config = MeshClientConfig::default();
        assert_eq!(config.node_name, "openfang-mesh-client");
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.request_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_hmac_sign_deterministic() {
        let sig1 = hmac_sign("secret", b"hello");
        let sig2 = hmac_sign("secret", b"hello");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_hmac_sign_different_keys() {
        let sig1 = hmac_sign("secret1", b"hello");
        let sig2 = hmac_sign("secret2", b"hello");
        assert_ne!(sig1, sig2);
    }
}
