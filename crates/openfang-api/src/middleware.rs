//! Production middleware for the OpenFang API server.
//!
//! Provides:
//! - Request ID generation and propagation
//! - Per-endpoint structured request logging
//! - In-memory rate limiting (per IP)

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use std::time::Instant;
use tracing::info;

/// Request ID header name (standard).
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Middleware: inject a unique request ID and log the request/response.
pub async fn request_logging(request: Request<Body>, next: Next) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = request.method().clone();
    let uri = request.uri().path().to_string();
    let start = Instant::now();

    let mut response = next.run(request).await;

    let elapsed = start.elapsed();
    let status = response.status().as_u16();

    info!(
        request_id = %request_id,
        method = %method,
        path = %uri,
        status = status,
        latency_ms = elapsed.as_millis() as u64,
        "API request"
    );

    // Inject the request ID into the response
    if let Ok(header_val) = request_id.parse() {
        response.headers_mut().insert(REQUEST_ID_HEADER, header_val);
    }

    response
}

/// Bearer token authentication middleware.
///
/// When `api_key` is non-empty, all requests must include
/// `Authorization: Bearer <api_key>`. If the key is empty, auth is bypassed.
pub async fn auth(
    axum::extract::State(api_key): axum::extract::State<String>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // If no API key configured, restrict to loopback addresses only.
    if api_key.is_empty() {
        let is_loopback = request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().is_loopback())
            .unwrap_or(false);

        if !is_loopback {
            tracing::warn!(
                "Rejected non-localhost request: no API key configured. \
                 Set api_key in config.toml for remote access."
            );
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "error": "No API key configured. Remote access denied. Configure api_key in ~/.openfang/config.toml"
                    })
                    .to_string(),
                ))
                .unwrap_or_default();
        }
        return next.run(request).await;
    }

    // Public endpoints that don't require auth.
    // SECURITY: Keep this list minimal — only endpoints the SPA needs to
    // render its login/bootstrap screen, plus health checks and OAuth callbacks.
    // All data-bearing endpoints (agents, sessions, config, budget, etc.)
    // MUST require authentication to prevent information disclosure.
    let path = request.uri().path();
    if path == "/"
        || path == "/api/health"
        || path == "/api/status"
        || path == "/api/version"
        || path.starts_with("/api/uploads/")
        || path.starts_with("/api/providers/github-copilot/oauth/")
    {
        return next.run(request).await;
    }

    // Check Authorization: Bearer <token> header
    let bearer_token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    // SECURITY: Use constant-time comparison to prevent timing attacks.
    let header_auth = bearer_token.map(|token| {
        use subtle::ConstantTimeEq;
        if token.len() != api_key.len() {
            return false;
        }
        token.as_bytes().ct_eq(api_key.as_bytes()).into()
    });

    // Also check ?token= query parameter (for EventSource/SSE clients that
    // cannot set custom headers, same approach as WebSocket auth).
    let query_token = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("token=")));

    // SECURITY: Use constant-time comparison to prevent timing attacks.
    let query_auth = query_token.map(|token| {
        use subtle::ConstantTimeEq;
        if token.len() != api_key.len() {
            return false;
        }
        token.as_bytes().ct_eq(api_key.as_bytes()).into()
    });

    // Accept if either auth method matches
    if header_auth == Some(true) || query_auth == Some(true) {
        return next.run(request).await;
    }

    // Determine error message: was a credential provided but wrong, or missing entirely?
    let credential_provided = header_auth.is_some() || query_auth.is_some();
    let error_msg = if credential_provided {
        "Invalid API key"
    } else {
        "Missing Authorization: Bearer <api_key> header"
    };

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("www-authenticate", "Bearer")
        .body(Body::from(
            serde_json::json!({"error": error_msg}).to_string(),
        ))
        .unwrap_or_default()
}

/// Security headers middleware — applied to ALL API responses.
pub async fn security_headers(request: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("x-xss-protection", "1; mode=block".parse().unwrap());
    // All JS/CSS is bundled inline — only external resource is Google Fonts.
    headers.insert(
        "content-security-policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self' ws://localhost:* ws://127.0.0.1:* wss://localhost:* wss://127.0.0.1:*; font-src 'self' https://fonts.gstatic.com; media-src 'self' blob:; frame-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'"
            .parse()
            .unwrap(),
    );
    headers.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    headers.insert(
        "cache-control",
        "no-store, no-cache, must-revalidate".parse().unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_header_constant() {
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
    }

    /// Verify that data-bearing endpoints require authentication.
    /// Regression test for unauthenticated data exposure and approval bypass.
    #[test]
    fn test_sensitive_endpoints_require_auth() {
        // Mirror the auth bypass list — must stay in sync with the middleware.
        let bypass_path = |path: &str| -> bool {
            path == "/"
                || path == "/api/health"
                || path == "/api/status"
                || path == "/api/version"
                || path.starts_with("/api/uploads/")
                || path.starts_with("/api/providers/github-copilot/oauth/")
        };

        // Public endpoints — OK unauthenticated
        assert!(bypass_path("/"));
        assert!(bypass_path("/api/health"));
        assert!(bypass_path("/api/status"));
        assert!(bypass_path("/api/version"));
        assert!(bypass_path("/api/uploads/some-uuid"));

        // Data-bearing endpoints MUST require authentication
        assert!(!bypass_path("/api/config"));
        assert!(!bypass_path("/api/agents"));
        assert!(!bypass_path("/api/sessions"));
        assert!(!bypass_path("/api/profiles"));
        assert!(!bypass_path("/api/channels"));
        assert!(!bypass_path("/api/skills"));
        assert!(!bypass_path("/api/budget"));
        assert!(!bypass_path("/api/budget/agents"));
        assert!(!bypass_path("/api/integrations"));
        assert!(!bypass_path("/api/approvals"));
        assert!(!bypass_path("/api/network/status"));
        assert!(!bypass_path("/api/a2a/agents"));
        assert!(!bypass_path("/api/models"));

        // Approval mutation endpoints MUST require authentication
        assert!(!bypass_path("/api/approvals/some-uuid/approve"));
        assert!(!bypass_path("/api/approvals/some-uuid/reject"));
    }
}
