use fanghub_registry::server::{RegistryConfig, RegistryServer};
use leptos_config::Env;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::from_default_env().add_directive("fanghub_registry=info".parse()?))
        .init();

    // Load config from environment variables (with sensible defaults)
    let leptos_env = match std::env::var("LEPTOS_ENV").as_deref() {
        Ok("PROD") | Ok("prod") | Ok("production") => Env::PROD,
        _ => Env::DEV,
    };

    let config = RegistryConfig {
        db_url: std::env::var("FANGHUB_DB_URL")
            .unwrap_or_else(|_| "surrealkv://./data/fanghub.db".to_string()),
        bind_addr: std::env::var("FANGHUB_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:4300".to_string()),
        jwt_secret: std::env::var("FANGHUB_JWT_SECRET")
            .unwrap_or_else(|_| "fanghub-dev-secret-change-in-production!".to_string())
            .into_bytes(),
        base_url: std::env::var("FANGHUB_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:4300".to_string()),
        site_root: std::env::var("FANGHUB_SITE_ROOT")
            .unwrap_or_else(|_| "site".to_string()),
        leptos_env,
    };

    RegistryServer::new(config).run().await
}
