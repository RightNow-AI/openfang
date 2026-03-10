use crate::{auth::require_auth, manifest::load_manifest, packager::build_archive};
use anyhow::{Context, Result};

pub async fn run(registry: &str, notes: Option<String>) -> Result<()> {
    let auth = require_auth()?;
    let dir = std::env::current_dir()?;
    let manifest = load_manifest(&dir)?;
    let hand = &manifest.hand;

    println!("Publishing {} v{} to {} ...", hand.id, hand.version, registry);

    let manifest_content = std::fs::read_to_string(dir.join("HAND.toml"))?;
    let (archive_bytes, checksum) = build_archive(&dir)?;

    println!("  Archive size: {} bytes", archive_bytes.len());
    println!("  SHA-256:      {}", checksum);

    // Build multipart form
    let form = reqwest::multipart::Form::new()
        .text("manifest", manifest_content)
        .part(
            "archive",
            reqwest::multipart::Part::bytes(archive_bytes)
                .file_name(format!("{}-{}.tar.gz", hand.id, hand.version))
                .mime_str("application/gzip")?,
        );

    let form = if let Some(n) = notes {
        form.text("release_notes", n)
    } else {
        form
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/packages/{}/versions", registry, hand.id))
        .header("Authorization", format!("Bearer {}", auth.token))
        .multipart(form)
        .send()
        .await
        .context("Failed to connect to FangHub registry")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        anyhow::bail!(
            "Publish failed ({}): {}",
            status,
            body["error"].as_str().unwrap_or("unknown error")
        );
    }

    let result: serde_json::Value = resp.json().await?;
    println!("\n✓ Published successfully!");
    println!("  Package:  {}", result["package_id"].as_str().unwrap_or(""));
    println!("  Version:  {}", result["version"].as_str().unwrap_or(""));
    println!("  Download: {}", result["download_url"].as_str().unwrap_or(""));
    Ok(())
}
