//! OMTAE Wire Protocol (OFP) — agent-to-agent networking.
//!
//! Provides cross-machine agent discovery, authentication, and communication
//! over TCP connections using a JSON-RPC framed protocol.
//!
//! ## Architecture
//!
//! - **PeerNode**: Local network endpoint that listens for incoming connections
//! - **PeerRegistry**: Tracks known peers and their agents
//! - **WireMessage**: JSON-framed protocol messages
//! - **PeerHandle**: Trait for routing remote messages through the kernel

pub mod message;
pub mod peer;
pub mod registry;

pub use message::{WireMessage, WireRequest, WireResponse};
pub use peer::{PeerConfig, PeerNode};
pub use registry::{PeerEntry, PeerRegistry, RemoteAgent};
