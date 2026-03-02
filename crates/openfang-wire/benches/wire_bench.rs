//! Criterion benchmarks for the OpenFang Wire Protocol.
//!
//! Covers the per-message hot path (encode/decode/HMAC) and registry
//! operations that run on every peer interaction.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use openfang_wire::message::*;
use openfang_wire::peer::hmac_sign;
use openfang_wire::registry::{PeerEntry, PeerRegistry};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_ping() -> WireMessage {
    WireMessage {
        id: "bench-ping".to_string(),
        kind: WireMessageKind::Request(WireRequest::Ping),
    }
}

fn make_handshake(n_agents: usize) -> WireMessage {
    let agents: Vec<RemoteAgentInfo> = (0..n_agents)
        .map(|i| RemoteAgentInfo {
            id: format!("agent-{i}"),
            name: format!("agent-{i}"),
            description: format!("Benchmark agent number {i}"),
            tags: vec!["bench".to_string(), format!("group-{}", i % 5)],
            tools: vec!["tool_a".to_string(), "tool_b".to_string()],
            state: "running".to_string(),
        })
        .collect();

    WireMessage {
        id: "bench-hs".to_string(),
        kind: WireMessageKind::Request(WireRequest::Handshake {
            node_id: "bench-node".to_string(),
            node_name: "bench-kernel".to_string(),
            protocol_version: PROTOCOL_VERSION,
            agents,
            nonce: "bench-nonce-0123456789abcdef".to_string(),
            auth_hmac: "deadbeef".to_string(),
        }),
    }
}

fn make_agent_message(payload_size: usize) -> WireMessage {
    WireMessage {
        id: "bench-am".to_string(),
        kind: WireMessageKind::Request(WireRequest::AgentMessage {
            agent: "coder".to_string(),
            message: "x".repeat(payload_size),
            sender: Some("orchestrator".to_string()),
        }),
    }
}

fn make_remote_agent(id: &str, name: &str, tags: &[&str]) -> RemoteAgentInfo {
    RemoteAgentInfo {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("{name} agent"),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        tools: vec!["tool_a".to_string()],
        state: "running".to_string(),
    }
}

fn make_peer_entry(node_id: &str, agents: Vec<RemoteAgentInfo>) -> PeerEntry {
    PeerEntry {
        node_id: node_id.to_string(),
        node_name: format!("{node_id}-name"),
        address: "127.0.0.1:9000".parse().unwrap(),
        agents,
        state: openfang_wire::registry::PeerState::Connected,
        connected_at: chrono::Utc::now(),
        protocol_version: 1,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: Message encoding / decoding
// ---------------------------------------------------------------------------

fn bench_encode_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode");

    // Ping (smallest message)
    let ping = make_ping();
    group.bench_function("ping", |b| {
        b.iter(|| {
            let bytes = encode_message(black_box(&ping)).unwrap();
            let header: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let _ = decode_length(&header);
            let _ = decode_message(black_box(&bytes[4..])).unwrap();
        })
    });

    // Handshake with varying agent counts
    for n_agents in [1, 10, 50] {
        let hs = make_handshake(n_agents);
        group.bench_with_input(
            BenchmarkId::new("handshake", n_agents),
            &hs,
            |b, msg| {
                b.iter(|| {
                    let bytes = encode_message(black_box(msg)).unwrap();
                    let _ = decode_message(black_box(&bytes[4..])).unwrap();
                })
            },
        );
    }

    // AgentMessage with varying payload sizes
    for size in [64, 1024, 16384] {
        let am = make_agent_message(size);
        group.bench_with_input(
            BenchmarkId::new("agent_message_bytes", size),
            &am,
            |b, msg| {
                b.iter(|| {
                    let bytes = encode_message(black_box(msg)).unwrap();
                    let _ = decode_message(black_box(&bytes[4..])).unwrap();
                })
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: HMAC sign / verify
// ---------------------------------------------------------------------------

fn bench_hmac(c: &mut Criterion) {
    let mut group = c.benchmark_group("hmac");
    let secret = "bench-shared-secret-key-for-testing";

    // Short payload (typical nonce+node_id ≈ 70 bytes)
    let short_data = "a1b2c3d4-e5f6-7890-abcd-ef1234567890node-bench-id-001";
    group.bench_function("sign_short", |b| {
        b.iter(|| hmac_sign(black_box(secret), black_box(short_data.as_bytes())))
    });

    // Verify (includes sign + constant-time compare)
    let sig = hmac_sign(secret, short_data.as_bytes());
    group.bench_function("verify_short", |b| {
        b.iter(|| {
            openfang_wire::peer::hmac_verify(
                black_box(secret),
                black_box(short_data.as_bytes()),
                black_box(&sig),
            )
        })
    });

    // Larger payload (1 KB — simulates signing a message body)
    let large_data = "x".repeat(1024);
    group.bench_function("sign_1kb", |b| {
        b.iter(|| hmac_sign(black_box(secret), black_box(large_data.as_bytes())))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: PeerRegistry operations
// ---------------------------------------------------------------------------

fn bench_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry");

    // add_peer at various registry sizes
    for n_peers in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("add_peer", n_peers),
            &n_peers,
            |b, &n| {
                b.iter_with_setup(
                    || {
                        let reg = PeerRegistry::new();
                        for i in 0..n {
                            reg.add_peer(make_peer_entry(
                                &format!("node-{i}"),
                                vec![make_remote_agent(
                                    &format!("a-{i}"),
                                    &format!("agent-{i}"),
                                    &["code"],
                                )],
                            ));
                        }
                        reg
                    },
                    |reg| {
                        reg.add_peer(make_peer_entry(
                            "node-new",
                            vec![make_remote_agent("a-new", "new-agent", &["bench"])],
                        ));
                    },
                );
            },
        );
    }

    // get_peer lookup
    for n_peers in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("get_peer", n_peers),
            &n_peers,
            |b, &n| {
                let reg = PeerRegistry::new();
                for i in 0..n {
                    reg.add_peer(make_peer_entry(
                        &format!("node-{i}"),
                        vec![make_remote_agent(
                            &format!("a-{i}"),
                            &format!("agent-{i}"),
                            &["code"],
                        )],
                    ));
                }
                let target = format!("node-{}", n / 2);
                b.iter(|| {
                    let _ = reg.get_peer(black_box(&target));
                })
            },
        );
    }

    // find_agents — searches name, description, tags across all peers
    for n_peers in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("find_agents", n_peers),
            &n_peers,
            |b, &n| {
                let reg = PeerRegistry::new();
                for i in 0..n {
                    reg.add_peer(make_peer_entry(
                        &format!("node-{i}"),
                        vec![
                            make_remote_agent(
                                &format!("a-{i}-0"),
                                &format!("coder-{i}"),
                                &["code", "rust"],
                            ),
                            make_remote_agent(
                                &format!("a-{i}-1"),
                                &format!("writer-{i}"),
                                &["docs"],
                            ),
                        ],
                    ));
                }
                b.iter(|| {
                    let _ = reg.find_agents(black_box("code"));
                })
            },
        );
    }

    // all_remote_agents — full scan
    for n_peers in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("all_remote_agents", n_peers),
            &n_peers,
            |b, &n| {
                let reg = PeerRegistry::new();
                for i in 0..n {
                    reg.add_peer(make_peer_entry(
                        &format!("node-{i}"),
                        vec![make_remote_agent(
                            &format!("a-{i}"),
                            &format!("agent-{i}"),
                            &["code"],
                        )],
                    ));
                }
                b.iter(|| {
                    let _ = reg.all_remote_agents();
                })
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: Full handshake message construction (end-to-end CPU path)
// ---------------------------------------------------------------------------

fn bench_handshake_construction(c: &mut Criterion) {
    let secret = "bench-shared-secret-key";

    c.bench_function("handshake_e2e_construct", |b| {
        b.iter(|| {
            let nonce = "fixed-nonce-for-bench";
            let node_id = "bench-node-id";
            let auth_data = format!("{}{}", nonce, node_id);
            let auth_hmac = hmac_sign(black_box(secret), auth_data.as_bytes());

            let msg = WireMessage {
                id: "hs-bench".to_string(),
                kind: WireMessageKind::Request(WireRequest::Handshake {
                    node_id: node_id.to_string(),
                    node_name: "bench-kernel".to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    agents: vec![RemoteAgentInfo {
                        id: "a1".to_string(),
                        name: "coder".to_string(),
                        description: "Coding agent".to_string(),
                        tags: vec!["code".to_string()],
                        tools: vec!["file_read".to_string()],
                        state: "running".to_string(),
                    }],
                    nonce: nonce.to_string(),
                    auth_hmac,
                }),
            };

            let _ = encode_message(black_box(&msg)).unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_encode_decode,
    bench_hmac,
    bench_registry,
    bench_handshake_construction,
);
criterion_main!(benches);
