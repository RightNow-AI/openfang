<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-wire — OFP Network Protocol

## Purpose

Implements the OpenFang Wire Protocol (OFP) for agent-to-agent communication across machines. Provides peer discovery, authentication, and JSON-RPC message framing for remote agent invocation.

## Key Files

| File | Purpose |
|------|---------|
| `peer.rs` | `PeerNode` — local network endpoint that listens for incoming connections from other OpenFang instances |
| `message.rs` | `WireMessage`, `WireRequest`, `WireResponse` — JSON-RPC framed protocol messages |
| `registry.rs` | `PeerRegistry`, `PeerEntry`, `RemoteAgent` — tracks known peers and their exported agents |

## For AI Agents

**When to read:** Understand cross-machine agent communication, peer discovery, or remote agent invocation.

**Key types:**
- `PeerNode` — listens on a TCP socket for incoming peer connections
- `PeerRegistry` — registry of known peers and their agents
- `RemoteAgent` — reference to an agent exported by another peer
- `WireMessage` — JSON-RPC messages over TCP

**Common tasks:**
- Adding a remote agent → register in `PeerRegistry` with discovery info
- Implementing new wire protocol features → `message.rs` protocol additions
- Debugging peer connections → `peer.rs` connection logic

**Architecture note:** OFP is designed for trusted networks (firewalled environments, VPNs). Not recommended for untrusted internet connections without additional TLS/encryption wrapper.
