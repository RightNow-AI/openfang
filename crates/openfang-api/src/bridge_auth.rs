//! Per-spawn bridge authorization.
//!
//! The MCP bridge subprocess presents a token on each IPC call; the daemon
//! resolves that token to the `AgentId` it was issued for. This module owns
//! the spawn table — the source of truth for "which bridge process is acting
//! for which agent."
//!
//! ## Layering
//!
//! The runtime-facing abstraction (`TokenIssuer` trait, `SpawnGuard` struct)
//! lives in [`openfang_runtime::bridge_auth`] so the `claude-code` driver
//! can issue tokens without depending on this crate. `BridgeAuthority` is
//! the concrete impl — daemon-owned, holds the spawn table, performs
//! resolution.
//!
//! ## Lifetime model
//!
//! Each call to [`TokenIssuer::issue`] returns a [`SpawnGuard`]. The guard
//! holds the token and an `Arc<dyn TokenIssuer>` back-reference; dropping
//! the guard calls [`TokenIssuer::revoke`], which evicts the entry from the
//! spawn table (and the held [`Token`] zeroizes via its `ZeroizeOnDrop`
//! impl). Wire the guard into `BridgeMcpConfig` so its `Drop` runs when
//! the CC subprocess terminates.
//!
//! ## Identity is resolved, not asserted
//!
//! Bridge IPC requests carry only the token. The daemon never trusts an
//! `agent_id` claim from the bridge — it looks up the agent_id keyed by
//! token. This is the core fix for ANAI-31.
//!
//! ## Concurrency
//!
//! The spawn table is guarded by `std::sync::Mutex`. Critical sections are
//! `HashMap` insert/remove/lookup — sub-microsecond — so async contention is
//! a non-issue. We *never* hold the lock across `.await`; `clippy::
//! await_holding_lock` (default-warn for `std::sync::Mutex`) enforces this
//! at lint time, which `tokio::sync::Mutex` would not.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use openfang_runtime::bridge_auth::{SpawnGuard, TokenIssuer};
use openfang_types::agent::AgentId;
use openfang_types::bridge_auth::Token;

/// Authority that issues per-spawn tokens and resolves them back to agent IDs.
///
/// Held as `Arc<BridgeAuthority>` by the daemon; cloned into the spawn path
/// for each CC subprocess launch. Construct with [`BridgeAuthority::new`],
/// which uses `Arc::new_cyclic` to stash a `Weak<Self>` so that `&self`
/// trait methods can produce an `Arc<dyn TokenIssuer>` for `SpawnGuard`.
pub struct BridgeAuthority {
    spawns: Mutex<HashMap<Token, AgentId>>,
    /// Self-reference for handing out `Arc<dyn TokenIssuer>` to guards.
    /// Populated via `Arc::new_cyclic`. Upgraded inside [`Self::issue`].
    weak_self: Weak<BridgeAuthority>,
}

// Manual Debug — Token deliberately has no Debug to prevent accidental
// secret logging. Redact to live-spawn count only.
impl std::fmt::Debug for BridgeAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let live = self.spawns.lock().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("BridgeAuthority")
            .field("live_spawns", &live)
            .finish()
    }
}

impl BridgeAuthority {
    /// Construct an empty authority. Returns `Arc<Self>` because the
    /// authority must live behind an `Arc` — guards need to hold a strong
    /// reference back to it to evict on drop.
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|weak_self| Self {
            spawns: Mutex::new(HashMap::new()),
            weak_self: weak_self.clone(),
        })
    }

    /// Resolve a presented token to its bound agent. Returns `None` if the
    /// token is unknown or its spawn has terminated.
    ///
    /// Equality on `Token` is constant-time (`subtle::ConstantTimeEq`), so
    /// the per-slot comparison does not leak timing. `HashMap` probing is
    /// not constant-time, but the keys are 256-bit CSPRNG values an attacker
    /// cannot craft collisions for.
    pub fn resolve(&self, token: &Token) -> Option<AgentId> {
        let guard = self
            .spawns
            .lock()
            .expect("BridgeAuthority spawn table mutex poisoned");
        guard.get(token).copied()
    }

    /// Number of live spawns. Diagnostic / test only.
    #[doc(hidden)]
    pub fn live_spawn_count(&self) -> usize {
        self.spawns
            .lock()
            .expect("BridgeAuthority spawn table mutex poisoned")
            .len()
    }

    /// Cast an `Arc<BridgeAuthority>` into the trait-object form
    /// `Arc<dyn TokenIssuer>`. Ergonomic helper for callers in
    /// `openfang-cli` / `openfang-api::server` that need to hand the issuer
    /// to `OpenFangKernel::boot_with_config_and_issuer` without naming the
    /// `TokenIssuer` trait themselves. ANAI-31 phase E.
    pub fn as_token_issuer(self: &Arc<Self>) -> Arc<dyn TokenIssuer> {
        self.clone() as Arc<dyn TokenIssuer>
    }

    /// Upgrade the stashed `Weak<Self>` into the `Arc<dyn TokenIssuer>` that
    /// each [`SpawnGuard`] holds. Panics if the authority has been dropped
    /// while a method on it is still running — impossible in practice, since
    /// the caller holds a `&self` borrow that pins the `Arc`.
    fn issuer_arc(&self) -> Arc<dyn TokenIssuer> {
        let strong: Arc<BridgeAuthority> = self
            .weak_self
            .upgrade()
            .expect("BridgeAuthority dropped while its own method is running");
        strong as Arc<dyn TokenIssuer>
    }
}

impl TokenIssuer for BridgeAuthority {
    fn issue(&self, agent_id: AgentId) -> SpawnGuard {
        // The Hash + Eq impls on Token operate on the full 32 random bytes,
        // so duplicate-key collision is effectively impossible (2^-256).
        // Defensively: if a duplicate ever occurs we'd overwrite the prior
        // binding (and the prior guard's Drop would then evict *our* new
        // entry on its scope exit). To avoid that subtle aliasing bug,
        // regenerate on collision.
        let token = {
            let mut guard = self
                .spawns
                .lock()
                .expect("BridgeAuthority spawn table mutex poisoned");
            loop {
                let candidate = Token::generate();
                if !guard.contains_key(&candidate) {
                    guard.insert(candidate.clone(), agent_id);
                    break candidate;
                }
            }
        };
        SpawnGuard::new(self.issuer_arc(), token)
    }

    fn revoke(&self, token: &Token) {
        // If the mutex is poisoned during shutdown, eviction is best-effort.
        // The `Token` field itself still zeroizes via `ZeroizeOnDrop`.
        if let Ok(mut spawns) = self.spawns.lock() {
            spawns.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentId {
        AgentId::new()
    }

    #[test]
    fn issue_registers_a_spawn() {
        let authority = BridgeAuthority::new();
        let a = agent();
        let guard = authority.issue(a);
        assert_eq!(authority.live_spawn_count(), 1);
        assert_eq!(authority.resolve(guard.token()), Some(a));
    }

    #[test]
    fn drop_evicts_the_spawn() {
        let authority = BridgeAuthority::new();
        let a = agent();
        let token_copy;
        {
            let guard = authority.issue(a);
            token_copy = guard.token().clone();
            assert_eq!(authority.live_spawn_count(), 1);
        }
        assert_eq!(authority.live_spawn_count(), 0);
        assert_eq!(authority.resolve(&token_copy), None);
    }

    #[test]
    fn distinct_spawns_get_distinct_tokens() {
        let authority = BridgeAuthority::new();
        let g1 = authority.issue(agent());
        let g2 = authority.issue(agent());
        assert!(g1.token() != g2.token());
        assert_eq!(authority.live_spawn_count(), 2);
    }

    #[test]
    fn same_agent_two_concurrent_spawns_resolve_independently() {
        // Same agent, two simultaneous bridge processes — each binds its own
        // entry; resolving either token returns the same agent_id.
        let authority = BridgeAuthority::new();
        let a = agent();
        let g1 = authority.issue(a);
        let g2 = authority.issue(a);
        assert_eq!(authority.resolve(g1.token()), Some(a));
        assert_eq!(authority.resolve(g2.token()), Some(a));
        assert_eq!(authority.live_spawn_count(), 2);
        drop(g1);
        assert_eq!(authority.live_spawn_count(), 1);
        assert_eq!(authority.resolve(g2.token()), Some(a));
    }

    #[test]
    fn unknown_token_resolves_to_none() {
        let authority = BridgeAuthority::new();
        let stranger = Token::generate();
        assert_eq!(authority.resolve(&stranger), None);
    }

    #[test]
    fn guard_fingerprint_matches_token_fingerprint() {
        let authority = BridgeAuthority::new();
        let guard = authority.issue(agent());
        assert_eq!(guard.fingerprint(), guard.token().fingerprint());
    }

    #[test]
    fn authority_implements_token_issuer_via_arc_dyn() {
        // Confirm the abstraction the driver will hold actually works:
        // construct `Arc<dyn TokenIssuer>` and exercise issue/revoke
        // through it without naming `BridgeAuthority`.
        let authority = BridgeAuthority::new();
        let issuer: Arc<dyn TokenIssuer> = authority.clone();
        let a = agent();
        {
            let guard = issuer.issue(a);
            assert_eq!(authority.resolve(guard.token()), Some(a));
            assert_eq!(authority.live_spawn_count(), 1);
        }
        assert_eq!(authority.live_spawn_count(), 0);
    }
}
