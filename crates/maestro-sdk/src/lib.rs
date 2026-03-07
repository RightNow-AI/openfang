//! # maestro-sdk
//!
//! External SDK for interacting with the Maestro-OpenFang platform.
//!
//! ## What Kore.ai Has
//!
//! - web-kore-sdk (JavaScript, 36 stars, most popular repo)
//! - BotKit (Node.js server-side framework)
//! - Native SDKs for Android, iOS, React Native
//! - amp-sdk (TypeScript, zero-dependency, OpenTelemetry-based)
//!
//! ## What OpenFang Has
//!
//! - Python SDK (`sdk/python/`) with basic agent interaction
//! - JavaScript SDK (`sdk/js/`) for web integration
//! - BUT: No Rust SDK for programmatic embedding
//!
//! ## What This Crate Provides
//!
//! A Rust SDK for embedding the Maestro-OpenFang platform into other
//! Rust applications. Provides typed HTTP client for the REST API.
//!
//! ## HONEST GAPS
//!
//! - This is a Rust-only SDK. Most consumers want JavaScript or Python.
//! - No WebSocket support for real-time streaming
//! - No authentication beyond API key
//! - No offline/local mode (requires running server)

/// SDK client for the Maestro-OpenFang platform.
#[allow(dead_code)]
pub struct MaestroClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl MaestroClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            http: reqwest::Client::new(),
        }
    }

    // TODO: Implement typed API methods:
    // - create_session() -> Session
    // - send_message(session_id, message) -> Response
    // - list_agents() -> Vec<AgentInfo>
    // - get_traces(session_id) -> Vec<Trace>
    // - install_skill(name, version) -> Result
}
