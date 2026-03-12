//! A2A Transport layer.
//!
//! Provides the transport abstraction for sending and receiving A2A messages.

use crate::error::A2AError;
use crate::protocol::A2AMessage;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Trait for A2A transport implementations.
#[async_trait]
pub trait A2ATransport: Send + Sync {
    /// Send a message through the transport.
    async fn send(&self, message: A2AMessage) -> Result<(), A2AError>;

    /// Receive a message from the transport.
    /// Returns None if the transport is closed.
    async fn receive(&mut self) -> Option<A2AMessage>;
}

/// MPSC-based transport for testing and local communication.
pub struct MpscTransport {
    sender: mpsc::Sender<A2AMessage>,
    receiver: mpsc::Receiver<A2AMessage>,
}

impl MpscTransport {
    /// Create a pair of connected transports for bidirectional communication.
    pub fn new() -> (Self, Self) {
        let (sender1, receiver1) = mpsc::channel(100);
        let (sender2, receiver2) = mpsc::channel(100);

        let transport1 = Self {
            sender: sender1,
            receiver: receiver2,
        };

        let transport2 = Self {
            sender: sender2,
            receiver: receiver1,
        };

        (transport1, transport2)
    }
}

#[async_trait]
impl A2ATransport for MpscTransport {
    async fn send(&self, message: A2AMessage) -> Result<(), A2AError> {
        self.sender
            .send(message)
            .await
            .map_err(|e| {
                warn!("Failed to send message: {}", e);
                A2AError::SendFailed(e.to_string())
            })
    }

    async fn receive(&mut self) -> Option<A2AMessage> {
        let msg = self.receiver.recv().await;
        if msg.is_some() {
            debug!("Received message");
        }
        msg
    }
}