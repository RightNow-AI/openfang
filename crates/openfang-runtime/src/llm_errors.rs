//! LLM error classification and sanitization.
//!
//! Classifies raw LLM API errors into 8 categories using pattern matching
//! against error messages and HTTP status codes. Handles error formats from
//! all 19+ providers OpenFang supports: Anthropic, OpenAI, Gemini, Groq,
//! DeepSeek, Mistral, Together, Fireworks, Ollama, vLLM, LM Studio,
//! Perplexity, Cohere, AI21, Cerebras, SambaNova, HuggingFace, XAI, Replicate.
//!
//! Pattern matching is done via case-insensitive substring checks with no
//! external regex dependency, keeping the crate dependency graph lean.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Classified LLM error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum LlmErrorCategory {
    /// 429, quota exceeded, too many requests.
    RateLimit,
    /// 503, overloaded, service unavailable, high demand.
    Overloaded,
    /// Request timeout, deadline exceeded, ETIMEDOUT, ECONNRESET.
    Timeout,
    /// 402, payment required, insufficient credits/balance.
    Billing,
    /// 401/403, invalid API key, unauthorized, forbidden.
    Auth,
    /// Context length exceeded, max tokens, context window.
    ContextOverflow,
    /// Invalid request format, malformed tool_use, schema violation.
    Format,
    /// Model not found, unknown model, NOT_FOUND.
    ModelNotFound,
}

/// Classified error with metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ClassifiedError {
    /// The classified category.
    pub category: LlmErrorCategory,
    /// `true` for RateLimit, Overloaded, Timeout.
    pub is_retryable: bool,
    /// `true` only for Billing.
    pub is_billing: bool,
    /// Retry delay parsed from the error message, if available.
    pub suggested_delay_ms: Option<u64>,
    /// User-safe message (no raw API details).
    pub sanitized_message: String,
    /// Original error message for logging.
    pub raw_message: String,
}

// ---------------------------------------------------------------------------
// Pattern tables (case-insensitive substring checks)
// ---------------------------------------------------------------------------

/// Context overflow patterns -- checked first because they are highly specific.
const CONTEXT_OVERFLOW_PATTERNS: &[&str] = &[
    "context_length_exceeded",
    "context length",
    "context_length",
    "maximum context",
    "context window",
    "token limit",
    "too many tokens",
    "max_tokens_exceeded",
    "max tokens exceeded",
    "prompt is too long",
    "input too long",
    "context.length",
];

/// Billing patterns.
const BILLING_PATTERNS: &[&str] = &[
    "payment required",
    "insufficient credits",
    "credit balance",
    "billing",
    "insufficient balance",
    "usage limit",
];

/// Auth patterns.
const AUTH_PATTERNS: &[&str] = &[
    "invalid api key",
    "invalid api_key",
    "invalid apikey",
    "incorrect api key",
    "invalid token",
    "unauthorized",
    "forbidden",
    "authentication",
    "permission denied",
];

/// Rate-limit patterns.
const RATE_LIMIT_PATTERNS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "too many requests",
    "exceeded quota",
    "exceeded your quota",
    "resource exhausted",
    "resource_exhausted",
    "quota exceeded",
    "tokens per minute",
    "requests per minute",
    "tpm limit",
    "rpm limit",
];

/// Model-not-found patterns.
const MODEL_NOT_FOUND_PATTERNS: &[&str] = &[
    "model not found",
    "model_not_found",
    "unknown model",
    "does not exist",
    "not_found",
    "model unavailable",
    "model_unavailable",
    "no such model",
    "invalid model",
    "is not found",
];

/// Format / bad-request patterns (catch-all for 400-class issues).
const FORMAT_PATTERNS: &[&str] = &[
    "invalid request",
    "invalid_request",
    "malformed",
    "tool_use",
    "schema",
    "validation error",
    "validation_error",
    "invalid parameter",
    "invalid_parameter",
    "missing required",
    "bad request",
    "bad_request",
];

/// Overloaded patterns.
const OVERLOADED_PATTERNS: &[&str] = &[
    "overloaded",
    "overloaded_error",
    "service unavailable",
    "service_unavailable",
    "high demand",
    "capacity",
    "server_error",
    "internal server error",
    "internal_server_error",
];

/// Timeout / network patterns.
const TIMEOUT_PATTERNS: &[&str] = &[
    "timeout",
    "timed out",
    "deadline exceeded",
    "etimedout",
    "econnreset",
    "econnrefused",
    "econnaborted",
    "epipe",
    "ehostunreach",
    "enetunreach",
    "connection reset",
    "connection refused",
    "network error",
    "fetch failed",
];

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Check if `haystack` (lowercased) contains any pattern from `patterns`.
fn matches_any(haystack: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| haystack.contains(p))
}

/// Classify a raw error message + optional HTTP status into a category.
///
/// Priority order (most specific first):
/// 1. ContextOverflow  2. Billing (402)  3. Auth (401/403)
/// 4. RateLimit (429)  5. ModelNotFound  6. Format (400)
/// 7. Overloaded (503/500)  8. Timeout (network)
///
/// If nothing matches, falls back to `Format` for structured errors or
/// `Timeout` for network-sounding messages.
pub fn classify_error(message: &str, status: Option<u16>) -> ClassifiedError {
    let lower = message.to_lowercase();
    let delay = extract_retry_delay(message);

    // Helper to build ClassifiedError
    let build = |category: LlmErrorCategory| ClassifiedError {
        category,
        is_retryable: matches!(
            category,
            LlmErrorCategory::RateLimit | LlmErrorCategory::Overloaded | LlmErrorCategory::Timeout
        ),
        is_billing: category == LlmErrorCategory::Billing,
        suggested_delay_ms: delay,
        sanitized_message: sanitize_for_user(category, message),
        raw_message: message.to_string(),
    };

    // --- Status-code fast paths (some statuses are unambiguous) ---
    if let Some(code) = status {
        match code {
            429 => return build(LlmErrorCategory::RateLimit),
            402 => return build(LlmErrorCategory::Billing),
            401 => return build(LlmErrorCategory::Auth),
            403 => {
                // 403 could be auth OR rate-limit on some providers; check message
                if matches_any(&lower, RATE_LIMIT_PATTERNS) {
                    return build(LlmErrorCategory::RateLimit);
                }
                return build(LlmErrorCategory::Auth);
            }
            _ => {}
        }
    }

    // --- Pattern matching in priority order ---

    // 1. Context overflow (very specific patterns)
    if matches_any(&lower, CONTEXT_OVERFLOW_PATTERNS) {
        return build(LlmErrorCategory::ContextOverflow);
    }

    // 2. Billing
    if matches_any(&lower, BILLING_PATTERNS) {
        return build(LlmErrorCategory::Billing);
    }
    if status == Some(402) {
        return build(LlmErrorCategory::Billing);
    }

    // 3. Auth
    if matches_any(&lower, AUTH_PATTERNS) {
        return build(LlmErrorCategory::Auth);
    }
    if matches!(status, Some(401) | Some(403)) {
        return build(LlmErrorCategory::Auth);
    }

    // 4. Rate limit
    if matches_any(&lower, RATE_LIMIT_PATTERNS) {
        return build(LlmErrorCategory::RateLimit);
    }
    if status == Some(429) {
        return build(LlmErrorCategory::RateLimit);
    }

    // 5. Model not found
    if matches_any(&lower, MODEL_NOT_FOUND_PATTERNS) {
        return build(LlmErrorCategory::ModelNotFound);
    }
    // Composite check: "model" + "not found" anywhere in the message
    if lower.contains("model") && lower.contains("not found") {
        return build(LlmErrorCategory::ModelNotFound);
    }

    // 6. Format / bad request (before overloaded, since 400 is more specific)
    if matches_any(&lower, FORMAT_PATTERNS) {
        return build(LlmErrorCategory::Format);
    }
    if status == Some(400) {
        return build(LlmErrorCategory::Format);
    }

    // 7. Overloaded
    if matches_any(&lower, OVERLOADED_PATTERNS) {
        return build(LlmErrorCategory::Overloaded);
    }
    if matches!(status, Some(500) | Some(503)) {
        return build(LlmErrorCategory::Overloaded);
    }

    // 8. Timeout / network
    if matches_any(&lower, TIMEOUT_PATTERNS) {
        return build(LlmErrorCategory::Timeout);
    }

    // --- HTML error page detection (Cloudflare etc.) ---
    if is_html_error_page(message) {
        return build(LlmErrorCategory::Overloaded);
    }

    // --- Fallback ---
    // If there's a status code in the 5xx range, treat as overloaded.
    if let Some(code) = status {
        if (500..600).contains(&code) {
            return build(LlmErrorCategory::Overloaded);
        }
        if (400..500).contains(&code) {
            return build(LlmErrorCategory::Format);
        }
    }

    // Last resort: if the message mentions network-like terms, call it timeout;
    // otherwise default to format (unknown structured error).
    if lower.contains("connect") || lower.contains("network") || lower.contains("dns") {
        build(LlmErrorCategory::Timeout)
    } else {
        build(LlmErrorCategory::Format)
    }
}

// ---------------------------------------------------------------------------
// Sanitization
// ---------------------------------------------------------------------------

/// Produce a user-friendly error message.
///
/// Maps each category to a human-readable description, capped at 200 chars.
pub fn sanitize_for_user(category: LlmErrorCategory, _raw: &str) -> String {
    let msg = match category {
        LlmErrorCategory::RateLimit => {
            "The AI provider is rate-limiting requests. Retrying shortly..."
        }
        LlmErrorCategory::Overloaded => "The AI provider is temporarily overloaded. Retrying...",
        LlmErrorCategory::Timeout => "The request timed out. Check your network connection.",
        LlmErrorCategory::Billing => "Billing issue with the AI provider. Check your API plan.",
        LlmErrorCategory::Auth => "Authentication failed. Check your API key configuration.",
        LlmErrorCategory::ContextOverflow => {
            "The conversation is too long for the model's context window."
        }
        LlmErrorCategory::Format => {
            "LLM request failed. Check your API key and model configuration in Settings."
        }
        LlmErrorCategory::ModelNotFound => {
            "The requested model was not found. Check the model name."
        }
    };
    // Cap at 200 chars (all built-in messages are under 200, but defensive).
    if msg.chars().count() > 200 {
        let end = msg
            .char_indices()
            .nth(197)
            .map(|(i, _)| i)
            .unwrap_or(msg.len());
        format!("{}...", &msg[..end])
    } else {
        msg.to_string()
    }
}

// ---------------------------------------------------------------------------
// Retry-After extraction
// ---------------------------------------------------------------------------

/// Try to extract a retry delay (in milliseconds) from the error message.
///
/// Recognizes patterns like:
/// - `retry after 30` (seconds)
/// - `retry-after: 30` (seconds)
/// - `try again in 30` (seconds)
/// - `retry after 500ms` (milliseconds)
///
/// Returns `None` if no recognizable delay is found.
pub fn extract_retry_delay(message: &str) -> Option<u64> {
    let lower = message.to_lowercase();

    // Patterns to search for, each followed by a number.
    const PREFIXES: &[&str] = &["retry after ", "retry-after: ", "try again in "];

    for prefix in PREFIXES {
        if let Some(start) = lower.find(prefix) {
            let after = &lower[start + prefix.len()..];
            // Parse the leading digits.
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(value) = num_str.parse::<u64>() {
                if value == 0 {
                    continue;
                }
                // Check for "ms" suffix (milliseconds).
                let rest = &after[num_str.len()..];
                if rest.starts_with("ms") {
                    return Some(value);
                }
                // Default: treat as seconds, convert to ms.
                return Some(value.saturating_mul(1000));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Transient error detection
// ---------------------------------------------------------------------------

/// Check if an error is likely transient (network hiccup, temporary overload).
///
/// This is a quick heuristic that does not require full classification.
pub fn is_transient(message: &str) -> bool {
    let lower = message.to_lowercase();
    matches_any(&lower, TIMEOUT_PATTERNS)
        || matches_any(&lower, OVERLOADED_PATTERNS)
        || matches_any(&lower, RATE_LIMIT_PATTERNS)
}

// ---------------------------------------------------------------------------
// HTML / Cloudflare error detection
// ---------------------------------------------------------------------------

/// Detect if the response body is a Cloudflare error page or raw HTML
/// instead of expected JSON.
///
/// Checks for: `<!DOCTYPE`, `<html`, Cloudflare error codes (521-530),
/// `cf-error-code`.
pub fn is_html_error_page(body: &str) -> bool {
    let lower = body.to_lowercase();

    // HTML markers
    if lower.contains("<!doctype") || lower.contains("<html") {
        return true;
    }

    // Cloudflare error code header/attribute
    if lower.contains("cf-error-code") || lower.contains("cf-error-type") {
        return true;
    }

    // Cloudflare error status codes in text (e.g., "Error 522" or "522:")
    for code in 521..=530 {
        let code_str = code.to_string();
        if lower.contains(&code_str) && lower.contains("cloudflare") {
            return true;
        }
    }

    false
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Classification tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_classify_rate_limit() {
        // Standard 429
        let e = classify_error("Too Many Requests", Some(429));
        assert_eq!(e.category, LlmErrorCategory::RateLimit);
        assert!(e.is_retryable);

        // Pattern: "rate limit"
        let e = classify_error("You have hit the rate limit for this API", None);
        assert_eq!(e.category, LlmErrorCategory::RateLimit);

        // Pattern: "quota exceeded"
        let e = classify_error("Resource exhausted: quota exceeded", None);
        assert_eq!(e.category, LlmErrorCategory::RateLimit);

        // Pattern: "tokens per minute"
        let e = classify_error("You exceeded your tokens per minute limit", None);
        assert_eq!(e.category, LlmErrorCategory::RateLimit);

        // Pattern: "RPM"
        let e = classify_error("RPM limit reached, slow down", None);
        assert_eq!(e.category, LlmErrorCategory::RateLimit);
    }

    #[test]
    fn test_classify_overloaded() {
        let e = classify_error("The server is currently overloaded", None);
        assert_eq!(e.category, LlmErrorCategory::Overloaded);
        assert!(e.is_retryable);

        let e = classify_error("Service unavailable due to high demand", None);
        assert_eq!(e.category, LlmErrorCategory::Overloaded);

        // Status 503
        let e = classify_error("Please try again later", Some(503));
        assert_eq!(e.category, LlmErrorCategory::Overloaded);

        // Status 500
        let e = classify_error("Something went wrong", Some(500));
        assert_eq!(e.category, LlmErrorCategory::Overloaded);
    }

    #[test]
    fn test_classify_timeout() {
        let e = classify_error("ETIMEDOUT: request timed out", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);
        assert!(e.is_retryable);

        let e = classify_error("ECONNRESET", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);

        let e = classify_error("ECONNREFUSED: connection refused", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);

        let e = classify_error("fetch failed: network error", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);

        let e = classify_error("deadline exceeded while waiting for response", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);
    }

    #[test]
    fn test_classify_billing() {
        let e = classify_error("Payment required", Some(402));
        assert_eq!(e.category, LlmErrorCategory::Billing);
        assert!(e.is_billing);
        assert!(!e.is_retryable);

        let e = classify_error("Insufficient credits in your account", None);
        assert_eq!(e.category, LlmErrorCategory::Billing);

        let e = classify_error("Your credit balance is too low", None);
        assert_eq!(e.category, LlmErrorCategory::Billing);
    }

    #[test]
    fn test_classify_auth() {
        let e = classify_error("Invalid API key provided", Some(401));
        assert_eq!(e.category, LlmErrorCategory::Auth);
        assert!(!e.is_retryable);

        let e = classify_error("Forbidden: you do not have access", Some(403));
        assert_eq!(e.category, LlmErrorCategory::Auth);

        let e = classify_error("Incorrect API key format", None);
        assert_eq!(e.category, LlmErrorCategory::Auth);

        let e = classify_error("Authentication failed for this endpoint", None);
        assert_eq!(e.category, LlmErrorCategory::Auth);
    }

    #[test]
    fn test_classify_context_overflow() {
        let e = classify_error("This model's maximum context length is 128000 tokens", None);
        assert_eq!(e.category, LlmErrorCategory::ContextOverflow);

        let e = classify_error("context_length_exceeded", Some(400));
        assert_eq!(e.category, LlmErrorCategory::ContextOverflow);

        let e = classify_error("prompt is too long for the context window", None);
        assert_eq!(e.category, LlmErrorCategory::ContextOverflow);

        let e = classify_error("input too long: exceeds maximum context", None);
        assert_eq!(e.category, LlmErrorCategory::ContextOverflow);
    }

    #[test]
    fn test_classify_format() {
        let e = classify_error("Invalid request: missing 'messages' field", None);
        assert_eq!(e.category, LlmErrorCategory::Format);

        let e = classify_error("Malformed JSON in request body", None);
        assert_eq!(e.category, LlmErrorCategory::Format);

        let e = classify_error("Validation error: tool_use block missing id", None);
        assert_eq!(e.category, LlmErrorCategory::Format);

        // Status 400 without more specific patterns
        let e = classify_error("Something is wrong with your request", Some(400));
        assert_eq!(e.category, LlmErrorCategory::Format);
    }

    #[test]
    fn test_classify_model_not_found() {
        let e = classify_error("Model 'gpt-5-ultra' not found", None);
        assert_eq!(e.category, LlmErrorCategory::ModelNotFound);

        let e = classify_error("The model does not exist or you lack access", None);
        assert_eq!(e.category, LlmErrorCategory::ModelNotFound);

        let e = classify_error("Unknown model: claude-99", None);
        assert_eq!(e.category, LlmErrorCategory::ModelNotFound);
    }

    #[test]
    fn test_status_code_override() {
        // Even though message says "overloaded", status 429 wins
        let e = classify_error("server overloaded", Some(429));
        assert_eq!(e.category, LlmErrorCategory::RateLimit);

        // Status 402 overrides message
        let e = classify_error("something generic happened", Some(402));
        assert_eq!(e.category, LlmErrorCategory::Billing);

        // Status 401 overrides message
        let e = classify_error("generic error text", Some(401));
        assert_eq!(e.category, LlmErrorCategory::Auth);
    }

    #[test]
    fn test_retryable_categories() {
        // Retryable
        assert!(classify_error("rate limit", None).is_retryable);
        assert!(classify_error("overloaded", None).is_retryable);
        assert!(classify_error("timeout", None).is_retryable);

        // Not retryable
        assert!(!classify_error("", Some(402)).is_retryable); // Billing
        assert!(!classify_error("", Some(401)).is_retryable); // Auth
        assert!(!classify_error("context_length_exceeded", None).is_retryable); // ContextOverflow
        assert!(!classify_error("model not found", None).is_retryable); // ModelNotFound
    }

    #[test]
    fn test_billing_flag() {
        let e = classify_error("payment required", Some(402));
        assert!(e.is_billing);

        let e = classify_error("rate limit exceeded", None);
        assert!(!e.is_billing);

        let e = classify_error("insufficient credits", None);
        assert!(e.is_billing);
    }

    #[test]
    fn test_sanitize_messages() {
        let msg = sanitize_for_user(LlmErrorCategory::RateLimit, "raw error details here");
        assert!(msg.contains("rate-limiting"));
        assert!(!msg.contains("raw error"));

        let msg = sanitize_for_user(LlmErrorCategory::Auth, "sk-xxxx invalid");
        assert!(msg.contains("Authentication"));
        assert!(!msg.contains("sk-xxxx"));

        let msg = sanitize_for_user(LlmErrorCategory::ContextOverflow, "");
        assert!(msg.contains("context window"));

        let msg = sanitize_for_user(LlmErrorCategory::ModelNotFound, "");
        assert!(msg.contains("model"));

        // All messages should be under 200 chars
        for cat in [
            LlmErrorCategory::RateLimit,
            LlmErrorCategory::Overloaded,
            LlmErrorCategory::Timeout,
            LlmErrorCategory::Billing,
            LlmErrorCategory::Auth,
            LlmErrorCategory::ContextOverflow,
            LlmErrorCategory::Format,
            LlmErrorCategory::ModelNotFound,
        ] {
            let m = sanitize_for_user(cat, "test");
            assert!(
                m.len() <= 200,
                "Message for {:?} too long: {}",
                cat,
                m.len()
            );
        }
    }

    #[test]
    fn test_extract_retry_delay() {
        assert_eq!(
            extract_retry_delay("Rate limited. Retry after 30 seconds"),
            Some(30_000)
        );
        assert_eq!(extract_retry_delay("retry-after: 5"), Some(5_000));
        assert_eq!(
            extract_retry_delay("Please try again in 10 seconds"),
            Some(10_000)
        );
        assert_eq!(extract_retry_delay("Retry after 500ms"), Some(500));
    }

    #[test]
    fn test_extract_retry_delay_none() {
        assert_eq!(extract_retry_delay("Something went wrong"), None);
        assert_eq!(extract_retry_delay(""), None);
        assert_eq!(extract_retry_delay("rate limit exceeded"), None);
    }

    #[test]
    fn test_is_transient() {
        assert!(is_transient("Connection reset by peer"));
        assert!(is_transient("ECONNRESET"));
        assert!(is_transient("Request timed out after 30s"));
        assert!(is_transient("Service unavailable"));
        assert!(is_transient("rate limit exceeded"));

        // Non-transient
        assert!(!is_transient("invalid api key"));
        assert!(!is_transient("model not found"));
        assert!(!is_transient("context_length_exceeded"));
    }

    #[test]
    fn test_is_html_error_page() {
        assert!(is_html_error_page(
            "<!DOCTYPE html><html><body>Error</body></html>"
        ));
        assert!(is_html_error_page("<html lang='en'>502 Bad Gateway</html>"));
        assert!(!is_html_error_page(r#"{"error": "rate limit"}"#));
        assert!(!is_html_error_page("plain text error message"));
    }

    #[test]
    fn test_cloudflare_detection() {
        assert!(is_html_error_page(
            "<!DOCTYPE html><html><body>cloudflare 522 connection timed out</body></html>"
        ));
        assert!(is_html_error_page(
            "<html><head><meta cf-error-code='1015'></head></html>"
        ));
    }

    #[test]
    fn test_unknown_error_defaults() {
        // An error with no recognizable pattern and no status code
        let e = classify_error("??? something unknown ???", None);
        // Should default to Format (unknown structured error)
        assert_eq!(e.category, LlmErrorCategory::Format);

        // Network-sounding message without explicit pattern
        let e = classify_error("failed to connect to host", None);
        assert_eq!(e.category, LlmErrorCategory::Timeout);
    }

    #[test]
    fn test_gemini_specific_errors() {
        // Gemini model not found format
        let e = classify_error(
            "models/gemini-ultra is not found for API version v1beta",
            None,
        );
        assert_eq!(e.category, LlmErrorCategory::ModelNotFound);

        // Gemini overloaded
        let e = classify_error("The model is overloaded. Please try again later.", None);
        assert_eq!(e.category, LlmErrorCategory::Overloaded);

        // Gemini resource exhausted (rate limit)
        let e = classify_error("Resource exhausted: request rate limit exceeded", None);
        assert_eq!(e.category, LlmErrorCategory::RateLimit);
    }

    #[test]
    fn test_anthropic_specific_errors() {
        // Anthropic overloaded_error
        let e = classify_error(
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            Some(529),
        );
        assert_eq!(e.category, LlmErrorCategory::Overloaded);

        // Anthropic rate limit
        let e = classify_error(
            "rate_limit_error: Number of request tokens has exceeded your per-minute rate limit",
            Some(429),
        );
        assert_eq!(e.category, LlmErrorCategory::RateLimit);

        // Anthropic invalid API key
        let e = classify_error(
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
            Some(401),
        );
        assert_eq!(e.category, LlmErrorCategory::Auth);
    }
}
