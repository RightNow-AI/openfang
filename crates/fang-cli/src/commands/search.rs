use anyhow::Result;

pub async fn run(query: &str, category: Option<String>) -> Result<()> {
    let registry = std::env::var("FANGHUB_REGISTRY_URL")
        .unwrap_or_else(|_| "https://fanghub.paradiseai.io".to_string());

    let mut url = format!("{}/packages?q={}", registry, urlencoding::encode(query));
    if let Some(cat) = &category {
        url.push_str(&format!("&category={}", urlencoding::encode(cat)));
    }

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;

    let total = resp["total"].as_u64().unwrap_or(0);
    println!("Found {} package(s) matching '{}':\n", total, query);

    if let Some(results) = resp["results"].as_array() {
        for pkg in results {
            let id = pkg["package_id"].as_str().unwrap_or("");
            let name = pkg["name"].as_str().unwrap_or("");
            let version = pkg["latest_version"].as_str().unwrap_or("—");
            let installs = pkg["install_count"].as_u64().unwrap_or(0);
            let desc = pkg["description"].as_str().unwrap_or("");
            println!("  {} ({}) v{}  [{} installs]", name, id, version, installs);
            println!("    {}", desc);
            println!("    Install: fang install {}\n", id);
        }
    }
    Ok(())
}
