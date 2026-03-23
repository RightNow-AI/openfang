//! Production middleware for the OpenFang API server.
//!
//! Provides:
//! - Request ID generation and propagation
//! - Per-endpoint structured request logging
//! - In-memory rate limiting (per IP)

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::api_response::ApiError;
use crate::auth::{self, AuthPrincipal};
use crate::rate_limiter;
use crate::request_context::{RequestContext, RequestId};
use crate::routes::AppState;
use crate::security::{log_security_event, SecurityLog};

/// Request ID header name (standard).
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Middleware: inject a unique request ID and log the request/response.
pub async fn request_logging(request: Request<Body>, next: Next) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut request = request;
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

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

/// Normalize framework-generated empty 404/405 responses into the standard API envelope.
pub async fn normalize_empty_error_responses(request: Request<Body>, next: Next) -> Response<Body> {
    let ctx = RequestContext::from_request(&request);
    let response = next.run(request).await;

    let is_standardized = response.headers().contains_key(header::CONTENT_TYPE);
    if is_standardized {
        return response;
    }

    let mut replacement = match response.status() {
        StatusCode::NOT_FOUND => ApiError::not_found("Route not found", ctx.request_id()).into_response(),
        StatusCode::METHOD_NOT_ALLOWED => {
            ApiError::method_not_allowed("Method not allowed", ctx.request_id()).into_response()
        }
        _ => return response,
    };

    if let Some(allow) = response.headers().get(header::ALLOW).cloned() {
        replacement.headers_mut().insert(header::ALLOW, allow);
    }

    replacement
}

/// Bearer token authentication middleware.
///
/// When `api_key` is non-empty, requests to non-public endpoints must include
/// `Authorization: Bearer <api_key>`.
pub async fn auth(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let mut request = request;
    let ctx = RequestContext::from_request(&request);

    // SECURITY: Capture method early for method-aware public endpoint checks.
    let method = request.method().clone();

    // Shutdown is loopback-only (CLI on same machine) — skip token auth
    let path = request.uri().path();
    if path == "/api/shutdown" {
        let is_loopback = request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().is_loopback())
            .unwrap_or(false); // SECURITY: default-deny — unknown origin is NOT loopback
        if is_loopback {
            return next.run(request).await;
        }
    }

    // Public endpoints that NEVER require auth (even if API key is set)
    let is_get = method == axum::http::Method::GET;
    let is_public = path == "/"
        || path == "/logo.png"
        || path == "/favicon.ico"
        || (path == "/.well-known/agent.json" && is_get)
        || (path.starts_with("/a2a/") && is_get)
        || path == "/api/health"
        || path == "/api/version"
        || path == "/api/auth/status"
        || path == "/api/auth/github/start"
        || path.starts_with("/api/auth/github/poll/")
        || path.starts_with("/api/providers/github-copilot/oauth/");

    if is_public {
        return next.run(request).await;
    }

    if !auth::auth_required(&state) {
        return next.run(request).await;
    }

    match auth::resolve_principal(request.headers(), request.uri(), &state) {
        Ok(Some(principal)) => {
            let actor_type = match &principal {
                AuthPrincipal::ApiKey => "api_key",
                AuthPrincipal::User(_) => "jwt",
            };
            request.extensions_mut().insert(principal);
            log_security_event(
                &SecurityLog::new("auth.success", "info", "success", &ctx)
                    .actor_type(actor_type),
            );
            next.run(request).await
        }
        Ok(None) => {
            log_security_event(
                &SecurityLog::new("auth.missing_token", "warn", "denied", &ctx)
                    .actor_type("bearer")
                    .reason("missing api key or session token"),
            );

            let mut response = ApiError::unauthorized(
                "Missing Authorization: Bearer <api_key_or_session_token> header",
                ctx.request_id(),
            )
            .into_response();
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
            response
        }
        Err(error_msg) => {
            log_security_event(
                &SecurityLog::new("auth.invalid_token", "warn", "denied", &ctx)
                    .actor_type("bearer")
                    .reason(&error_msg),
            );

            let mut response = ApiError::unauthorized(error_msg, ctx.request_id()).into_response();
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
            response
        }
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

/// Per-user rate limiting middleware for LLM-heavy endpoints.
///
/// C5: The GCRA IP limiter (`gcra_rate_limit`) throttles anonymous and API-key
/// access by IP. This middleware adds a second layer that throttles individual
/// *authenticated user accounts* to prevent a single OAuth user from consuming
/// the entire IP quota on a shared server.
///
/// Rate: 100 LLM tokens / minute / user_id (from JWT session).
/// API-key principals (admin) bypass per-user limiting.
pub async fn user_rate_limit(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Only restrict authenticated JWT users. API-key principals are admin-level
    // callers (CLI, scripts) and are already throttled by the IP-based limiter.
    let user_id = match request.extensions().get::<AuthPrincipal>() {
        Some(AuthPrincipal::User(session)) => session.user_id.clone(),
        _ => return next.run(request).await,
    };

    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let cost = rate_limiter::operation_cost(&method, &path);

    match state.user_rate_limiter.check_key_n(&user_id, cost) {
        Ok(Ok(_)) => next.run(request).await,
        Ok(Err(_)) | Err(_) => {
            let ctx = RequestContext::from_request(&request);
            log_security_event(
                &SecurityLog::new("rate_limit.user_blocked", "warn", "denied", &ctx)
                    .actor_type("jwt")
                    .reason(format!(
                        "per-user token budget exhausted for user={user_id}; cost={c}",
                        c = cost.get()
                    )),
            );
            let mut response =
                ApiError::rate_limited("Per-user rate limit exceeded", ctx.request_id())
                    .into_response();
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("60"),
            );
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::to_bytes,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[test]
    fn test_request_id_header_constant() {
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
    }

    #[tokio::test]
    async fn test_normalize_empty_not_found_response() {
        let app = Router::new()
            .route("/ok", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(normalize_empty_error_responses))
            .layer(axum::middleware::from_fn(request_logging));

        let response = app
            .oneshot(Request::builder().uri("/missing").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let body: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
        )
        .unwrap();

        assert_eq!(body["success"], false);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert_eq!(body["error"]["message"], "Route not found");
        assert!(body["error"]["request_id"].is_string());
    }

    #[tokio::test]
    async fn test_normalize_empty_method_not_allowed_response_preserves_allow() {
        let app = Router::new()
            .route("/ok", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(normalize_empty_error_responses))
            .layer(axum::middleware::from_fn(request_logging));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers().get(header::ALLOW).unwrap(), "GET,HEAD");

        let body: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
        )
        .unwrap();

        assert_eq!(body["success"], false);
        assert_eq!(body["error"]["code"], "METHOD_NOT_ALLOWED");
        assert_eq!(body["error"]["message"], "Method not allowed");
        assert!(body["error"]["request_id"].is_string());
    }
}
