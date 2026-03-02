//! Criterion benchmarks for openfang-channels hot paths.
//!
//! Covers: AgentRouter resolve, ChannelRateLimiter, formatter, ChannelMessage serde.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use openfang_channels::bridge::ChannelRateLimiter;
use openfang_channels::formatter::format_for_channel;
use openfang_channels::router::{AgentRouter, BindingContext};
use openfang_channels::types::{ChannelMessage, ChannelContent, ChannelType, ChannelUser};
use openfang_types::agent::AgentId;
use openfang_types::config::{
    AgentBinding, BindingMatchRule, BroadcastConfig, BroadcastStrategy, OutputFormat,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers: build test fixtures
// ---------------------------------------------------------------------------

fn make_channel_message(text_len: usize) -> ChannelMessage {
    ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: "msg-12345".to_string(),
        sender: ChannelUser {
            platform_id: "user_42".to_string(),
            display_name: "Alice".to_string(),
            openfang_user: Some("alice".to_string()),
        },
        content: ChannelContent::Text("x".repeat(text_len)),
        target_agent: None,
        timestamp: chrono::Utc::now(),
        is_group: false,
        thread_id: None,
        metadata: HashMap::new(),
    }
}

fn make_markdown_text(n_elements: usize) -> String {
    let mut text = String::new();
    for i in 0..n_elements {
        match i % 4 {
            0 => text.push_str(&format!("Here is **bold text {}** and ", i)),
            1 => text.push_str(&format!("some *italic {}* plus ", i)),
            2 => text.push_str(&format!("`code_block_{}` and ", i)),
            3 => text.push_str(&format!("[link {}](https://example.com/{}) ", i, i)),
            _ => unreachable!(),
        }
    }
    text
}

fn make_router_with_bindings(n_bindings: usize) -> AgentRouter {
    let mut router = AgentRouter::new();
    let default_id = AgentId::new();
    router.set_default(default_id);

    let mut bindings = Vec::with_capacity(n_bindings);
    for i in 0..n_bindings {
        let agent_name = format!("agent-{i}");
        let agent_id = AgentId::new();
        router.register_agent(agent_name.clone(), agent_id);
        bindings.push(AgentBinding {
            agent: agent_name,
            match_rule: BindingMatchRule {
                channel: Some(format!("channel-{i}")),
                peer_id: Some(format!("peer-{i}")),
                ..Default::default()
            },
        });
    }
    router.load_bindings(&bindings);

    // Also set some user defaults and direct routes
    for i in 0..n_bindings.min(50) {
        let id = AgentId::new();
        router.set_user_default(format!("user-{i}"), id);
        router.set_direct_route(
            "Telegram".to_string(),
            format!("tg_{i}"),
            id,
        );
    }

    router
}

fn make_router_with_broadcast(n_targets: usize) -> AgentRouter {
    let mut router = AgentRouter::new();
    let default_id = AgentId::new();
    router.set_default(default_id);

    let mut agent_names = Vec::new();
    for i in 0..n_targets {
        let name = format!("broadcast-agent-{i}");
        let id = AgentId::new();
        router.register_agent(name.clone(), id);
        agent_names.push(name);
    }

    let mut routes = HashMap::new();
    routes.insert("vip_user".to_string(), agent_names);
    router.load_broadcast(BroadcastConfig {
        strategy: BroadcastStrategy::Parallel,
        routes,
    });

    router
}

// ---------------------------------------------------------------------------
// Benchmarks: AgentRouter::resolve
// ---------------------------------------------------------------------------

fn bench_router_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_resolve");

    for n_bindings in [0, 10, 50, 200] {
        let router = make_router_with_bindings(n_bindings);

        // Case 1: Direct route hit (fastest path)
        group.bench_with_input(
            BenchmarkId::new("direct_route_hit", n_bindings),
            &router,
            |b, router| {
                b.iter(|| {
                    router.resolve(
                        black_box(&ChannelType::Telegram),
                        black_box("tg_0"),
                        black_box(Some("user-0")),
                    )
                })
            },
        );

        // Case 2: Binding miss → fall through to system default
        group.bench_with_input(
            BenchmarkId::new("binding_miss_to_default", n_bindings),
            &router,
            |b, router| {
                b.iter(|| {
                    router.resolve(
                        black_box(&ChannelType::Discord),
                        black_box("unknown_user"),
                        black_box(None),
                    )
                })
            },
        );

        // Case 3: Binding match (last binding)
        if n_bindings > 0 {
            let last = n_bindings - 1;
            let ctx = BindingContext {
                channel: format!("channel-{last}"),
                peer_id: format!("peer-{last}"),
                ..Default::default()
            };
            group.bench_with_input(
                BenchmarkId::new("binding_match_last", n_bindings),
                &(router, ctx),
                |b, (router, ctx)| {
                    b.iter(|| {
                        router.resolve_with_context(
                            black_box(&ChannelType::Custom(format!("channel-{last}"))),
                            black_box(&format!("peer-{last}")),
                            black_box(None),
                            black_box(ctx),
                        )
                    })
                },
            );
        }
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: Broadcast resolution
// ---------------------------------------------------------------------------

fn bench_broadcast(c: &mut Criterion) {
    let mut group = c.benchmark_group("broadcast");

    for n_targets in [1, 5, 20] {
        let router = make_router_with_broadcast(n_targets);

        group.bench_with_input(
            BenchmarkId::new("has_broadcast", n_targets),
            &router,
            |b, router| {
                b.iter(|| router.has_broadcast(black_box("vip_user")))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("resolve_broadcast", n_targets),
            &router,
            |b, router| {
                b.iter(|| router.resolve_broadcast(black_box("vip_user")))
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: ChannelRateLimiter
// ---------------------------------------------------------------------------

fn bench_rate_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_limiter");

    // Cold path: first call for a user
    group.bench_function("cold_check", |b| {
        let limiter = ChannelRateLimiter::default();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            limiter.check(
                black_box("telegram"),
                black_box(&format!("user_{i}")),
                black_box(60),
            )
        })
    });

    // Hot path: same user, many calls within window
    group.bench_function("hot_check_same_user", |b| {
        let limiter = ChannelRateLimiter::default();
        b.iter(|| {
            limiter.check(
                black_box("telegram"),
                black_box("hot_user"),
                black_box(1000), // high limit so we don't hit it
            )
        })
    });

    // Rate-limited path: user at the limit
    group.bench_function("rate_limited", |b| {
        let limiter = ChannelRateLimiter::default();
        // Fill up the bucket
        for _ in 0..60 {
            let _ = limiter.check("telegram", "limited_user", 60);
        }
        b.iter(|| {
            limiter.check(
                black_box("telegram"),
                black_box("limited_user"),
                black_box(60),
            )
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: Formatter (markdown → platform format)
// ---------------------------------------------------------------------------

fn bench_formatter(c: &mut Criterion) {
    let mut group = c.benchmark_group("formatter");

    for n_elements in [5, 20, 100] {
        let text = make_markdown_text(n_elements);

        group.bench_with_input(
            BenchmarkId::new("markdown_passthrough", n_elements),
            &text,
            |b, text| {
                b.iter(|| format_for_channel(black_box(text), OutputFormat::Markdown))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("to_telegram_html", n_elements),
            &text,
            |b, text| {
                b.iter(|| format_for_channel(black_box(text), OutputFormat::TelegramHtml))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("to_slack_mrkdwn", n_elements),
            &text,
            |b, text| {
                b.iter(|| format_for_channel(black_box(text), OutputFormat::SlackMrkdwn))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("to_plain_text", n_elements),
            &text,
            |b, text| {
                b.iter(|| format_for_channel(black_box(text), OutputFormat::PlainText))
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: ChannelMessage serde
// ---------------------------------------------------------------------------

fn bench_message_serde(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serde");

    for text_len in [64, 512, 4096] {
        let msg = make_channel_message(text_len);

        group.bench_with_input(
            BenchmarkId::new("serialize", text_len),
            &msg,
            |b, msg| {
                b.iter(|| serde_json::to_vec(black_box(msg)).unwrap())
            },
        );

        let json_bytes = serde_json::to_vec(&msg).unwrap();
        group.bench_with_input(
            BenchmarkId::new("deserialize", text_len),
            &json_bytes,
            |b, bytes| {
                b.iter(|| serde_json::from_slice::<ChannelMessage>(black_box(bytes)).unwrap())
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_router_resolve,
    bench_broadcast,
    bench_rate_limiter,
    bench_formatter,
    bench_message_serde,
);
criterion_main!(benches);
