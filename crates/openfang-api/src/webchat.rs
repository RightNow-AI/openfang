//! Embedded WebChat UI served as static HTML.
//!
//! The production dashboard is assembled at compile time from separate
//! HTML/CSS/JS files under `static/` using `include_str!()`. This keeps
//! single-binary deployment while allowing organized source files.
//!
//! Features:
//! - Alpine.js SPA with hash-based routing (10 panels)
//! - Dark/light theme toggle with system preference detection
//! - Responsive layout with collapsible sidebar
//! - Markdown rendering + syntax highlighting (bundled locally)
//! - WebSocket real-time chat with HTTP fallback
//! - Agent management, workflows, memory browser, audit log, and more

use axum::http::header;
use axum::response::IntoResponse;

/// Compile-time ETag based on the crate version.
const ETAG: &str = concat!("\"openfang-", env!("CARGO_PKG_VERSION"), "\"");

/// Embedded logo PNG for single-binary deployment.
const LOGO_PNG: &[u8] = include_bytes!("../static/logo.png");

/// Embedded favicon ICO for browser tabs.
const FAVICON_ICO: &[u8] = include_bytes!("../static/favicon.ico");

/// GET /logo.png — Serve the OpenFang logo.
pub async fn logo_png() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        LOGO_PNG,
    )
}

/// GET /favicon.ico — Serve the OpenFang favicon.
pub async fn favicon_ico() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/x-icon"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        FAVICON_ICO,
    )
}

/// Embedded PWA manifest for installable web app support.
const MANIFEST_JSON: &str = include_str!("../static/manifest.json");

/// Embedded service worker for PWA support.
const SW_JS: &str = include_str!("../static/sw.js");

/// GET /manifest.json — Serve the PWA web app manifest.
pub async fn manifest_json() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        MANIFEST_JSON,
    )
}

/// GET /sw.js — Serve the PWA service worker.
pub async fn sw_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/javascript"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        SW_JS,
    )
}

/// GET / — Serve the OpenFang Dashboard single-page application.
///
/// **Primary frontend:** `sdk/javascript/examples/nextjs-app-router/` (port 3002)
///
/// This endpoint now redirects browsers to Next.js by default.
/// The legacy Alpine SPA is preserved for emergency fallback only.
///
/// ## Environment variables
/// - `OPENFANG_DASHBOARD_URL`: redirect target (default: `http://localhost:3002`)
/// - `OPENFANG_LEGACY_UI=1`: serve the legacy Alpine SPA instead of redirecting
pub async fn webchat_page() -> axum::response::Response {
    use axum::response::IntoResponse as _;

    // Emergency fallback: set OPENFANG_LEGACY_UI=1 to bypass redirect and serve Alpine.
    if std::env::var("OPENFANG_LEGACY_UI").as_deref() == Ok("1") {
        return (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::ETAG, ETAG),
                (header::CACHE_CONTROL, "no-store"),
            ],
            WEBCHAT_HTML,
        )
        .into_response();
    }

    // Redirect to the primary Next.js frontend.
    let url = std::env::var("OPENFANG_DASHBOARD_URL")
        .unwrap_or_else(|_| "http://localhost:3002".to_string());
    axum::response::Redirect::temporary(&url).into_response()
}

/// LEGACY: Alpine.js SPA — pending migration to Next.js (sdk/javascript/examples/nextjs-app-router/).
///
/// Assembled at compile time from organized static files.
/// All vendor libraries (Alpine.js, marked.js, highlight.js) are bundled
/// locally — no CDN dependency. Alpine.js is included LAST because it
/// immediately processes x-data directives and fires alpine:init on load.
///
/// DO NOT apply CSS/JS patches here without a confirmed live repro.
/// Changes require a fresh `cargo build -p openfang-cli` to deploy.
/// Verify with: `curl -s http://127.0.0.1:50051/ | Select-String "your-pattern"`
const WEBCHAT_HTML: &str = concat!(
    include_str!("../static/index_head.html"),
    "<style>\n",
    include_str!("../static/css/theme.css"),
    "\n",
    include_str!("../static/css/layout.css"),
    "\n",
    include_str!("../static/css/components.css"),
    "\n",
    include_str!("../static/vendor/github-dark.min.css"),
    "\n</style>\n",
    include_str!("../static/index_body.html"),
    // Vendor libs: marked + highlight first (used by app.js)
    "<script>\n",
    include_str!("../static/vendor/marked.min.js"),
    "\n</script>\n",
    "<script>\n",
    include_str!("../static/vendor/highlight.min.js"),
    "\n</script>\n",
    // App code
    "<script>\n",
    include_str!("../static/js/connection.js"),
    "\n",
    include_str!("../static/js/auth-oauth.js"),
    "\n",
    include_str!("../static/js/api.js"),
    "\n",
    include_str!("../static/js/fallback-policy.js"),
    "\n",
    include_str!("../static/js/ollama-adapter.js"),
    "\n",
    include_str!("../static/js/local-orchestration.js"),
    "\n",
    include_str!("../static/js/app.js"),
    "\n",
    include_str!("../static/js/pages/today.js"),
    "\n",
    include_str!("../static/js/pages/inbox.js"),
    "\n",
    include_str!("../static/js/pages/agent-catalog.js"),
    "\n",
    include_str!("../static/js/pages/overview.js"),
    "\n",
    include_str!("../static/js/pages/chat.js"),
    "\n",
    include_str!("../static/js/pages/agents.js"),
    "\n",
    include_str!("../static/js/pages/workflows.js"),
    "\n",
    include_str!("../static/js/pages/workflow-builder.js"),
    "\n",
    include_str!("../static/js/pages/channels.js"),
    "\n",
    include_str!("../static/js/pages/skills.js"),
    "\n",
    include_str!("../static/js/pages/hands.js"),
    "\n",
    include_str!("../static/js/pages/scheduler.js"),
    "\n",
    include_str!("../static/js/pages/settings.js"),
    "\n",
    include_str!("../static/js/pages/usage.js"),
    "\n",
    include_str!("../static/js/pages/sessions.js"),
    "\n",
    include_str!("../static/js/pages/logs.js"),
    "\n",
    include_str!("../static/js/pages/wizard.js"),
    "\n",
    include_str!("../static/js/pages/approvals.js"),
    "\n",
    include_str!("../static/js/pages/comms.js"),
    "\n",
    include_str!("../static/js/pages/runtime.js"),
    "\n</script>\n",
    // Alpine.js MUST be last — it processes x-data and fires alpine:init
    "<script>\n",
    include_str!("../static/vendor/alpine.min.js"),
    "\n</script>\n",
    "</body></html>"
);
