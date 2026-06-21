//! Typed errors for kernel-side tool implementations (ANAI-55 follow-up).
//!
//! Today the `KernelHandle` trait returns `Result<String, String>` for the
//! channel-send tool surface, which means every error is a free-form string
//! and every caller that wants to discriminate must do prefix-matching. That
//! is the contract this enum is on the path to replacing.
//!
//! Scope as of ANAI-55: only the recipient-resolution failure mode is
//! modeled, because that is the one the resolver produces and the runner
//! benefits most from being able to identify (e.g., to surface a structured
//! tool-call error to the agent rather than a stringly-typed one when the
//! trait boundary migrates).
//!
//! The trait boundary stays `Result<_, String>` for now; converting that is a
//! ripple through `KernelHandle`, the API bridge, the IPC bridge, and the
//! cron-delivery test doubles. This file is the foothold for that migration.
//!
//! Display impls are stable: the prefix tokens (e.g. `RECIPIENT_UNRESOLVED:`)
//! are part of the contract for any caller that round-trips through
//! `to_string()` and wants to recover the variant.

use openfang_channels::types::ResolutionError;
use std::fmt;

/// Errors a kernel tool implementation can surface to the runtime layer.
#[derive(Debug)]
pub enum ToolError {
    /// The channel adapter could not resolve the recipient string to a
    /// platform-native identity (ANAI-55). Carries the underlying
    /// `ResolutionError` so the runtime can choose its surface form.
    RecipientUnresolved(ResolutionError),

    /// Catch-all for anything not yet modeled. New variants should be
    /// introduced as call sites need to discriminate them.
    Other(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::RecipientUnresolved(e) => {
                // Stable prefix — see module docs.
                write!(f, "RECIPIENT_UNRESOLVED: {e}")
            }
            ToolError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ToolError::RecipientUnresolved(e) => Some(e),
            ToolError::Other(_) => None,
        }
    }
}

impl From<ResolutionError> for ToolError {
    fn from(e: ResolutionError) -> Self {
        ToolError::RecipientUnresolved(e)
    }
}

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        ToolError::Other(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipient_unresolved_display_has_stable_prefix() {
        let inner = ResolutionError::UnknownRecipient {
            recipient: "ghost".to_string(),
        };
        let err = ToolError::RecipientUnresolved(inner);
        let s = err.to_string();
        assert!(
            s.starts_with("RECIPIENT_UNRESOLVED: "),
            "display lost stable prefix: {s}"
        );
    }

    #[test]
    fn from_resolution_error_yields_recipient_unresolved() {
        let inner = ResolutionError::BareNameDmRefused {
            name: "benhoverter".to_string(),
        };
        let err: ToolError = inner.into();
        assert!(matches!(err, ToolError::RecipientUnresolved(_)));
    }

    #[test]
    fn from_string_yields_other() {
        let err: ToolError = "boom".to_string().into();
        assert!(matches!(err, ToolError::Other(ref s) if s == "boom"));
        assert_eq!(err.to_string(), "boom");
    }
}
