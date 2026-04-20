//! Enhanced web fetch with SSRF protection, HTML→Markdown extraction,
//! in-memory caching, and external content markers.
//!
//! Pipeline: SSRF check → cache lookup → HTTP GET → detect HTML →
//! html_to_markdown() → truncate → wrap_external_content() → cache → return

use crate::str_utils::safe_truncate_str;
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
            .user_agent(crate::USER_AGENT)
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .gzip(true)
            .deflate(true)
            .brotli(true)
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            cache,
        }
    }

    /// Fetch a URL with full security pipeline (GET only, for backwards compat).
    pub async fn fetch(&self, url: &str) -> Result<String, String> {
        self.fetch_with_options(url, "GET", None, None).await
    }

    /// Fetch a URL with configurable HTTP method, headers, and body.
    pub async fn fetch_with_options(
        &self,
        url: &str,
        method: &str,
        headers: Option<&serde_json::Map<String, serde_json::Value>>,
        body: Option<&str>,
    ) -> Result<String, String> {
        let method_upper = method.to_uppercase();

        // Step 1: SSRF protection — BEFORE any network I/O
        check_ssrf(url, &self.config.ssrf_allowed_hosts)?;

        // Step 2: Cache lookup (only for GET)
        let cache_key = format!("fetch:{}:{}", method_upper, url);
        if method_upper == "GET" {
            if let Some(cached) = self.cache.get(&cache_key) {
                debug!(url, "Fetch cache hit");
                return Ok(cached);
            }
        }

        // Step 3: Build request with configured method
        let mut req = match method_upper.as_str() {
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "PATCH" => self.client.patch(url),
            "DELETE" => self.client.delete(url),
            _ => self.client.get(url),
        };
        req = req.header(
            "User-Agent",
            format!("Mozilla/5.0 (compatible; {})", crate::USER_AGENT),
        );

        // Add custom headers
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                if let Some(val) = v.as_str() {
                    req = req.header(k.as_str(), val);
                }
            }
        }

        // Add body for non-GET methods
        if let Some(b) = body {
            // Auto-detect JSON body
            if b.trim_start().starts_with('{') || b.trim_start().starts_with('[') {
                req = req.header("Content-Type", "application/json");
            }
            req = req.body(b.to_string());
        }

        let resp = req
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

        let resp_body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        // Step 4: For GET requests, detect HTML and convert to Markdown.
        // For non-GET (API calls), return raw body — don't mangle JSON/XML responses.
        let processed = if method_upper == "GET"
            && self.config.readability
            && is_html(&content_type, &resp_body)
        {
            let markdown = html_to_markdown(&resp_body);
            if markdown.trim().is_empty() {
                resp_body
            } else {
                markdown
            }
        } else {
            resp_body
        };

        // Step 5: Truncate (char-boundary-safe to avoid panics on multi-byte UTF-8)
        let truncated = if processed.len() > self.config.max_chars {
            format!(
                "{}... [truncated, {} total chars]",
                safe_truncate_str(&processed, self.config.max_chars),
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

        // Step 7: Cache (only GET responses)
        if method_upper == "GET" {
            self.cache.put(cache_key, result.clone());
        }

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
///
/// The `allowed_hosts` slice lets self-hosted deployments bypass the
/// private-IP check for specific hosts.  Entries can be exact hostnames
/// (`"n8n.local"`), wildcard domains (`"*.olares.com"`), or CIDR ranges
/// (`"10.0.0.0/8"`).
///
/// **Cloud metadata endpoints are NEVER allowed regardless of the allowlist.**
pub(crate) fn check_ssrf(url: &str, allowed_hosts: &[String]) -> Result<(), String> {
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
    // These are UNCONDITIONALLY blocked — no allowlist can override them.
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

    // Check if the hostname is explicitly allowed before doing DNS resolution.
    if is_host_allowed(hostname, allowed_hosts) {
        return Ok(());
    }

    // Resolve DNS and check every returned IP
    let port = if url.starts_with("https") { 443 } else { 80 };
    let socket_addr = format!("{hostname}:{port}");
    if let Ok(addrs) = socket_addr.to_socket_addrs() {
        for addr in addrs {
            let ip = addr.ip();
            if is_metadata_ip(&ip) {
                // Metadata IPs are NEVER allowed, even via allowlist.
                return Err(format!(
                    "SSRF blocked: {hostname} resolves to metadata IP {ip}"
                ));
            }
            if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
                // Check if the resolved IP matches a CIDR in the allowlist.
                if is_ip_allowed(&ip, allowed_hosts) {
                    continue;
                }
                return Err(format!(
                    "SSRF blocked: {hostname} resolves to private IP {ip}"
                ));
            }
        }
    }

    Ok(())
}

/// Returns true if an IP is a cloud metadata endpoint address.
fn is_metadata_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 169.254.169.254 (AWS/GCP/Azure IMDS)
            octets == [169, 254, 169, 254]
            // 100.100.100.200 (Alibaba Cloud IMDS)
            || octets == [100, 100, 100, 200]
            // 192.0.0.192 (Azure IMDS alternative)
            || octets == [192, 0, 0, 192]
        }
        _ => false,
    }
}

/// Check if a hostname matches any entry in the allowlist.
/// Supports exact match and wildcard domains (`*.example.com`).
fn is_host_allowed(hostname: &str, allowed_hosts: &[String]) -> bool {
    let lower = hostname.to_lowercase();
    for entry in allowed_hosts {
        let entry_lower = entry.to_lowercase();
        // Exact match
        if entry_lower == lower {
            return true;
        }
        // Wildcard domain: *.example.com matches sub.example.com
        if let Some(suffix) = entry_lower.strip_prefix("*.") {
            if lower.ends_with(&format!(".{suffix}")) || lower == suffix {
                return true;
            }
        }
    }
    false
}

/// Check if an IP address matches any CIDR entry in the allowlist.
fn is_ip_allowed(ip: &IpAddr, allowed_hosts: &[String]) -> bool {
    for entry in allowed_hosts {
        if let Some(pos) = entry.find('/') {
            // Parse as CIDR: base_ip/prefix_len
            let base_str = &entry[..pos];
            let prefix_str = &entry[pos + 1..];
            if let (Ok(base_ip), Ok(prefix_len)) =
                (base_str.parse::<IpAddr>(), prefix_str.parse::<u32>())
            {
                if ip_in_cidr(ip, &base_ip, prefix_len) {
                    return true;
                }
            }
        } else if let Ok(entry_ip) = entry.parse::<IpAddr>() {
            // Exact IP match
            if *ip == entry_ip {
                return true;
            }
        }
    }
    false
}

/// Check if `ip` falls within the CIDR block `base/prefix_len`.
fn ip_in_cidr(ip: &IpAddr, base: &IpAddr, prefix_len: u32) -> bool {
    match (ip, base) {
        (IpAddr::V4(ip4), IpAddr::V4(base4)) => {
            if prefix_len > 32 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let ip_bits = u32::from_be_bytes(ip4.octets());
            let base_bits = u32::from_be_bytes(base4.octets());
            let mask = !0u32 << (32 - prefix_len);
            (ip_bits & mask) == (base_bits & mask)
        }
        (IpAddr::V6(ip6), IpAddr::V6(base6)) => {
            if prefix_len > 128 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let ip_bits = u128::from_be_bytes(ip6.octets());
            let base_bits = u128::from_be_bytes(base6.octets());
            let mask = !0u128 << (128 - prefix_len);
            (ip_bits & mask) == (base_bits & mask)
        }
        _ => false, // mismatched families
    }
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
    use crate::str_utils::safe_truncate_str;

    #[test]
    fn test_truncate_multibyte_no_panic() {
        // Simulate a gzip-decoded response containing multi-byte UTF-8
        // (Chinese, Japanese, emoji — common on international finance sites).
        // Old code: &s[..max] panics when max lands inside a multi-byte char.
        let content = "\u{4f60}\u{597d}\u{4e16}\u{754c}!"; // "你好世界!" = 13 bytes
                                                           // Truncate at byte 7 — lands inside the 3rd Chinese char (bytes 6..9).
                                                           // safe_truncate_str walks back to byte 6, returning "你好".
        let truncated = safe_truncate_str(content, 7);
        assert_eq!(truncated, "\u{4f60}\u{597d}");
        assert!(truncated.len() <= 7);
    }

    #[test]
    fn test_truncate_emoji_no_panic() {
        let content = "\u{1f4b0}\u{1f4c8}\u{1f4b9}"; // 💰📈💹 = 12 bytes
                                                     // Truncate at byte 5 — lands inside the 2nd emoji (bytes 4..8).
        let truncated = safe_truncate_str(content, 5);
        assert_eq!(truncated, "\u{1f4b0}"); // 4 bytes
    }

    #[test]
    fn test_ssrf_blocks_localhost() {
        assert!(check_ssrf("http://localhost/admin", &[]).is_err());
        assert!(check_ssrf("http://localhost:8080/api", &[]).is_err());
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
        assert!(check_ssrf("http://169.254.169.254/latest/meta-data/", &[]).is_err());
        assert!(check_ssrf("http://metadata.google.internal/computeMetadata/v1/", &[]).is_err());
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
        assert!(check_ssrf("file:///etc/passwd", &[]).is_err());
        assert!(check_ssrf("ftp://internal.corp/data", &[]).is_err());
        assert!(check_ssrf("gopher://evil.com", &[]).is_err());
    }

    #[test]
    fn test_ssrf_blocks_cloud_metadata() {
        // Alibaba Cloud IMDS
        assert!(check_ssrf("http://100.100.100.200/latest/meta-data/", &[]).is_err());
        // Azure IMDS alternative
        assert!(check_ssrf("http://192.0.0.192/metadata/instance", &[]).is_err());
    }

    #[test]
    fn test_ssrf_blocks_zero_ip() {
        assert!(check_ssrf("http://0.0.0.0/", &[]).is_err());
    }

    #[test]
    fn test_ssrf_blocks_ipv6_localhost() {
        assert!(check_ssrf("http://[::1]/admin", &[]).is_err());
        assert!(check_ssrf("http://[::1]:8080/api", &[]).is_err());
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

    // ── SSRF allowlist tests ─────────────────────────────────────────────

    #[test]
    fn test_ssrf_allowlist_permits_private_ip() {
        // A CIDR allowlist entry should permit an otherwise-blocked private IP.
        let allow = vec!["10.0.0.0/8".to_string()];
        assert!(check_ssrf("http://10.1.2.3", &allow).is_ok());
    }

    #[test]
    fn test_ssrf_allowlist_still_blocks_metadata() {
        // Even if the allowlist covers the entire link-local range,
        // cloud metadata endpoints must NEVER be permitted.
        let allow = vec!["169.254.0.0/16".to_string()];
        assert!(check_ssrf("http://169.254.169.254/latest/meta-data/", &allow).is_err());
        // Also verify hostname-based metadata blocks
        let allow2 = vec!["metadata.google.internal".to_string()];
        assert!(check_ssrf(
            "http://metadata.google.internal/computeMetadata/v1/",
            &allow2
        )
        .is_err());
    }

    #[test]
    fn test_ssrf_allowlist_wildcard_domain() {
        let allow = vec!["*.example.com".to_string()];
        assert!(check_ssrf("http://api.example.com", &allow).is_ok());
        // Non-matching domain should still go through normal checks
        assert!(!is_host_allowed("other.net", &allow));
    }

    #[test]
    fn test_ssrf_allowlist_exact_hostname() {
        let allow = vec!["n8n.local".to_string()];
        assert!(check_ssrf("http://n8n.local/webhook", &allow).is_ok());
    }

    #[test]
    fn test_cidr_matching() {
        let ip_in: IpAddr = "10.1.2.3".parse().unwrap();
        let ip_out: IpAddr = "11.0.0.1".parse().unwrap();
        let base: IpAddr = "10.0.0.0".parse().unwrap();
        assert!(ip_in_cidr(&ip_in, &base, 8));
        assert!(!ip_in_cidr(&ip_out, &base, 8));
    }
}
