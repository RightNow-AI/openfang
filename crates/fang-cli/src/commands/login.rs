use crate::config::{save_config, AuthConfig, FangConfig};
use anyhow::{bail, Result};

pub async fn run() -> Result<()> {
    println!("FangHub Login");
    println!("─────────────");
    println!("1. Go to https://fanghub.paradiseai.io/settings/tokens");
    println!("2. Generate a new API token");
    println!("3. Paste it below\n");

    print!("API Token: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut token = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(std::io::stdin()), &mut token)?;
    let token = token.trim().to_string();

    if token.is_empty() {
        bail!("No token provided");
    }

    // Validate token against the registry
    let registry_url = std::env::var("FANGHUB_REGISTRY_URL")
        .unwrap_or_else(|_| "https://fanghub.paradiseai.io".to_string());

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/me", registry_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("Invalid token — authentication failed ({})", resp.status());
    }

    let user: serde_json::Value = resp.json().await?;
    let login = user["github_login"].as_str().unwrap_or("unknown");

    let config = FangConfig {
        auth: Some(AuthConfig {
            github_login: login.to_string(),
            token,
            registry_url,
        }),
    };
    save_config(&config)?;

    println!("\n✓ Logged in as @{}", login);
    Ok(())
}
