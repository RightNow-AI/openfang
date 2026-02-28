//! Host function implementations for the WASM sandbox.
//!
//! Each function checks capabilities before executing. Deny-by-default:
//! if no matching capability is found, the operation is rejected.
//!
//! These functions are called from the `host_call` dispatch in `sandbox.rs`.
//! They receive `&GuestState` (not `&mut`) and return JSON values.

use crate::sandbox::GuestState;
use openfang_types::capability::{capability_matches, Capability};
use serde_json::json;
use std::net::ToSocketAddrs;
use std::path::{Component, Path};
use tracing::debug;

/// Dispatch a host call to the appropriate handler.
///
/// Returns JSON: `{"ok": ...}` on success, `{"error": "..."}` on failure.
pub fn dispatch(state: &GuestState, method: &str, params: &serde_json::Value) -> serde_json::Value {
    debug!(method, "WASM host_call dispatch");
    match method {
        // Always allowed (no capability check)
        "time_now" => host_time_now(),

        // Filesystem — requires FileRead/FileWrite
        "fs_read" => host_fs_read(state, params),
        "fs_write" => host_fs_write(state, params),
        "fs_list" => host_fs_list(state, params),

        // Network — requires NetConnect
        "net_fetch" => host_net_fetch(state, params),

        // Shell — requires ShellExec
        "shell_exec" => host_shell_exec(state, params),

        // Environment — requires EnvRead
        "env_read" => host_env_read(state, params),

        // Memory KV — requires MemoryRead/MemoryWrite
        "kv_get" => host_kv_get(state, params),
        "kv_set" => host_kv_set(state, params),

        // Agent interaction — requires AgentMessage/AgentSpawn
        "agent_send" => host_agent_send(state, params),
        "agent_spawn" => host_agent_spawn(state, params),

        _ => json!({"error": format!("Unknown host method: {method}")}),
    }
}

// ---------------------------------------------------------------------------
// Capability checking
// ---------------------------------------------------------------------------

/// Check that the guest has a capability matching `required`.
/// Returns `Ok(())` if granted, `Err(json)` with an error response if denied.
fn check_capability(
    capabilities: &[Capability],
    required: &Capability,
) -> Result<(), serde_json::Value> {
    for granted in capabilities {
        if capability_matches(granted, required) {
            return Ok(());
        }
    }
    Err(json!({"error": format!("Capability denied: {required:?}")}))
}

// ---------------------------------------------------------------------------
// Path traversal protection
// ---------------------------------------------------------------------------

/// Secure path resolution — NEVER returns raw unchecked paths.
/// Rejects traversal components, resolves symlinks where possible.
fn safe_resolve_path(path: &str) -> Result<std::path::PathBuf, serde_json::Value> {
    let p = Path::new(path);

    // Phase 1: Reject any path with ".." components (even if they'd resolve safely)
    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied: '..' components forbidden"}));
        }
    }

    // Phase 2: Canonicalize to resolve symlinks and normalize
    std::fs::canonicalize(p).map_err(|e| json!({"error": format!("Cannot resolve path: {e}")}))
}

/// For writes where the file may not exist yet: canonicalize the parent, validate the filename.
fn safe_resolve_parent(path: &str) -> Result<std::path::PathBuf, serde_json::Value> {
    let p = Path::new(path);

    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied: '..' components forbidden"}));
        }
    }

    let parent = p
        .parent()
        .filter(|par| !par.as_os_str().is_empty())
        .ok_or_else(|| json!({"error": "Invalid path: no parent directory"}))?;

    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|e| json!({"error": format!("Cannot resolve parent directory: {e}")}))?;

    let file_name = p
        .file_name()
        .ok_or_else(|| json!({"error": "Invalid path: no file name"}))?;

    // Double-check filename doesn't contain traversal (belt-and-suspenders)
    if file_name.to_string_lossy().contains("..") {
        return Err(json!({"error": "Path traversal denied in file name"}));
    }

    Ok(canonical_parent.join(file_name))
}

// ---------------------------------------------------------------------------
// SSRF protection
// ---------------------------------------------------------------------------

/// SSRF protection: check if a hostname resolves to a private/internal IP.
///
/// Returns `Ok(Some((domain, resolved_addrs)))` with the resolved addresses
/// so callers can pin them in the HTTP client to prevent DNS rebinding TOCTOU.
/// Returns `Ok(None)` if DNS resolution failed.
fn is_ssrf_target(url: &str) -> Result<Option<(String, Vec<std::net::SocketAddr>)>, serde_json::Value> {
    // Only allow http:// and https:// schemes (block file://, gopher://, ftp://)
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(json!({"error": "Only http:// and https:// URLs are allowed"}));
    }

    let host = extract_host_from_url(url);
    // Handle IPv6 bracket notation: [::1] → extract inside brackets for matching
    let hostname = if host.starts_with('[') {
        host.find(']')
            .map(|i| &host[..=i])
            .unwrap_or(&host)
    } else {
        host.split(':').next().unwrap_or(&host)
    };

    // Check hostname-based blocklist first (catches metadata endpoints)
    let blocked_hostnames = [
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
    if blocked_hostnames.contains(&hostname) {
        return Err(json!({"error": format!("SSRF blocked: {hostname} is a restricted hostname")}));
    }

    // Resolve DNS and check every returned IP.
    // SECURITY: Return the resolved addresses so callers can pin them in
    // the HTTP client, preventing DNS rebinding TOCTOU attacks.
    let port = if url.starts_with("https") { 443 } else { 80 };
    let socket_addr = format!("{hostname}:{port}");
    if let Ok(addrs) = socket_addr.to_socket_addrs() {
        let resolved: Vec<std::net::SocketAddr> = addrs.collect();
        for addr in &resolved {
            let ip = addr.ip();
            if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
                return Err(json!({"error": format!(
                    "SSRF blocked: {hostname} resolves to private IP {ip}"
                )}));
            }
        }
        Ok(Some((hostname.to_string(), resolved)))
    } else {
        Ok(None)
    }
}

fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let octets = v4.octets();
            matches!(
                octets,
                [10, ..] | [172, 16..=31, ..] | [192, 168, ..] | [169, 254, ..]
            )
        }
        std::net::IpAddr::V6(v6) => {
            let segments = v6.segments();
            (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
        }
    }
}

// ---------------------------------------------------------------------------
// Always-allowed functions
// ---------------------------------------------------------------------------

fn host_time_now() -> serde_json::Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    json!({"ok": now})
}

// ---------------------------------------------------------------------------
// Filesystem (capability-checked)
// ---------------------------------------------------------------------------

fn host_fs_read(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let path = match params.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing 'path' parameter"}),
    };
    // SECURITY: Canonicalize BEFORE capability check to prevent symlink escape.
    // If we check capabilities on the raw path and then read the canonical path,
    // a symlink under a permitted directory can point to arbitrary files.
    let canonical = match safe_resolve_path(path) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let canonical_str = canonical.to_string_lossy().to_string();
    if let Err(e) = check_capability(&state.capabilities, &Capability::FileRead(canonical_str)) {
        return e;
    }
    match std::fs::read_to_string(&canonical) {
        Ok(content) => json!({"ok": content}),
        Err(e) => json!({"error": format!("fs_read failed: {e}")}),
    }
}

fn host_fs_write(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let path = match params.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing 'path' parameter"}),
    };
    let content = match params.get("content").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return json!({"error": "Missing 'content' parameter"}),
    };
    // SECURITY: Canonicalize BEFORE capability check to prevent symlink escape.
    let write_path = match safe_resolve_parent(path) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let canonical_str = write_path.to_string_lossy().to_string();
    if let Err(e) = check_capability(
        &state.capabilities,
        &Capability::FileWrite(canonical_str),
    ) {
        return e;
    }
    match std::fs::write(&write_path, content) {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"error": format!("fs_write failed: {e}")}),
    }
}

fn host_fs_list(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let path = match params.get("path").and_then(|p| p.as_str()) {
        Some(p) => p,
        None => return json!({"error": "Missing 'path' parameter"}),
    };
    // SECURITY: Canonicalize BEFORE capability check to prevent symlink escape.
    let canonical = match safe_resolve_path(path) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let canonical_str = canonical.to_string_lossy().to_string();
    if let Err(e) = check_capability(&state.capabilities, &Capability::FileRead(canonical_str)) {
        return e;
    }
    match std::fs::read_dir(&canonical) {
        Ok(entries) => {
            let names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            json!({"ok": names})
        }
        Err(e) => json!({"error": format!("fs_list failed: {e}")}),
    }
}

// ---------------------------------------------------------------------------
// Network (capability-checked)
// ---------------------------------------------------------------------------

fn host_net_fetch(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let url = match params.get("url").and_then(|u| u.as_str()) {
        Some(u) => u,
        None => return json!({"error": "Missing 'url' parameter"}),
    };
    let method = params
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET");
    let body = params.get("body").and_then(|b| b.as_str()).unwrap_or("");

    // SECURITY: SSRF protection — check resolved IP against private ranges.
    // Returns resolved addresses to pin in reqwest, preventing DNS rebinding.
    let ssrf_result = match is_ssrf_target(url) {
        Ok(result) => result,
        Err(e) => return e,
    };

    // Extract host:port from URL for capability check
    let host = extract_host_from_url(url);
    if let Err(e) = check_capability(&state.capabilities, &Capability::NetConnect(host)) {
        return e;
    }

    state.tokio_handle.block_on(async {
        // SECURITY: Build a per-request client with pinned DNS resolution and
        // disabled redirects to prevent both DNS rebinding and redirect-based
        // SSRF bypass attacks.
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none());
        if let Some((domain, addrs)) = ssrf_result {
            builder = builder.resolve_to_addrs(&domain, &addrs);
        }
        let client = builder.build().unwrap_or_default();
        let request = match method.to_uppercase().as_str() {
            "POST" => client.post(url).body(body.to_string()),
            "PUT" => client.put(url).body(body.to_string()),
            "DELETE" => client.delete(url),
            _ => client.get(url),
        };
        match request.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match resp.text().await {
                    Ok(text) => json!({"ok": {"status": status, "body": text}}),
                    Err(e) => json!({"error": format!("Failed to read response: {e}")}),
                }
            }
            Err(e) => json!({"error": format!("Request failed: {e}")}),
        }
    })
}

/// Extract host:port from a URL for capability checking.
/// Handles IPv6 bracket notation: `http://[::1]:8080/path` → `[::1]:8080`
fn extract_host_from_url(url: &str) -> String {
    if let Some(after_scheme) = url.split("://").nth(1) {
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        // Handle IPv6 bracket notation: [::1]:8080 or [::1]
        if host_port.starts_with('[') {
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

// ---------------------------------------------------------------------------
// Shell (capability-checked)
// ---------------------------------------------------------------------------

fn host_shell_exec(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let command = match params.get("command").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return json!({"error": "Missing 'command' parameter"}),
    };
    if let Err(e) = check_capability(
        &state.capabilities,
        &Capability::ShellExec(command.to_string()),
    ) {
        return e;
    }

    let args: Vec<&str> = params
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // Command::new does NOT use a shell — safe from shell injection.
    // Each argument is passed directly to the process.
    match std::process::Command::new(command).args(&args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            json!({
                "ok": {
                    "exit_code": output.status.code(),
                    "stdout": stdout,
                    "stderr": stderr,
                }
            })
        }
        Err(e) => json!({"error": format!("shell_exec failed: {e}")}),
    }
}

// ---------------------------------------------------------------------------
// Environment (capability-checked)
// ---------------------------------------------------------------------------

fn host_env_read(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return json!({"error": "Missing 'name' parameter"}),
    };
    if let Err(e) = check_capability(&state.capabilities, &Capability::EnvRead(name.to_string())) {
        return e;
    }
    match std::env::var(name) {
        Ok(val) => json!({"ok": val}),
        Err(_) => json!({"ok": null}),
    }
}

// ---------------------------------------------------------------------------
// Memory KV (capability-checked, uses kernel handle)
// ---------------------------------------------------------------------------

fn host_kv_get(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let key = match params.get("key").and_then(|k| k.as_str()) {
        Some(k) => k,
        None => return json!({"error": "Missing 'key' parameter"}),
    };
    if let Err(e) = check_capability(
        &state.capabilities,
        &Capability::MemoryRead(key.to_string()),
    ) {
        return e;
    }
    let kernel = match &state.kernel {
        Some(k) => k,
        None => return json!({"error": "No kernel handle available"}),
    };
    match kernel.memory_recall(key) {
        Ok(Some(val)) => json!({"ok": val}),
        Ok(None) => json!({"ok": null}),
        Err(e) => json!({"error": e}),
    }
}

fn host_kv_set(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let key = match params.get("key").and_then(|k| k.as_str()) {
        Some(k) => k,
        None => return json!({"error": "Missing 'key' parameter"}),
    };
    let value = match params.get("value") {
        Some(v) => v.clone(),
        None => return json!({"error": "Missing 'value' parameter"}),
    };
    if let Err(e) = check_capability(
        &state.capabilities,
        &Capability::MemoryWrite(key.to_string()),
    ) {
        return e;
    }
    let kernel = match &state.kernel {
        Some(k) => k,
        None => return json!({"error": "No kernel handle available"}),
    };
    match kernel.memory_store(key, value) {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"error": e}),
    }
}

// ---------------------------------------------------------------------------
// Agent interaction (capability-checked, uses kernel handle)
// ---------------------------------------------------------------------------

fn host_agent_send(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let target = match params.get("target").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return json!({"error": "Missing 'target' parameter"}),
    };
    let message = match params.get("message").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return json!({"error": "Missing 'message' parameter"}),
    };
    if let Err(e) = check_capability(
        &state.capabilities,
        &Capability::AgentMessage(target.to_string()),
    ) {
        return e;
    }
    let kernel = match &state.kernel {
        Some(k) => k,
        None => return json!({"error": "No kernel handle available"}),
    };
    match state
        .tokio_handle
        .block_on(kernel.send_to_agent(target, message))
    {
        Ok(response) => json!({"ok": response}),
        Err(e) => json!({"error": e}),
    }
}

fn host_agent_spawn(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    if let Err(e) = check_capability(&state.capabilities, &Capability::AgentSpawn) {
        return e;
    }
    let manifest_toml = match params.get("manifest").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return json!({"error": "Missing 'manifest' parameter"}),
    };
    let kernel = match &state.kernel {
        Some(k) => k,
        None => return json!({"error": "No kernel handle available"}),
    };
    // SECURITY: Enforce capability inheritance — child <= parent
    match state.tokio_handle.block_on(kernel.spawn_agent_checked(
        manifest_toml,
        Some(&state.agent_id),
        &state.capabilities,
    )) {
        Ok((id, name)) => json!({"ok": {"id": id, "name": name}}),
        Err(e) => json!({"error": e}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(capabilities: Vec<Capability>) -> GuestState {
        GuestState {
            capabilities,
            kernel: None,
            agent_id: "test-agent".to_string(),
            tokio_handle: tokio::runtime::Handle::current(),
            store_limits: wasmtime::StoreLimitsBuilder::new().build(),
        }
    }

    #[tokio::test]
    async fn test_time_now_always_allowed() {
        let result = host_time_now();
        assert!(result.get("ok").is_some());
        let ts = result["ok"].as_u64().unwrap();
        assert!(ts > 1_700_000_000);
    }

    #[tokio::test]
    async fn test_fs_read_denied_no_capability() {
        let state = test_state(vec![]);
        let result = host_fs_read(&state, &json!({"path": "/etc/passwd"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_fs_write_denied_no_capability() {
        let state = test_state(vec![]);
        let result = host_fs_write(&state, &json!({"path": "/tmp/test", "content": "hello"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_fs_read_granted_wildcard() {
        let state = test_state(vec![Capability::FileRead("*".to_string())]);
        let result = host_fs_read(&state, &json!({"path": "Cargo.toml"}));
        // Should not be capability-denied (may still fail on path)
        if let Some(err) = result.get("error") {
            let msg = err.as_str().unwrap_or("");
            assert!(
                !msg.contains("denied"),
                "Should not be capability-denied: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn test_shell_exec_denied() {
        let state = test_state(vec![]);
        let result = host_shell_exec(&state, &json!({"command": "ls"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_env_read_denied() {
        let state = test_state(vec![]);
        let result = host_env_read(&state, &json!({"name": "HOME"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_env_read_granted() {
        let state = test_state(vec![Capability::EnvRead("PATH".to_string())]);
        let result = host_env_read(&state, &json!({"name": "PATH"}));
        assert!(result.get("ok").is_some(), "Expected ok: {:?}", result);
    }

    #[tokio::test]
    async fn test_kv_get_no_kernel() {
        let state = test_state(vec![Capability::MemoryRead("*".to_string())]);
        let result = host_kv_get(&state, &json!({"key": "test"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("kernel"));
    }

    #[tokio::test]
    async fn test_agent_send_denied() {
        let state = test_state(vec![]);
        let result = host_agent_send(&state, &json!({"target": "some-agent", "message": "hello"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_agent_spawn_denied() {
        let state = test_state(vec![]);
        let result = host_agent_spawn(&state, &json!({"manifest": "name = 'test'"}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("denied"));
    }

    #[tokio::test]
    async fn test_dispatch_unknown_method() {
        let state = test_state(vec![]);
        let result = dispatch(&state, "bogus_method", &json!({}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("Unknown"));
    }

    #[tokio::test]
    async fn test_missing_params() {
        let state = test_state(vec![Capability::FileRead("*".to_string())]);
        let result = host_fs_read(&state, &json!({}));
        let err = result["error"].as_str().unwrap();
        assert!(err.contains("Missing"));
    }

    #[test]
    fn test_safe_resolve_path_traversal() {
        assert!(safe_resolve_path("../etc/passwd").is_err());
        assert!(safe_resolve_path("/tmp/../../etc/passwd").is_err());
        assert!(safe_resolve_path("foo/../bar").is_err());
    }

    #[test]
    fn test_safe_resolve_parent_traversal() {
        assert!(safe_resolve_parent("../malicious.txt").is_err());
        assert!(safe_resolve_parent("/tmp/../../etc/shadow").is_err());
    }

    #[test]
    fn test_ssrf_private_ips_blocked() {
        assert!(is_ssrf_target("http://127.0.0.1:8080/secret").is_err());
        assert!(is_ssrf_target("http://localhost:3000/api").is_err());
        assert!(is_ssrf_target("http://169.254.169.254/metadata").is_err());
        assert!(is_ssrf_target("http://metadata.google.internal/v1/instance").is_err());
    }

    #[test]
    fn test_ssrf_public_ips_allowed() {
        assert!(is_ssrf_target("https://api.openai.com/v1/chat").is_ok());
        assert!(is_ssrf_target("https://google.com").is_ok());
    }

    #[test]
    fn test_ssrf_scheme_validation() {
        assert!(is_ssrf_target("file:///etc/passwd").is_err());
        assert!(is_ssrf_target("gopher://evil.com").is_err());
        assert!(is_ssrf_target("ftp://example.com").is_err());
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

    #[test]
    fn test_extract_host_from_url() {
        assert_eq!(
            extract_host_from_url("https://api.openai.com/v1/chat"),
            "api.openai.com:443"
        );
        assert_eq!(
            extract_host_from_url("http://localhost:8080/api"),
            "localhost:8080"
        );
        assert_eq!(
            extract_host_from_url("http://example.com"),
            "example.com:80"
        );
    }

    #[test]
    fn test_extract_host_ipv6_bracket_notation() {
        assert_eq!(
            extract_host_from_url("http://[::1]:8080/path"),
            "[::1]:8080"
        );
        assert_eq!(
            extract_host_from_url("https://[::1]/path"),
            "[::1]:443"
        );
        assert_eq!(
            extract_host_from_url("http://[::1]/path"),
            "[::1]:80"
        );
    }

    #[test]
    fn test_ssrf_blocks_ipv6_localhost() {
        assert!(is_ssrf_target("http://[::1]/secret").is_err());
        assert!(is_ssrf_target("http://[::1]:8080/api").is_err());
        assert!(is_ssrf_target("https://[::1]/admin").is_err());
    }

    #[test]
    fn test_ssrf_blocks_cloud_metadata() {
        // Alibaba Cloud IMDS
        assert!(is_ssrf_target("http://100.100.100.200/latest/meta-data/").is_err());
        // Azure IMDS alternative
        assert!(is_ssrf_target("http://192.0.0.192/metadata/instance").is_err());
        // Zero IP
        assert!(is_ssrf_target("http://0.0.0.0/").is_err());
    }
}
