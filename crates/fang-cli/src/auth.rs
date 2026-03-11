//! Authentication helpers for the fang CLI.
use crate::config::{load_config, AuthConfig};
use anyhow::Result;

/// Load the stored auth config, or bail with a helpful message.
pub fn require_auth() -> Result<AuthConfig> {
    let config = load_config()?;
    config
        .auth
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Run `fang login` first."))
}
