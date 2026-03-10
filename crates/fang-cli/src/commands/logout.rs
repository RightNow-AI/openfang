use crate::config::{load_config, save_config};
use anyhow::Result;

pub async fn run() -> Result<()> {
    let mut config = load_config()?;
    if config.auth.is_none() {
        println!("Not currently logged in.");
        return Ok(());
    }
    let login = config.auth.as_ref().map(|a| a.github_login.clone()).unwrap_or_default();
    config.auth = None;
    save_config(&config)?;
    println!("✓ Logged out (@{})", login);
    Ok(())
}
