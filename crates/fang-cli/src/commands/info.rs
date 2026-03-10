use anyhow::Result;

pub async fn run(package_id: &str) -> Result<()> {
    let registry = std::env::var("FANGHUB_REGISTRY_URL")
        .unwrap_or_else(|_| "https://fanghub.paradiseai.io".to_string());

    let client = reqwest::Client::new();
    let pkg: serde_json::Value = client
        .get(format!("{}/packages/{}", registry, package_id))
        .send()
        .await?
        .json()
        .await?;

    println!("Package: {} ({})", pkg["name"].as_str().unwrap_or(""), package_id);
    println!("Owner:   @{}", pkg["owner"].as_str().unwrap_or(""));
    println!("Latest:  v{}", pkg["latest_version"].as_str().unwrap_or("—"));
    println!("Installs: {}", pkg["install_count"].as_u64().unwrap_or(0));
    println!("Category: {}", pkg["category"].as_str().unwrap_or(""));
    println!("Description: {}", pkg["description"].as_str().unwrap_or(""));

    if let Some(tags) = pkg["tags"].as_array() {
        let tag_list: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
        if !tag_list.is_empty() {
            println!("Tags: {}", tag_list.join(", "));
        }
    }

    // List versions
    let versions: serde_json::Value = client
        .get(format!("{}/packages/{}/versions", registry, package_id))
        .send()
        .await?
        .json()
        .await?;

    if let Some(vers) = versions.as_array() {
        println!("\nVersions ({}):", vers.len());
        for v in vers.iter().take(5) {
            println!(
                "  v{}  published {}  [{} installs]",
                v["version"].as_str().unwrap_or(""),
                v["published_at"].as_str().unwrap_or(""),
                v["install_count"].as_u64().unwrap_or(0)
            );
        }
    }
    Ok(())
}
