//! Production middleware for the OpenFang API server.
//!
//! Provides:
//! - Request ID generation and propagation
//! - Per-endpoint structured request logging
//! - In-memory rate limiting (per IP)

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use axum::middleware::Next;
use std::net::IpAddr;
use std::time::Instant;
use tracing::{info, Instrument};

/// Request ID header name (standard).
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Request-scoped correlation ID stored in request extensions.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Middleware: inject a unique request ID and log the request/response.
pub async fn request_logging(mut request: Request<Body>, next: Next) -> Response<Body> {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let method = request.method().clone();
    let uri = request.uri().path().to_string();
    let start = Instant::now();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %uri
    );

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let mut response = next.run(request).instrument(span).await;

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

/// Authentication state passed to the auth middleware.
#[derive(Clone)]
pub struct AuthState {
    pub api_key: String,
    pub auth_enabled: bool,
    pub session_secret: String,
}

/// Structured auth failure so HTTP middleware and WebSocket upgrades can
/// enforce the same rules without re-implementing the logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    LoopbackOnly,
    MissingCredentials,
    InvalidCredentials,
}

fn normalize_forwarded_ip(raw: &str) -> Option<IpAddr> {
    let mut candidate = raw.trim().trim_matches('"');
    if candidate.is_empty() || candidate.eq_ignore_ascii_case("unknown") {
        return None;
    }

    if let Some(rest) = candidate.strip_prefix("for=") {
        candidate = rest.trim().trim_matches('"');
    }

    if let Some(rest) = candidate.strip_prefix('[') {
        if let Some((host, _port)) = rest.split_once("]:") {
            candidate = host;
        } else if let Some(host) = rest.strip_suffix(']') {
            candidate = host;
        }
    }

    if let Ok(ip) = candidate.parse::<IpAddr>() {
        return Some(ip);
    }

    if let Some((host, port)) = candidate.rsplit_once(':') {
        if !host.contains(':') && !port.is_empty() {
            return host.parse::<IpAddr>().ok();
        }
    }

    None
}

fn forwarded_header_indicates_remote_client(headers: &HeaderMap) -> bool {
    let candidate_values = [
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(|value| value.split(',').map(str::trim).collect::<Vec<_>>()),
        headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(|value| vec![value.trim()]),
        headers
            .get("forwarded")
            .and_then(|v| v.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .flat_map(|part| part.split(','))
                    .map(str::trim)
                    .filter(|part| part.starts_with("for="))
                    .collect::<Vec<_>>()
            }),
    ];

    candidate_values.into_iter().flatten().flatten().any(|raw| {
        normalize_forwarded_ip(raw)
            .map(|ip| !ip.is_loopback())
            .unwrap_or(true)
    })
}

pub(crate) fn is_effective_loopback(peer_ip: IpAddr, headers: &HeaderMap) -> bool {
    peer_ip.is_loopback() && !forwarded_header_indicates_remote_client(headers)
}

fn request_is_effective_loopback(request: &Request<Body>) -> bool {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| is_effective_loopback(ci.0.ip(), request.headers()))
        .unwrap_or(false)
}

fn query_token_allowed(path: &str, method: &axum::http::Method) -> bool {
    *method == axum::http::Method::GET
        && (path == "/api/logs/stream"
            || path == "/api/comms/events/stream"
            || (path.starts_with("/api/agents/") && path.ends_with("/ws")))
}

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                c.trim()
                    .strip_prefix("openfang_session=")
                    .map(|v| v.to_string())
            })
        })
}

fn extract_query_token(query: Option<&str>) -> Option<String> {
    query.and_then(|value| {
        url::form_urlencoded::parse(value.as_bytes()).find_map(|(key, token)| {
            if key == "token" {
                Some(token.into_owned())
            } else {
                None
            }
        })
    })
}

/// Apply the non-public auth rules to an arbitrary request shape.
pub fn authorize_request_parts(
    auth_state: &AuthState,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    query: Option<&str>,
    is_loopback: bool,
) -> Result<(), AuthFailure> {
    // If no API key is configured and session auth is disabled, stay in
    // localhost-only mode: unauthenticated requests from loopback are allowed,
    // but remote requests to protected endpoints are rejected.
    let api_key_trimmed = auth_state.api_key.trim();
    if api_key_trimmed.is_empty() && !auth_state.auth_enabled {
        if is_loopback {
            return Ok(());
        }

        return Err(AuthFailure::LoopbackOnly);
    }

    let bearer_token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let api_token = bearer_token.or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()));

    let header_auth = if api_key_trimmed.is_empty() {
        None
    } else {
        api_token.map(|token| {
            use subtle::ConstantTimeEq;
            if token.len() != api_key_trimmed.len() {
                return false;
            }
            token.as_bytes().ct_eq(api_key_trimmed.as_bytes()).into()
        })
    };

    let query_token = if api_key_trimmed.is_empty() || !query_token_allowed(path, method) {
        None
    } else {
        extract_query_token(query)
    };

    let query_auth = query_token.as_deref().map(|token| {
        use subtle::ConstantTimeEq;
        if token.len() != api_key_trimmed.len() {
            return false;
        }
        token.as_bytes().ct_eq(api_key_trimmed.as_bytes()).into()
    });

    if header_auth == Some(true) || query_auth == Some(true) {
        return Ok(());
    }

    if auth_state.auth_enabled {
        if let Some(token) = extract_session_cookie(headers) {
            if crate::session_auth::verify_session_token(&token, &auth_state.session_secret)
                .is_some()
            {
                return Ok(());
            }
        }
    }

    let credential_provided = header_auth.is_some() || query_auth.is_some();
    if credential_provided {
        Err(AuthFailure::InvalidCredentials)
    } else {
        Err(AuthFailure::MissingCredentials)
    }
}

fn auth_error_response(error: AuthFailure) -> Response<Body> {
    let (status, body, www_authenticate) = match error {
        AuthFailure::LoopbackOnly => (
            StatusCode::FORBIDDEN,
            serde_json::json!({
                "error": "Protected endpoints are localhost-only when API auth is disabled"
            })
            .to_string(),
            None,
        ),
        AuthFailure::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            serde_json::json!({"error": "Invalid API key"}).to_string(),
            Some("Bearer"),
        ),
        AuthFailure::MissingCredentials => (
            StatusCode::UNAUTHORIZED,
            serde_json::json!({"error": "Missing authentication credentials"}).to_string(),
            Some("Bearer"),
        ),
    };

    let mut builder = Response::builder().status(status);
    if let Some(value) = www_authenticate {
        builder = builder.header("www-authenticate", value);
    }
    builder.body(Body::from(body)).unwrap_or_default()
}

/// Bearer token authentication middleware.
///
/// When `api_key` is non-empty (after trimming), requests to non-public
/// endpoints must include `Authorization: Bearer <api_key>`.
/// If the key is empty or whitespace-only, auth is disabled entirely
/// (public/local development mode).
///
/// When dashboard auth is enabled, session cookies are also accepted.
pub async fn auth(
    axum::extract::State(auth_state): axum::extract::State<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // SECURITY: Capture method early for method-aware public endpoint checks.
    let method = request.method().clone();
    let path = request.uri().path();

    // Public endpoints that don't require auth.
    // Keep this list intentionally small: the dashboard shell, liveness probe,
    // auth bootstrap, and protocol discovery endpoints that must be reachable
    // before a user has authenticated.
    let is_get = method == axum::http::Method::GET;
    let is_post = method == axum::http::Method::POST;
    let is_public = path == "/"
        || path == "/logo.png"
        || path == "/favicon.ico"
        || path == "/manifest.json"
        || path == "/sw.js"
        || (path == "/.well-known/agent.json" && is_get)
        || (path.starts_with("/a2a/") && is_get)
        || path == "/api/health"
        || path == "/api/version"
        || (path == "/api/auth/login" && is_post)
        || (path == "/api/auth/logout" && is_post)
        || (path == "/api/auth/check" && is_get);

    if is_public {
        return next.run(request).await;
    }

    match authorize_request_parts(
        &auth_state,
        &method,
        path,
        request.headers(),
        request.uri().query(),
        request_is_effective_loopback(&request),
    ) {
        Ok(()) => next.run(request).await,
        Err(error) => auth_error_response(error),
    }
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
    headers.insert(
        "strict-transport-security",
        "max-age=63072000; includeSubDomains".parse().unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::ConnectInfo;
    use axum::http::{HeaderMap, Request};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn test_request_id_header_constant() {
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
    }

    #[tokio::test]
    async fn test_protected_endpoint_is_remote_forbidden_when_auth_disabled() {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/status", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let mut request = Request::builder()
            .uri("/api/status")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 9],
                4242,
            ))));

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_loopback_proxy_request_with_forwarded_remote_ip_is_forbidden_when_auth_disabled()
    {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/status", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let mut request = Request::builder()
            .uri("/api/status")
            .header("x-forwarded-for", "198.51.100.42")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                4242,
            ))));

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_loopback_proxy_request_with_forwarded_ipv6_loopback_stays_allowed() {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/status", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let mut request = Request::builder()
            .uri("/api/status")
            .header("forwarded", "for=\"[::1]:1234\"")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                std::net::Ipv6Addr::LOCALHOST,
                4242,
            ))));

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_is_effective_loopback_rejects_forwarded_remote_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.42".parse().unwrap());
        assert!(!is_effective_loopback(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            &headers
        ));
    }

    #[test]
    fn test_is_effective_loopback_accepts_forwarded_loopback_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("forwarded", "for=\"[::1]:1234\"".parse().unwrap());
        assert!(is_effective_loopback(
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            &headers
        ));
    }

    #[tokio::test]
    async fn test_shutdown_requires_auth_when_api_key_is_configured() {
        let auth_state = AuthState {
            api_key: "secret".to_string(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/shutdown", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let mut request = Request::builder()
            .uri("/api/shutdown")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                4242,
            ))));

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_query_token_rejected_for_regular_endpoint() {
        let auth_state = AuthState {
            api_key: "secret".to_string(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/status", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let request = Request::builder()
            .uri("/api/status?token=secret")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_query_token_allowed_for_stream_endpoint() {
        let auth_state = AuthState {
            api_key: "secret".to_string(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/logs/stream", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let request = Request::builder()
            .uri("/api/logs/stream?token=secret")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_query_token_allowed_for_agent_websocket_endpoint() {
        let auth_state = AuthState {
            api_key: "secret".to_string(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/agents/{id}/ws", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let request = Request::builder()
            .uri("/api/agents/123/ws?token=secret")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_query_token_is_url_decoded_for_stream_endpoint() {
        let auth_state = AuthState {
            api_key: "abc+123=".to_string(),
            auth_enabled: false,
            session_secret: "secret".to_string(),
        };
        let app = Router::new()
            .route("/api/logs/stream", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(auth_state, auth));
        let request = Request::builder()
            .uri("/api/logs/stream?token=abc%2B123%3D")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_authorize_request_parts_accepts_session_cookie_for_websocket() {
        let auth_state = AuthState {
            api_key: "api-secret".to_string(),
            auth_enabled: true,
            session_secret: "api-secret".to_string(),
        };
        let token = crate::session_auth::create_session_token("admin", "api-secret", 1);
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            format!("openfang_session={token}").parse().unwrap(),
        );

        let result = authorize_request_parts(
            &auth_state,
            &Method::GET,
            "/api/agents/123/ws",
            &headers,
            None,
            false,
        );

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_authorize_request_parts_rejects_dashboard_only_websocket_without_cookie() {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: true,
            session_secret: "session-secret".to_string(),
        };

        let result = authorize_request_parts(
            &auth_state,
            &Method::GET,
            "/api/agents/123/ws",
            &HeaderMap::new(),
            None,
            false,
        );

        assert_eq!(result, Err(AuthFailure::MissingCredentials));
    }

    #[test]
    fn test_authorize_request_parts_rejects_empty_bearer_when_only_dashboard_auth_enabled() {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: true,
            session_secret: "session-secret".to_string(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());

        let result = authorize_request_parts(
            &auth_state,
            &Method::GET,
            "/api/status",
            &headers,
            None,
            false,
        );

        assert_eq!(result, Err(AuthFailure::MissingCredentials));
    }

    #[test]
    fn test_authorize_request_parts_rejects_empty_query_token_when_only_dashboard_auth_enabled() {
        let auth_state = AuthState {
            api_key: String::new(),
            auth_enabled: true,
            session_secret: "session-secret".to_string(),
        };

        let result = authorize_request_parts(
            &auth_state,
            &Method::GET,
            "/api/agents/123/ws",
            &HeaderMap::new(),
            Some("token="),
            false,
        );

        assert_eq!(result, Err(AuthFailure::MissingCredentials));
    }
}
