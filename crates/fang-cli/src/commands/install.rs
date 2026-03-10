use anyhow::{Context, Result};

pub async fn run(package: &str, api_url: &str) -> Result<()> {
    // Parse "package_id@version" or just "package_id"
    let (package_id, version) = if let Some((id, ver)) = package.split_once('@') {
        (id, Some(ver))
    } else {
        (package, None)
    };

    let registry = std::env::var("FANGHUB_REGISTRY_URL")
        .unwrap_or_else(|_| "https://fanghub.paradiseai.io".to_string());

    // Resolve version if not specified
    let resolved_version = if let Some(v) = version {
        v.to_string()
    } else {
        let client = reqwest::Client::new();
        let pkg: serde_json::Value = client
            .get(format!("{}/packages/{}", registry, package_id))
            .send()
            .await?
            .json()
            .await?;
        pkg["latest_version"]
            .as_str()
            .context("Package has no published versions")?
            .to_string()
    };

    println!("Installing {} v{} ...", package_id, resolved_version);

    // Record the install in the registry
    let client = reqwest::Client::new();
    let _ = client
        .post(format!(
            "{}/packages/{}/versions/{}/install",
            registry, package_id, resolved_version
        ))
        .send()
        .await;

    // Call the local OpenFang kernel to install the Hand
    let install_resp = client
        .post(format!("{}/api/hands/install", api_url))
        .json(&serde_json::json!({
            "package_id": package_id,
            "version": resolved_version,
            "registry_url": registry,
        }))
        .send()
        .await
        .context("Failed to connect to OpenFang kernel")?;

    if !install_resp.status().is_success() {
        let status = install_resp.status();
        let body: serde_json::Value = install_resp.json().await.unwrap_or_default();
        anyhow::bail!(
            "Install failed ({}): {}",
            status,
            body["error"].as_str().unwrap_or("unknown error")
        );
    }

    println!("✓ Installed {} v{}", package_id, resolved_version);
    println!("  Activate with: fang activate {} (or via the OpenFang UI)", package_id);
    Ok(())
}
