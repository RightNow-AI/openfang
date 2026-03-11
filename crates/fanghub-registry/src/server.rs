use crate::{
    db::init_db,
    routes::{
        get_me, get_package, get_stats, get_user_profile, get_version, health, list_versions,
        publish_version, record_install, search_packages, AppState,
    },
    store::RegistryStore,
    ui::{shell, FangHubApp},
};
use axum::{
    routing::{get, post},
    Router,
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use leptos_config::Env;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};

/// Configuration for the FangHub registry server.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// SurrealDB connection string (e.g. "mem://" or "surrealkv://./data/fanghub.db")
    pub db_url: String,
    /// TCP address to bind (e.g. "0.0.0.0:4300")
    pub bind_addr: String,
    /// JWT signing secret (at least 32 bytes)
    pub jwt_secret: Vec<u8>,
    /// Public base URL for download links
    pub base_url: String,
    /// Leptos site root (directory containing the WASM pkg/ output from Trunk)
    pub site_root: String,
    /// Leptos environment (Dev or Prod)
    pub leptos_env: Env,
}

impl RegistryConfig {
    /// Create a new configuration suitable for development/testing.
    /// NOTE: This generates a random JWT secret. For production, always
    /// provide an explicit secret via environment variable or secure config.
    pub fn development() -> Self {
        // Generate a random JWT secret for development to avoid hardcoded credentials
        let random_secret: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();

        Self {
            db_url: "mem://".to_string(),
            bind_addr: "0.0.0.0:4300".to_string(),
            jwt_secret: random_secret,
            base_url: "http://localhost:4300".to_string(),
            site_root: "site".to_string(),
            leptos_env: Env::DEV,
        }
    }

    /// Create configuration from environment variables.
    /// Panics if FANGHUB_JWT_SECRET is not set or is less than 32 bytes.
    pub fn from_env() -> Self {
        use std::env;

        let jwt_secret = env::var("FANGHUB_JWT_SECRET")
            .expect("FANGHUB_JWT_SECRET environment variable must be set in production")
            .into_bytes();

        if jwt_secret.len() < 32 {
            panic!("FANGHUB_JWT_SECRET must be at least 32 bytes long");
        }

        Self {
            db_url: env::var("FANGHUB_DB_URL").unwrap_or_else(|_| "surrealkv://./data/fanghub.db".to_string()),
            bind_addr: env::var("FANGHUB_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:4300".to_string()),
            jwt_secret,
            base_url: env::var("FANGHUB_BASE_URL").unwrap_or_else(|_| "http://localhost:4300".to_string()),
            site_root: env::var("FANGHUB_SITE_ROOT").unwrap_or_else(|_| "site".to_string()),
            leptos_env: match env::var("FANGHUB_ENV").as_deref() {
                Ok("production") | Ok("prod") => Env::PROD,
                _ => Env::DEV,
            },
        }
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::development()
    }
}

/// The FangHub registry server — serves both the REST API and the Leptos SSR UI.
pub struct RegistryServer {
    config: RegistryConfig,
}

impl RegistryServer {
    pub fn new(config: RegistryConfig) -> Self {
        Self { config }
    }

    /// Build and return the Axum router (useful for testing without binding).
    pub async fn build_router(&self) -> anyhow::Result<Router> {
        let db = init_db(&self.config.db_url).await?;
        let store = Arc::new(RegistryStore::new(db));

        // Configure Leptos options — the builder uses Arc<str> for string fields.
        let leptos_options = LeptosOptions::builder()
            .output_name(Arc::<str>::from("fanghub"))
            .site_root(Arc::<str>::from(self.config.site_root.as_str()))
            .site_pkg_dir(Arc::<str>::from("pkg"))
            .env(self.config.leptos_env.clone())
            .site_addr(self.config.bind_addr.parse::<std::net::SocketAddr>()?)
            .build();

        // AppState is Clone — use it directly as the Axum state (not Arc<AppState>)
        // so that LeptosOptions: FromRef<AppState> satisfies the LeptosRoutes bound.
        let state = AppState {
            store: (*store).clone(),
            jwt_secret: self.config.jwt_secret.clone(),
            base_url: self.config.base_url.clone(),
            leptos_options: leptos_options.clone(),
        };

        // Generate the Leptos route list from the FangHubApp component.
        let routes = generate_route_list(FangHubApp);

        let store_for_context = store.clone();

        let router = Router::new()
            // ── REST API routes (prefixed /api/) ──────────────────────────────
            .route("/api/health", get(health))
            .route("/api/stats", get(get_stats))
            .route("/api/packages", get(search_packages))
            .route("/api/packages/:package_id", get(get_package))
            .route(
                "/api/packages/:package_id/versions",
                get(list_versions).post(publish_version),
            )
            .route(
                "/api/packages/:package_id/versions/:version",
                get(get_version),
            )
            .route(
                "/api/packages/:package_id/versions/:version/install",
                post(record_install),
            )
            .route("/api/users/:login", get(get_user_profile))
            .route("/api/me", get(get_me))
            // ── Leptos SSR routes (auto-generated from FangHubApp router) ─────
            .leptos_routes_with_context(
                &state,
                routes,
                move || {
                    provide_context(store_for_context.clone());
                },
                {
                    let leptos_options = leptos_options.clone();
                    move || shell(leptos_options.clone())
                },
            )
            // ── Static file serving for WASM pkg/ output ─────────────────────
            .fallback_service(
                tower_http::services::ServeDir::new(&self.config.site_root)
                    .not_found_service(tower_http::services::ServeFile::new(format!(
                        "{}/index.html",
                        self.config.site_root
                    ))),
            )
            .with_state(state)
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(
                // CORS configuration: restrict to known origins in production
                CorsLayer::new()
                    .allow_origin(
                        if self.config.leptos_env == Env::PROD {
                            // In production, only allow same-origin requests
                            tower_http::cors::AllowOrigin::exact(
                                self.config.base_url.parse().unwrap_or_else(|_| "http://localhost:4300".parse().unwrap())
                            )
                        } else {
                            // In development, allow any origin
                            tower_http::cors::AllowOrigin::any()
                        }
                    )
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::PUT,
                        axum::http::Method::DELETE,
                        axum::http::Method::OPTIONS,
                    ])
                    .allow_headers([
                        axum::http::header::AUTHORIZATION,
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::ACCEPT,
                    ]),
            );

        Ok(router)
    }

    /// Start the server and listen for connections.
    pub async fn run(self) -> anyhow::Result<()> {
        let bind_addr = self.config.bind_addr.clone();
        let router = self.build_router().await?;

        tracing::info!(
            "FangHub registry (REST API + Leptos SSR UI) listening on {}",
            bind_addr
        );

        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        axum::serve(listener, router).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn test_server() -> Router {
        let config = RegistryConfig::default();
        RegistryServer::new(config).build_router().await.unwrap()
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = test_server().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_stats_endpoint() {
        let app = test_server().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_search_empty_registry() {
        let app = test_server().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/packages")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_get_nonexistent_package() {
        let app = test_server().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/packages/nonexistent-hand")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_unauthenticated_publish_fails() {
        let app = test_server().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/packages/my-hand/versions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should fail with 401 (no auth) or 400 (bad content-type) — not 500
        assert!(
            response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
}
