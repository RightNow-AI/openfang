//! Enhanced web fetch with SSRF protection, HTML→Markdown extraction,
//! in-memory caching, and external content markers.
//!
//! Pipeline: SSRF check → cache lookup → HTTP GET → detect HTML →
//! html_to_markdown() → truncate → wrap_external_content() → cache → return

use crate::web_cache::WebCache;
use crate::web_content::{html_to_markdown, wrap_external_content};
use openfang_types::config::WebFetchConfig;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;
use tracing::debug;

/// Enhanced web fetch engine with SSRF protection and readability extraction.
pub struct WebFetchEngine {
    config: WebFetchConfig,
    client: reqwest::Client,
    cache: Arc<WebCache>,
}

impl WebFetchEngine {
    /// Create a new fetch engine from config with a shared cache.
    pub fn new(config: WebFetchConfig, cache: Arc<WebCache>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            cache,
        }
    }

    /// Fetch a URL with full security pipeline.
    pub async fn fetch(&self, url: &str) -> Result<String, String> {
        // Step 1: SSRF protection — BEFORE any network I/O
        check_ssrf(url)?;

        // Step 2: Cache lookup
        let cache_key = format!("fetch:{}", url);
        if let Some(cached) = self.cache.get(&cache_key) {
            debug!(url, "Fetch cache hit");
            return Ok(cached);
        }

        // Step 3: HTTP GET
        let resp = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (compatible; OpenFangAgent/0.1)")
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status = resp.status();

        // Check response size
        if let Some(len) = resp.content_length() {
            if len > self.config.max_response_bytes as u64 {
                return Err(format!(
                    "Response too large: {} bytes (max {})",
                    len, self.config.max_response_bytes
                ));
            }
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        // Step 4: Detect HTML and optionally convert to Markdown
        let processed = if self.config.readability && is_html(&content_type, &body) {
            let markdown = html_to_markdown(&body);
            if markdown.trim().is_empty() {
                // Fallback to raw text if extraction produced nothing
                body
            } else {
                markdown
            }
        } else {
            body
        };

        // Step 5: Truncate
        let truncated = if processed.len() > self.config.max_chars {
            format!(
                "{}... [truncated, {} total chars]",
                &processed[..self.config.max_chars],
                processed.len()
            )
        } else {
            processed
        };

        // Step 6: Wrap with external content markers
        let result = format!(
            "HTTP {status}\n\n{}",
            wrap_external_content(url, &truncated)
        );

        // Step 7: Cache
        self.cache.put(cache_key, result.clone());

        Ok(result)
    }
}

/// Detect if content is HTML based on Content-Type header or body sniffing.
fn is_html(content_type: &str, body: &str) -> bool {
    if content_type.contains("text/html") || content_type.contains("application/xhtml") {
        return true;
    }
    // Sniff: check if body starts with HTML-like content
    let trimmed = body.trim_start();
    trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
}

// ---------------------------------------------------------------------------
// SSRF Protection (replicates host_functions.rs logic for builtin tools)
// ---------------------------------------------------------------------------

/// Check if a URL targets a private/internal network resource.
/// Blocks localhost, metadata endpoints, and private IPs.
/// Must run BEFORE any network I/O.
pub(crate) fn check_ssrf(url: &str) -> Result<(), String> {
    // Only allow http:// and https:// schemes
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }

    let host = extract_host(url);
    // For IPv6 bracket notation like [::1]:80, extract [::1] as hostname
    let hostname = if host.starts_with('[') {
        host.find(']').map(|i| &host[..=i]).unwrap_or(&host)
    } else {
        host.split(':').next().unwrap_or(&host)
    };

    // Hostname-based blocklist (catches metadata endpoints)
    let blocked = [
        "localhost",
        "ip6-localhost",
        "metadata.google.internal",
        "metadata.aws.internal",
        "instance-data",
        "169.254.169.254",
        "100.100.100.200", // Alibaba Cloud IMDS
        "192.0.0.192",     // Azure IMDS alternative
        "0.0.0.0",
        "::1",
        "[::1]",
    ];
    if blocked.contains(&hostname) {
        return Err(format!("SSRF blocked: {hostname} is a restricted hostname"));
    }

    // Resolve DNS and check every returned IP
    let port = if url.starts_with("https") { 443 } else { 80 };
    let socket_addr = format!("{hostname}:{port}");
    if let Ok(addrs) = socket_addr.to_socket_addrs() {
        for addr in addrs {
            let ip = addr.ip();
            if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
                return Err(format!(
                    "SSRF blocked: {hostname} resolves to private IP {ip}"
                ));
            }
        }
    }

    Ok(())
}

/// Check if an IP address is in a private range.
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            matches!(
                octets,
                [10, ..] | [172, 16..=31, ..] | [192, 168, ..] | [169, 254, ..]
            )
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Extract host:port from a URL.
fn extract_host(url: &str) -> String {
    if let Some(after_scheme) = url.split("://").nth(1) {
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        // Handle IPv6 bracket notation: [::1]:8080
        if host_port.starts_with('[') {
            // Extract [addr]:port or [addr]
            if let Some(bracket_end) = host_port.find(']') {
                let ipv6_host = &host_port[..=bracket_end]; // includes brackets
                let after_bracket = &host_port[bracket_end + 1..];
                if let Some(port) = after_bracket.strip_prefix(':') {
                    return format!("{ipv6_host}:{port}");
                }
                let default_port = if url.starts_with("https") { 443 } else { 80 };
                return format!("{ipv6_host}:{default_port}");
            }
        }
        if host_port.contains(':') {
            host_port.to_string()
        } else if url.starts_with("https") {
            format!("{host_port}:443")
        } else {
            format!("{host_port}:80")
        }
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrf_blocks_localhost() {
        assert!(check_ssrf("http://localhost/admin").is_err());
        assert!(check_ssrf("http://localhost:8080/api").is_err());
    }

    #[test]
    fn test_ssrf_blocks_private_ip() {
        use std::net::IpAddr;
        assert!(is_private_ip(&"10.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"192.168.1.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"169.254.169.254".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn test_ssrf_blocks_metadata() {
        assert!(check_ssrf("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(check_ssrf("http://metadata.google.internal/computeMetadata/v1/").is_err());
    }

    #[test]
    fn test_ssrf_allows_public() {
        assert!(!is_private_ip(
            &"8.8.8.8".parse::<std::net::IpAddr>().unwrap()
        ));
        assert!(!is_private_ip(
            &"1.1.1.1".parse::<std::net::IpAddr>().unwrap()
        ));
    }

    #[test]
    fn test_ssrf_blocks_non_http() {
        assert!(check_ssrf("file:///etc/passwd").is_err());
        assert!(check_ssrf("ftp://internal.corp/data").is_err());
        assert!(check_ssrf("gopher://evil.com").is_err());
    }

    #[test]
    fn test_ssrf_blocks_cloud_metadata() {
        // Alibaba Cloud IMDS
        assert!(check_ssrf("http://100.100.100.200/latest/meta-data/").is_err());
        // Azure IMDS alternative
        assert!(check_ssrf("http://192.0.0.192/metadata/instance").is_err());
    }

    #[test]
    fn test_ssrf_blocks_zero_ip() {
        assert!(check_ssrf("http://0.0.0.0/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_ipv6_localhost() {
        assert!(check_ssrf("http://[::1]/admin").is_err());
        assert!(check_ssrf("http://[::1]:8080/api").is_err());
    }

    #[test]
    fn test_extract_host_ipv6() {
        let h = extract_host("http://[::1]:8080/path");
        assert_eq!(h, "[::1]:8080");

        let h2 = extract_host("https://[::1]/path");
        assert_eq!(h2, "[::1]:443");

        let h3 = extract_host("http://[::1]/path");
        assert_eq!(h3, "[::1]:80");
    }
}
