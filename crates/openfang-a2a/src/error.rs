//! Error types for the A2A protocol.

use thiserror::Error;

/// Errors that can occur in the A2A protocol.
#[derive(Debug, Error)]
pub enum A2AError {
    /// Failed to send a message through the transport.
    #[error("Failed to send message: {0}")]
    SendFailed(String),

    /// Failed to receive a response.
    #[error("Failed to receive response: {0}")]
    ReceiveFailed(String),

    /// Task response was dropped before being received.
    #[error("Task response channel closed for task: {0}")]
    TaskChannelClosed(String),

    /// Transport error.
    #[error("Transport error: {0}")]
    TransportError(String),

    /// Timeout waiting for response.
    #[error("Timeout waiting for response for task: {0}")]
    Timeout(String),

    /// Invalid message format.
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}