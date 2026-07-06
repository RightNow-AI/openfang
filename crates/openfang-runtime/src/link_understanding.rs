//! Link understanding — auto-extract and summarize URLs from messages.

use tracing::warn;

/// Configuration for link understanding (re-exported from types).
pub use omtae_types::media::LinkConfig;

/// Summary of a fetched link.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkSummary {
    pub url: String,
    pub title: Option<String>,
    /// Content preview, max 2000 chars.
    pub content_preview: String,
    pub content_type: String,
}

/// Extract URLs from text, with SSRF validation.
///
/// Returns up to `max` valid, unique, non-private URLs.
pub fn extract_urls(text: &str, max: usize) -> Vec<String> {
    // Simple but effective URL regex
    let url_pattern = regex_lite::Regex::new(
        r#"https?://[^\s<>\[\](){}|\\^`"']+[^\s<>\[\](){}|\\^`"'.,;:!?\-)]"#,
    )
    .expect("URL regex is valid");

    let mut seen = std::collections::HashSet::new();
    let mut urls = Vec::new();

    for m in url_pattern.find_iter(text) {
        let url = m.as_str().to_string();

        // Deduplicate
        if !seen.insert(url.clone()) {
            continue;
        }

        // SECURITY: SSRF check — reject private IPs and metadata endpoints
        if is_private_url(&url) {
            warn!("Rejected private/SSRF URL: {}", url);
            continue;
        }

        urls.push(url);
        if urls.len() >= max {
            break;
        }
    }

    urls
}

/// Check if a URL points to a private/internal address (SSRF protection).
fn is_private_url(url: &str) -> bool {
    // Parse host from URL
    let authority = match url.split("://").nth(1) {
        Some(rest) => rest.split('/').next().unwrap_or(""),
        None => return true,
    };

    // Handle IPv6 bracket notation (e.g. [::1]:8080)
    let host = if authority.starts_with('[') {
        // Extract content between brackets
        authority
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        authority.split(':').next().unwrap_or("")
    };

    let host_lower = host.to_lowercase();

    // Block common SSRF targets
    if host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "0.0.0.0"
        || host_lower == "::1"
        || host_lower == "[::1]"
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".internal")
        || host_lower.starts_with("10.")
        || host_lower.starts_with("192.168.")
        || host_lower == "metadata.google.internal"
        || host_lower == "169.254.169.254"
    {
        return true;
    }

    // Block 172.16-31.x.x range
    if host_lower.starts_with("172.") {
        if let Some(second_octet) = host_lower.split('.').nth(1) {
            if let Ok(n) = second_octet.parse::<u8>() {
                if (16..=31).contains(&n) {
                    return true;
                }
            }
        }
    }

    false
}

/// Build link context string to inject into agent messages.
///
/// Returns None if no links found or link understanding is disabled.
pub fn build_link_context(text: &str, config: &LinkConfig) -> Option<String> {
    if !config.enabled {
        return None;
    }

    let urls = extract_urls(text, config.max_links);
    if urls.is_empty() {
        return None;
    }

    let mut context = String::from("\n\n[Link Context - URLs detected in message]\n");
    for url in &urls {
        context.push_str(&format!("- {url}\n"));
    }
    context.push_str(
        "Use web_fetch to retrieve content from these URLs if relevant to the user's request.\n",
    );
    Some(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls_basic() {
        let text = "Check out https://example.com and http://test.org/page";
        let urls = extract_urls(text, 10);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("example.com"));
        assert!(urls[1].contains("test.org"));
    }

    #[test]
    fn test_extract_urls_dedup() {
        let text = "Visit https://example.com and also https://example.com again";
        let urls = extract_urls(text, 10);
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn test_extract_urls_max_limit() {
        let text = "https://a.com https://b.com https://c.com https://d.com https://e.com";
        let urls = extract_urls(text, 3);
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn test_extract_urls_no_urls() {
        let text = "No URLs here, just plain text.";
        let urls = extract_urls(text, 10);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_ssrf_localhost_blocked() {
        assert!(is_private_url("http://localhost/admin"));
        assert!(is_private_url("http://127.0.0.1:8080/secret"));
        assert!(is_private_url("http://0.0.0.0/"));
        assert!(is_private_url("http://[::1]/"));
    }

    #[test]
    fn test_ssrf_private_ranges_blocked() {
        assert!(is_private_url("http://10.0.0.1/internal"));
        assert!(is_private_url("http://192.168.1.1/admin"));
        assert!(is_private_url("http://172.16.0.1/secret"));
        assert!(is_private_url("http://172.31.255.255/data"));
    }

    #[test]
    fn test_ssrf_metadata_blocked() {
        assert!(is_private_url("http://169.254.169.254/latest/meta-data/"));
        assert!(is_private_url("http://metadata.google.internal/"));
    }

    #[test]
    fn test_ssrf_public_allowed() {
        assert!(!is_private_url("https://example.com/page"));
        assert!(!is_private_url("https://api.github.com/repos"));
        assert!(!is_private_url("https://docs.rust-lang.org/"));
    }

    #[test]
    fn test_ssrf_172_non_private() {
        // 172.32.x.x is NOT private
        assert!(!is_private_url("http://172.32.0.1/ok"));
        assert!(!is_private_url("http://172.15.0.1/ok"));
    }

    #[test]
    fn test_extract_urls_filters_private() {
        let text =
            "Public: https://example.com Private: http://localhost/admin http://192.168.1.1/secret";
        let urls = extract_urls(text, 10);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("example.com"));
    }

    #[test]
    fn test_build_link_context_disabled() {
        let config = LinkConfig {
            enabled: false,
            ..Default::default()
        };
        let result = build_link_context("https://example.com", &config);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_link_context_enabled() {
        let config = LinkConfig {
            enabled: true,
            ..Default::default()
        };
        let result = build_link_context("Check https://example.com", &config);
        assert!(result.is_some());
        let ctx = result.unwrap();
        assert!(ctx.contains("example.com"));
        assert!(ctx.contains("Link Context"));
    }

    #[test]
    fn test_build_link_context_no_urls() {
        let config = LinkConfig {
            enabled: true,
            ..Default::default()
        };
        let result = build_link_context("No URLs here", &config);
        assert!(result.is_none());
    }
}
