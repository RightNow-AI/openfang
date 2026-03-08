//! Unified SSRF protection for all URL-fetching code paths.
//!
//! Provides a single `check_ssrf()` function used by `web_fetch.rs`
//! (builtin tools), `host_functions.rs` (WASM guest network calls),
//! `browser.rs`, and `tool_runner.rs`.

use std::net::{IpAddr, ToSocketAddrs};

/// Check if a URL targets a private/internal network resource.
/// Blocks localhost, metadata endpoints, private IPs.
/// Fails CLOSED: if DNS resolution fails, the request is blocked.
/// Must run BEFORE any network I/O.
pub fn check_ssrf(url: &str) -> Result<(), String> {
    // Only allow http:// and https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }

    // Parse with url crate to properly handle userinfo, IPv6, etc.
    let parsed =
        url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    let hostname = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Hostname-based blocklist (catches metadata endpoints before DNS)
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
    // Strip brackets for comparison (url crate returns "::1" not "[::1]" for IPv6)
    let cmp_host = hostname.trim_start_matches('[').trim_end_matches(']');
    if blocked.iter().any(|b| {
        let b_trimmed = b.trim_start_matches('[').trim_end_matches(']');
        b_trimmed.eq_ignore_ascii_case(cmp_host)
    }) {
        return Err(format!(
            "SSRF blocked: {hostname} is a restricted hostname"
        ));
    }

    // Resolve DNS and check every returned IP.
    // FAIL CLOSED: if DNS resolution fails, block the request.
    let port = parsed.port_or_known_default().unwrap_or(80);
    let socket_addr = format!("{hostname}:{port}");
    let addrs = socket_addr.to_socket_addrs().map_err(|e| {
        format!("SSRF blocked: DNS resolution failed for {hostname}: {e}")
    })?;

    for addr in addrs {
        let ip = addr.ip();
        if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
            return Err(format!(
                "SSRF blocked: {hostname} resolves to private IP {ip}"
            ));
        }
    }

    Ok(())
}

/// Check if a URL is an SSRF target, returning serde_json error for WASM host functions.
pub fn check_ssrf_json(url: &str) -> Result<(), serde_json::Value> {
    check_ssrf(url).map_err(|msg| serde_json::json!({"error": msg}))
}

/// Extract host (without userinfo or path) from a URL for capability checking.
pub fn extract_host_for_capability(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("unknown");
        let port = parsed.port_or_known_default().unwrap_or(80);
        if host.contains(':') && !host.starts_with('[') {
            // IPv6 without brackets — wrap in brackets
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        }
    } else {
        url.to_string()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Existing behavior (must still pass) ---

    #[test]
    fn test_blocks_localhost() {
        assert!(check_ssrf("http://localhost/admin").is_err());
        assert!(check_ssrf("http://localhost:8080/api").is_err());
    }

    #[test]
    fn test_blocks_private_ips() {
        assert!(check_ssrf("http://10.0.0.1/").is_err());
        assert!(check_ssrf("http://172.16.0.1/").is_err());
        assert!(check_ssrf("http://192.168.1.1/").is_err());
    }

    #[test]
    fn test_blocks_metadata_endpoints() {
        assert!(
            check_ssrf("http://169.254.169.254/latest/meta-data/").is_err()
        );
        assert!(check_ssrf(
            "http://metadata.google.internal/computeMetadata/v1/"
        )
        .is_err());
        assert!(
            check_ssrf("http://100.100.100.200/latest/meta-data/").is_err()
        );
        assert!(
            check_ssrf("http://192.0.0.192/metadata/instance").is_err()
        );
    }

    #[test]
    fn test_blocks_non_http_schemes() {
        assert!(check_ssrf("file:///etc/passwd").is_err());
        assert!(check_ssrf("ftp://internal.corp/data").is_err());
        assert!(check_ssrf("gopher://evil.com").is_err());
    }

    #[test]
    fn test_blocks_ipv6_localhost() {
        assert!(check_ssrf("http://[::1]/admin").is_err());
        assert!(check_ssrf("http://[::1]:8080/api").is_err());
    }

    #[test]
    fn test_blocks_zero_ip() {
        assert!(check_ssrf("http://0.0.0.0/").is_err());
    }

    #[test]
    fn test_allows_public_urls() {
        assert!(check_ssrf("https://example.com/").is_ok());
        assert!(check_ssrf("https://google.com/search?q=test").is_ok());
    }

    // --- NEW: Bypass prevention tests ---

    #[test]
    fn test_blocks_userinfo_bypass() {
        assert!(check_ssrf("http://user@localhost/admin").is_err());
        assert!(
            check_ssrf("http://user:pass@localhost:8080/api").is_err()
        );
        assert!(
            check_ssrf("http://foo@169.254.169.254/latest/").is_err()
        );
        assert!(check_ssrf("http://x@[::1]/").is_err());
    }

    #[test]
    fn test_fails_closed_on_dns_failure() {
        assert!(check_ssrf(
            "http://this-domain-does-not-exist.invalid/secret"
        )
        .is_err());
    }

    #[test]
    fn test_extract_host_strips_userinfo() {
        assert_eq!(
            extract_host_for_capability("http://user:pass@example.com/path"),
            "example.com:80"
        );
        assert_eq!(
            extract_host_for_capability(
                "https://token@api.github.com/repos"
            ),
            "api.github.com:443"
        );
    }

    #[test]
    fn test_extract_host_normal() {
        assert_eq!(
            extract_host_for_capability("http://example.com:8080/path"),
            "example.com:8080"
        );
        assert_eq!(
            extract_host_for_capability("https://example.com/path"),
            "example.com:443"
        );
        assert_eq!(
            extract_host_for_capability("http://[::1]:8080/path"),
            "[::1]:8080"
        );
    }

    #[test]
    fn test_is_private_ip() {
        use std::net::IpAddr;
        assert!(is_private_ip(&"10.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"192.168.1.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip(&"169.254.169.254".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip(&"8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip(&"1.1.1.1".parse::<IpAddr>().unwrap()));
    }
}
