//! Cost-aware rate limiting using GCRA (Generic Cell Rate Algorithm).
//!
//! Each API operation has a token cost (e.g., health=1, spawn=50, message=30).
//! The GCRA algorithm allows 500 tokens per minute per IP address.

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use governor::{clock::DefaultClock, state::keyed::DashMapStateStore, Quota, RateLimiter};
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

pub fn operation_cost(method: &str, path: &str) -> NonZeroU32 {
    match (method, path) {
        (_, "/api/health") => NonZeroU32::new(1).unwrap(),
        ("GET", "/api/status") => NonZeroU32::new(1).unwrap(),
        ("GET", "/api/version") => NonZeroU32::new(1).unwrap(),
        ("GET", "/api/tools") => NonZeroU32::new(1).unwrap(),
        ("GET", "/api/agents") => NonZeroU32::new(2).unwrap(),
        ("GET", "/api/skills") => NonZeroU32::new(2).unwrap(),
        ("GET", "/api/peers") => NonZeroU32::new(2).unwrap(),
        ("GET", "/api/config") => NonZeroU32::new(2).unwrap(),
        ("GET", "/api/usage") => NonZeroU32::new(3).unwrap(),
        ("GET", p) if p.starts_with("/api/audit") => NonZeroU32::new(5).unwrap(),
        ("GET", p) if p.starts_with("/api/marketplace") => NonZeroU32::new(10).unwrap(),
        ("POST", "/api/agents") => NonZeroU32::new(50).unwrap(),
        ("POST", p) if p.contains("/message") => NonZeroU32::new(30).unwrap(),
        ("POST", p) if p.contains("/run") => NonZeroU32::new(100).unwrap(),
        ("POST", "/api/skills/install") => NonZeroU32::new(50).unwrap(),
        ("POST", "/api/skills/uninstall") => NonZeroU32::new(10).unwrap(),
        ("POST", "/api/skills/reload") => NonZeroU32::new(5).unwrap(),
        ("GET", p) if p.starts_with("/api/skills/") && p.ends_with("/config") => {
            NonZeroU32::new(3).unwrap()
        }
        ("PUT", p) if p.starts_with("/api/skills/") && p.ends_with("/config") => {
            NonZeroU32::new(10).unwrap()
        }
        ("DELETE", p) if p.starts_with("/api/skills/") && p.contains("/config/") => {
            NonZeroU32::new(10).unwrap()
        }
        ("POST", "/api/migrate") => NonZeroU32::new(100).unwrap(),
        ("PUT", p) if p.contains("/update") => NonZeroU32::new(10).unwrap(),
        _ => NonZeroU32::new(5).unwrap(),
    }
}

pub type KeyedRateLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;

/// 500 tokens per minute per IP.
pub fn create_rate_limiter() -> Arc<KeyedRateLimiter> {
    Arc::new(RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(500).unwrap(),
    )))
}

/// GCRA rate limiting middleware.
///
/// Extracts the client IP from `ConnectInfo`, computes the cost for the
/// requested operation, and checks the GCRA limiter. Returns 429 if the
/// client has exhausted its token budget.
pub async fn gcra_rate_limit(
    axum::extract::State(limiter): axum::extract::State<Arc<KeyedRateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let ip = request
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]));

    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let cost = operation_cost(&method, &path);

    if limiter.check_key_n(&ip, cost).is_err() {
        tracing::warn!(ip = %ip, cost = cost.get(), path = %path, "GCRA rate limit exceeded");
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("content-type", "application/json")
            .header("retry-after", "60")
            .body(Body::from(
                serde_json::json!({"error": "Rate limit exceeded"}).to_string(),
            ))
            .unwrap_or_default();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_costs() {
        assert_eq!(operation_cost("GET", "/api/health").get(), 1);
        assert_eq!(operation_cost("GET", "/api/tools").get(), 1);
        assert_eq!(operation_cost("POST", "/api/agents/1/message").get(), 30);
        assert_eq!(operation_cost("POST", "/api/agents").get(), 50);
        assert_eq!(operation_cost("POST", "/api/workflows/1/run").get(), 100);
        assert_eq!(operation_cost("GET", "/api/agents/1/session").get(), 5);
        assert_eq!(operation_cost("GET", "/api/skills").get(), 2);
        assert_eq!(operation_cost("GET", "/api/peers").get(), 2);
        assert_eq!(operation_cost("GET", "/api/audit/recent").get(), 5);
        assert_eq!(operation_cost("POST", "/api/skills/install").get(), 50);
        assert_eq!(operation_cost("POST", "/api/migrate").get(), 100);
    }
}
