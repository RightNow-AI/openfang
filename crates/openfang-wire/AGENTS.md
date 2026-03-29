<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-wire

## Purpose

OpenFang Wire Protocol (OFP) enables peer-to-peer agent-to-agent networking across machines. Agents can discover peers, authenticate, and send tasks/messages to remote agents using a JSON-RPC framed protocol over TCP. A `PeerNode` listens for incoming connections. A `PeerRegistry` tracks known peers and their agents. `WireMessage` is the core message type. The peer system supports HMAC-SHA256 authentication and rate limiting.

## Key Files

| File | Description |
|------|-------------|
| `src/lib.rs` | Public exports: `WireMessage`, `WireRequest`, `WireResponse`, `PeerNode`, `PeerRegistry`. |
| `src/message.rs` | Wire protocol: `WireMessage`, `WireRequest` (task delegation), `WireResponse` (status/result). Frame format and serialization. |
| `src/peer.rs` | `PeerNode` — listen socket, incoming connection handler, authentication, message dispatch. `PeerConfig` for host/port/secret key. |
| `src/registry.rs` | `PeerRegistry` — track known peers (`PeerEntry`), remote agents (`RemoteAgent`). Query/lookup by peer ID or agent name. |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Message types, peer listener, registry, types. |

## For AI Agents

### Working In This Directory

- **Extending wire protocol**: Add new `WireRequest` variants in `message.rs` (e.g., health check, capability query).
- **Authentication**: Enhance HMAC scheme (currently SHA256 + secret key) with TLS or token-based auth.
- **Rate limiting**: Add per-peer quotas or backpressure in `peer.rs`.
- **Service discovery**: Extend `PeerRegistry` to support registration backends (mDNS, Zookeeper, etc).
- **Message routing**: Enhance message dispatch logic to route to correct kernel handler.

### Testing Requirements

- Unit tests for message serialization/deserialization (JSON-RPC format).
- Test HMAC authentication — valid/invalid signatures.
- Test PeerRegistry lookups and peer tracking.
- Test PeerNode connection handling — accept, authenticate, dispatch.
- Test message frame encoding/decoding.
- No live network tests (use mocked sockets).

### Common Patterns

- Messages are JSON-RPC 2.0 framed over TCP with length prefix.
- Authentication is HMAC-SHA256(message_body, secret_key) — sent in Authorization header.
- `PeerEntry` tracks peer metadata (id, hostname, port, last_seen).
- `RemoteAgent` represents an agent accessible on a peer (name, agent_id, capabilities).
- Request/response pattern: client sends `WireRequest`, server responds with `WireResponse`.
- All network operations are async/tokio-based.

## Dependencies

### Internal

- `openfang-types` — shared types (AgentId, etc).

### External

- `tokio` — async networking, TCP listener/stream.
- `serde`, `serde_json` — JSON-RPC serialization.
- `uuid`, `chrono` — peer IDs, timestamps.
- `hmac`, `sha2`, `hex` — HMAC-SHA256 authentication.
- `subtle` — constant-time comparison for auth.
- `rand` — nonce generation.
- `dashmap` — concurrent peer map.

<!-- MANUAL: -->
