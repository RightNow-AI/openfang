//! Bridge token issuance — runtime-side abstraction.
//!
//! This module defines the boundary the `claude-code` driver uses to obtain
//! a per-spawn bridge token without depending on the daemon crate. The
//! concrete authority (`openfang_api::bridge_auth::BridgeAuthority`) lives
//! in `openfang-api`, which already depends on `openfang-runtime`. Routing
//! the abstraction through this crate preserves the dep direction:
//!
//! ```text
//! openfang-api  ───depends on──▶  openfang-runtime  ───depends on──▶  openfang-types
//!     (impl TokenIssuer)               (trait + SpawnGuard)             (Token)
//! ```
//!
//! ## Lifetime model
//!
//! [`TokenIssuer::issue`] returns a [`SpawnGuard`]. The guard carries the
//! token plus an `Arc<dyn TokenIssuer>` back-reference to its issuer; on
//! drop it calls [`TokenIssuer::revoke`], which evicts the entry from the
//! issuer's spawn table. The `Token`'s `ZeroizeOnDrop` impl then clears its
//! bytes during the guard's own drop.
//!
//! Wire the guard into `BridgeMcpConfig` so its `Drop` runs exactly when
//! the `claude` subprocess terminates.
//!
//! ## Identity is resolved, not asserted
//!
//! Bridge IPC requests carry only the token. The daemon never trusts an
//! `agent_id` claim from the bridge — it looks up the agent_id keyed by
//! token. This module defines the trait surface; resolution lives on the
//! concrete authority.

use std::sync::Arc;

use openfang_types::agent::AgentId;
use openfang_types::bridge_auth::Token;

/// Issuer of per-spawn bridge tokens.
///
/// Held by the driver as `Arc<dyn TokenIssuer>` so the runtime crate does
/// not have to depend on `openfang-api`. The concrete implementation
/// (`openfang_api::bridge_auth::BridgeAuthority`) owns the spawn table and
/// the resolution path.
///
/// The trait is intentionally minimal — issuance + revocation only.
/// Resolution (`token → agent_id`) is daemon-internal and lives on the
/// concrete authority, since runtime callers never need it.
pub trait TokenIssuer: Send + Sync + 'static {
    /// Reserve a fresh token bound to `agent_id`. The returned [`SpawnGuard`]
    /// evicts the spawn-table entry on drop.
    fn issue(&self, agent_id: AgentId) -> SpawnGuard;

    /// Evict a previously-issued token from the spawn table.
    ///
    /// Called by [`SpawnGuard`]'s `Drop` impl. Not intended for direct use —
    /// always go through the guard so eviction and `Token` zeroization
    /// happen together.
    #[doc(hidden)]
    fn revoke(&self, token: &Token);
}

/// RAII handle for a reserved spawn slot. Drop evicts the entry from the
/// issuer's spawn table and zeroizes the held [`Token`].
///
/// The guard exposes the token by reference for emission to the subprocess
/// (e.g. into `BridgeMcpConfig`); it deliberately does not implement
/// `Clone` — there is exactly one live guard per spawn.
pub struct SpawnGuard {
    issuer: Arc<dyn TokenIssuer>,
    token: Token,
}

impl SpawnGuard {
    /// Construct a guard. Called by [`TokenIssuer`] implementations after
    /// they register a fresh token in their spawn table.
    pub fn new(issuer: Arc<dyn TokenIssuer>, token: Token) -> Self {
        Self { issuer, token }
    }

    /// Access the spawn's token for transport to the bridge subprocess
    /// (typically via env var). Callers should hex-encode immediately and
    /// drop the reference.
    pub fn token(&self) -> &Token {
        &self.token
    }

    /// Diagnostic identifier safe to log (first 8 hex chars of the token).
    pub fn fingerprint(&self) -> String {
        self.token.fingerprint()
    }
}

impl std::fmt::Debug for SpawnGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnGuard")
            .field("fingerprint", &self.fingerprint())
            .finish()
    }
}

impl Drop for SpawnGuard {
    fn drop(&mut self) {
        self.issuer.revoke(&self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Minimal in-memory issuer used to exercise the trait surface without
    /// depending on `openfang-api`. The real impl is `BridgeAuthority`.
    #[derive(Default)]
    struct StubIssuer {
        inner: Mutex<StubInner>,
    }

    #[derive(Default)]
    struct StubInner {
        live: Vec<(Token, AgentId)>,
        weak: Option<std::sync::Weak<StubIssuer>>,
    }

    impl StubIssuer {
        fn new() -> Arc<Self> {
            let arc = Arc::new(Self::default());
            arc.inner.lock().unwrap().weak = Some(Arc::downgrade(&arc));
            arc
        }
    }

    impl TokenIssuer for StubIssuer {
        fn issue(&self, agent_id: AgentId) -> SpawnGuard {
            let token = Token::generate();
            let mut inner = self.inner.lock().unwrap();
            inner.live.push((token.clone(), agent_id));
            let me: Arc<dyn TokenIssuer> = inner
                .weak
                .as_ref()
                .expect("weak self stash")
                .upgrade()
                .expect("issuer still alive");
            SpawnGuard::new(me, token)
        }

        fn revoke(&self, token: &Token) {
            let mut inner = self.inner.lock().unwrap();
            inner.live.retain(|(t, _)| t != token);
        }
    }

    #[test]
    fn issue_then_drop_evicts() {
        let issuer = StubIssuer::new();
        {
            let _g = issuer.issue(AgentId::new());
            assert_eq!(issuer.inner.lock().unwrap().live.len(), 1);
        }
        assert_eq!(issuer.inner.lock().unwrap().live.len(), 0);
    }

    #[test]
    fn guard_exposes_fingerprint_and_token() {
        let issuer = StubIssuer::new();
        let g = issuer.issue(AgentId::new());
        assert_eq!(g.fingerprint(), g.token().fingerprint());
        assert_eq!(g.token().to_hex().len(), 64);
    }
}
