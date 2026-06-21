//! Per-spawn auth primitives for the MCP bridge IPC handshake.
//!
//! The [`Token`] type carries 32 bytes of CSPRNG-generated secret material
//! used to authenticate a bridge subprocess to the daemon. Tokens are minted
//! by the daemon at CC-spawn time, handed to the subprocess via a 0600
//! per-spawn MCP config file, and presented back on the IPC Hello.
//!
//! Security properties:
//! - **CSPRNG-sourced** (`OsRng`), 256 bits — infeasible to guess.
//! - **Constant-time equality** via [`subtle::ConstantTimeEq`] — no timing
//!   oracle on token comparison.
//! - **Zeroized on drop** via [`zeroize::ZeroizeOnDrop`] — secret material
//!   does not linger in freed memory.
//! - **No `Debug`/`Display`** — prevents accidental logging of the secret.
//!   Use [`Token::fingerprint`] for log correlation (first 32 bits only).
//!
//! Identity is *resolved*, not *asserted*: the daemon looks up the agent_id
//! bound to a presented token in its spawn table. The token is the only
//! trusted claim; any `agent_id` env var or wire-asserted string is for
//! diagnostics.
//!
//! See `projects/openfang-fork/plans/bridge-tool-surface-v2-plan.md` for
//! the broader design.

use rand::RngCore;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Length of the token in bytes (256 bits of CSPRNG material).
pub const TOKEN_LEN: usize = 32;

/// Length of the hex-encoded token on the wire.
pub const TOKEN_HEX_LEN: usize = TOKEN_LEN * 2;

/// A per-spawn authentication token for the bridge IPC handshake.
///
/// 32 bytes of CSPRNG material. Constant-time equality. Zeroized on drop.
/// Hex-encoded only at the wire boundary; in-process always handles the
/// raw byte array.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Token([u8; TOKEN_LEN]);

impl Token {
    /// Generate a fresh token from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_LEN];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Encode for transport. The returned `String` is *not* zeroized —
    /// callers must treat it as secret until it reaches its destination
    /// (env var of a child process, IPC frame, etc.).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from wire form. Rejects malformed or wrong-length input.
    pub fn from_hex(s: &str) -> Result<Self, TokenParseError> {
        let bytes = hex::decode(s).map_err(|_| TokenParseError::Malformed)?;
        let arr: [u8; TOKEN_LEN] = bytes.try_into().map_err(|_| TokenParseError::WrongLength)?;
        Ok(Self(arr))
    }

    /// Short fingerprint for log correlation. First 32 bits only —
    /// safe to log, useless to an attacker.
    pub fn fingerprint(&self) -> String {
        hex::encode(&self.0[..4])
    }

    /// Construct from raw bytes. For tests and trusted internal callers
    /// only — production paths should use [`Token::generate`] or
    /// [`Token::from_hex`].
    #[doc(hidden)]
    pub fn from_bytes(bytes: [u8; TOKEN_LEN]) -> Self {
        Self(bytes)
    }
}

// Constant-time equality. Required for any comparison of the secret.
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}
impl Eq for Token {}

// Hash over the full 32 bytes for use as a HashMap key. HashMap probing
// itself is not constant-time, but keys are uniformly random 256-bit
// values: an attacker cannot craft collisions, and slot-level equality
// goes through `ct_eq` above. Acceptable for the spawn-table use case.
impl std::hash::Hash for Token {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Errors decoding a [`Token`] from its hex wire form.
#[derive(Debug, thiserror::Error)]
pub enum TokenParseError {
    #[error("token is not valid hex")]
    Malformed,
    #[error("token must decode to exactly 32 bytes")]
    WrongLength,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_unique_tokens() {
        let a = Token::generate();
        let b = Token::generate();
        // Vanishingly unlikely to collide; if this ever flakes, fix the CSPRNG.
        // Note: Token deliberately has no Debug impl, so we use `!=` rather than assert_ne!.
        assert!(a != b, "two generated tokens collided");
    }

    #[test]
    fn hex_round_trip() {
        let t = Token::generate();
        let s = t.to_hex();
        assert_eq!(s.len(), TOKEN_HEX_LEN);
        let parsed = Token::from_hex(&s).expect("round-trip should succeed");
        assert!(t == parsed, "hex round-trip changed the token");
    }

    #[test]
    fn from_hex_rejects_malformed() {
        assert!(matches!(
            Token::from_hex("not hex at all!!"),
            Err(TokenParseError::Malformed)
        ));
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        // Valid hex, wrong byte length.
        let short = "deadbeef";
        assert!(matches!(
            Token::from_hex(short),
            Err(TokenParseError::WrongLength)
        ));
        let long = "ab".repeat(64); // 64 bytes
        assert!(matches!(
            Token::from_hex(&long),
            Err(TokenParseError::WrongLength)
        ));
    }

    #[test]
    fn equality_is_reflexive_and_symmetric() {
        let t = Token::generate();
        let clone = t.clone();
        assert!(t == clone);
        assert!(clone == t);
    }

    #[test]
    fn inequality_for_distinct_bytes() {
        let a = Token::from_bytes([0u8; TOKEN_LEN]);
        let mut b_bytes = [0u8; TOKEN_LEN];
        b_bytes[31] = 1; // differ only in the last byte
        let b = Token::from_bytes(b_bytes);
        assert!(a != b);
    }

    #[test]
    fn fingerprint_is_eight_hex_chars() {
        let t = Token::from_bytes([0xab; TOKEN_LEN]);
        assert_eq!(t.fingerprint(), "abababab");
    }

    #[test]
    fn token_is_usable_as_hashmap_key() {
        use std::collections::HashMap;
        let t = Token::generate();
        let mut map: HashMap<Token, &str> = HashMap::new();
        map.insert(t.clone(), "agent-1");
        assert_eq!(map.get(&t), Some(&"agent-1"));
    }
}
