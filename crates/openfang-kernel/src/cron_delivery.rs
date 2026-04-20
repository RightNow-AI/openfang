//! Multi-destination cron output delivery.
//!
//! A single [`CronJob`] may declare zero or more [`CronDeliveryTarget`]s on
//! its `delivery_targets` field. After the job fires and produces output,
//! the [`CronDeliveryEngine`] fans out the same payload to every target
//! concurrently. Failures in one target do not abort delivery to the
//! others — every target's outcome is returned in a [`DeliveryResult`].
//!
//! This is the OpenFang port of the Hermes Agent multi-destination cron
//! pattern: one job → N destinations (channels / webhooks / files / email).

use futures::future::join_all;
use openfang_channels::bridge::ChannelBridgeHandle;
use openfang_types::scheduler::CronDeliveryTarget;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Webhook HTTP timeout. Matches the legacy single-target cron webhook.
const WEBHOOK_TIMEOUT_SECS: u64 = 30;

/// Per-target delivery outcome returned by [`CronDeliveryEngine::deliver`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// Human-readable target description (`"channel:telegram -> chat_123"`,
    /// `"webhook:https://..."`, `"file:/tmp/out.log"`, `"email:alice@x"`).
    pub target: String,
    /// Whether delivery succeeded.
    pub success: bool,
    /// Error message if `success` is `false`.
    pub error: Option<String>,
}

impl DeliveryResult {
    fn ok(target: String) -> Self {
        Self {
            target,
            success: true,
            error: None,
        }
    }

    fn err(target: String, msg: String) -> Self {
        Self {
            target,
            success: false,
            error: Some(msg),
        }
    }
}

/// Fan-out delivery engine for cron job output.
///
/// Holds a reference to the channel bridge (for adapter-based delivery) and
/// a shared HTTP client (for webhook delivery). Constructed once per kernel
/// and reused across every cron firing.
pub struct CronDeliveryEngine {
    /// Bridge used to invoke `send_channel_message` on registered adapters.
    channel_bridge: Arc<dyn ChannelBridgeHandle>,
    /// Shared HTTP client for webhook delivery.
    http: reqwest::Client,
}

impl CronDeliveryEngine {
    /// Build a new engine using the given channel bridge and a fresh
    /// `reqwest::Client`. Falls back to the default client if the builder
    /// fails (which effectively never happens on supported platforms).
    pub fn new(channel_bridge: Arc<dyn ChannelBridgeHandle>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(WEBHOOK_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self {
            channel_bridge,
            http,
        }
    }

    /// Build a new engine with an explicit HTTP client — used by tests.
    pub fn with_http_client(
        channel_bridge: Arc<dyn ChannelBridgeHandle>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            channel_bridge,
            http,
        }
    }

    /// Deliver `output` to every target concurrently.
    ///
    /// Returns a `Vec<DeliveryResult>` with one entry per target in the same
    /// order as the input slice. One target failing does not short-circuit
    /// the others — the job already succeeded, delivery is best-effort.
    pub async fn deliver(
        &self,
        targets: &[CronDeliveryTarget],
        job_name: &str,
        output: &str,
    ) -> Vec<DeliveryResult> {
        if targets.is_empty() {
            return Vec::new();
        }
        let futures = targets
            .iter()
            .map(|t| self.deliver_one(t, job_name, output));
        join_all(futures).await
    }

    /// Deliver to a single target. Never panics.
    async fn deliver_one(
        &self,
        target: &CronDeliveryTarget,
        job_name: &str,
        output: &str,
    ) -> DeliveryResult {
        match target {
            CronDeliveryTarget::Channel {
                channel_type,
                recipient,
            } => {
                let desc = format!("channel:{channel_type} -> {recipient}");
                match self
                    .channel_bridge
                    .send_channel_message(channel_type, recipient, output)
                    .await
                {
                    Ok(()) => {
                        debug!(target = %desc, "Cron fan-out: channel delivery ok");
                        DeliveryResult::ok(desc)
                    }
                    Err(e) => {
                        warn!(target = %desc, error = %e, "Cron fan-out: channel delivery failed");
                        DeliveryResult::err(desc, e)
                    }
                }
            }
            CronDeliveryTarget::Webhook { url, auth_header } => {
                let desc = format!("webhook:{url}");
                match deliver_webhook(&self.http, url, auth_header.as_deref(), job_name, output)
                    .await
                {
                    Ok(()) => {
                        debug!(target = %desc, "Cron fan-out: webhook delivery ok");
                        DeliveryResult::ok(desc)
                    }
                    Err(e) => {
                        warn!(target = %desc, error = %e, "Cron fan-out: webhook delivery failed");
                        DeliveryResult::err(desc, e)
                    }
                }
            }
            CronDeliveryTarget::LocalFile { path, append } => {
                let desc = format!("file:{path}");
                match deliver_local_file(Path::new(path), *append, output).await {
                    Ok(()) => {
                        debug!(target = %desc, "Cron fan-out: file write ok");
                        DeliveryResult::ok(desc)
                    }
                    Err(e) => {
                        warn!(target = %desc, error = %e, "Cron fan-out: file write failed");
                        DeliveryResult::err(desc, e)
                    }
                }
            }
            CronDeliveryTarget::Email {
                to,
                subject_template,
            } => {
                let desc = format!("email:{to}");
                let subject = render_subject(subject_template.as_deref(), job_name);
                // The existing email channel adapter sends via SMTP and does
                // not expose a subject/to pair on the trait, so we route a
                // formatted message through it. Most adapters treat the
                // recipient as a destination identifier; the email adapter
                // uses it as the RCPT TO address.
                let body = format!("{subject}\n\n{output}");
                match self
                    .channel_bridge
                    .send_channel_message("email", to, &body)
                    .await
                {
                    Ok(()) => {
                        debug!(target = %desc, "Cron fan-out: email delivery ok");
                        DeliveryResult::ok(desc)
                    }
                    Err(e) => {
                        warn!(target = %desc, error = %e, "Cron fan-out: email delivery failed");
                        DeliveryResult::err(desc, e)
                    }
                }
            }
        }
    }
}

/// Render an email subject from an optional template. `{job}` is the only
/// supported placeholder; everything else passes through unchanged.
fn render_subject(template: Option<&str>, job_name: &str) -> String {
    match template {
        Some(t) if !t.is_empty() => t.replace("{job}", job_name),
        _ => format!("Cron: {job_name}"),
    }
}

/// POST a JSON payload `{ job, output, timestamp }` to `url` and optionally
/// attach an `Authorization` header. Returns `Err(msg)` on non-2xx or
/// network failure.
async fn deliver_webhook(
    http: &reqwest::Client,
    url: &str,
    auth_header: Option<&str>,
    job_name: &str,
    output: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "job": job_name,
        "output": output,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let mut req = http.post(url).json(&payload);
    if let Some(auth) = auth_header {
        req = req.header("Authorization", auth);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("webhook send failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("webhook returned HTTP {status}"));
    }
    Ok(())
}

/// Append or overwrite `output` at `path`. Creates parent directories when
/// missing. Returns `Err(msg)` on any I/O failure.
async fn deliver_local_file(path: &Path, append: bool, output: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("create parent dir failed: {e}"))?;
        }
    }
    if append {
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|e| format!("open failed: {e}"))?;
        f.write_all(output.as_bytes())
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        // Newline separator between runs makes tailing nicer.
        f.write_all(b"\n")
            .await
            .map_err(|e| format!("write newline failed: {e}"))?;
    } else {
        tokio::fs::write(path, output.as_bytes())
            .await
            .map_err(|e| format!("write failed: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use openfang_channels::bridge::ChannelBridgeHandle;
    use openfang_types::agent::AgentId;
    use std::sync::Mutex;

    /// Mock bridge that records every channel send. Optionally fails for
    /// specific channel names.
    struct MockBridge {
        calls: Mutex<Vec<(String, String, String)>>,
        fail_on_channel: Option<String>,
    }

    impl MockBridge {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                fail_on_channel: None,
            })
        }

        fn failing_on(channel: &str) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                fail_on_channel: Some(channel.to_string()),
            })
        }

        fn calls(&self) -> Vec<(String, String, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ChannelBridgeHandle for MockBridge {
        async fn send_message(&self, _: AgentId, _: &str) -> Result<String, String> {
            Ok(String::new())
        }
        async fn find_agent_by_name(&self, _: &str) -> Result<Option<AgentId>, String> {
            Ok(None)
        }
        async fn list_agents(&self) -> Result<Vec<(AgentId, String)>, String> {
            Ok(Vec::new())
        }
        async fn spawn_agent_by_name(&self, _: &str) -> Result<AgentId, String> {
            Err("not implemented".into())
        }

        async fn send_channel_message(
            &self,
            channel_type: &str,
            recipient: &str,
            message: &str,
        ) -> Result<(), String> {
            self.calls.lock().unwrap().push((
                channel_type.to_string(),
                recipient.to_string(),
                message.to_string(),
            ));
            if let Some(ref failing) = self.fail_on_channel {
                if failing == channel_type {
                    return Err(format!("mock: forced failure on '{channel_type}'"));
                }
            }
            Ok(())
        }
    }

    fn test_engine(bridge: Arc<MockBridge>) -> CronDeliveryEngine {
        CronDeliveryEngine::new(bridge)
    }

    // -- LocalFile: overwrite ------------------------------------------------

    #[tokio::test]
    async fn localfile_overwrite_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("out.txt");
        let target = CronDeliveryTarget::LocalFile {
            path: path.to_string_lossy().to_string(),
            append: false,
        };

        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[target], "job-x", "hello world").await;

        assert_eq!(results.len(), 1);
        assert!(results[0].success, "error: {:?}", results[0].error);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn localfile_overwrite_replaces_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("replace.txt");
        std::fs::write(&path, "OLD CONTENT").unwrap();

        let target = CronDeliveryTarget::LocalFile {
            path: path.to_string_lossy().to_string(),
            append: false,
        };
        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[target], "job-x", "NEW").await;

        assert!(results[0].success);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "NEW");
    }

    // -- LocalFile: append ---------------------------------------------------

    #[tokio::test]
    async fn localfile_append_adds_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("log.txt");
        let target = CronDeliveryTarget::LocalFile {
            path: path.to_string_lossy().to_string(),
            append: true,
        };
        let engine = test_engine(MockBridge::new());

        // Two sequential deliveries should accumulate.
        engine
            .deliver(std::slice::from_ref(&target), "job", "first")
            .await;
        engine.deliver(&[target], "job", "second").await;

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("first") && content.contains("second"),
            "expected both lines in appended file, got: {content:?}"
        );
    }

    #[tokio::test]
    async fn localfile_append_creates_missing_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/deep/out.log");
        let target = CronDeliveryTarget::LocalFile {
            path: path.to_string_lossy().to_string(),
            append: true,
        };
        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[target], "job", "payload").await;

        assert!(results[0].success, "error: {:?}", results[0].error);
        assert!(path.exists(), "nested file should have been created");
    }

    // -- Webhook: success ----------------------------------------------------

    #[tokio::test]
    async fn webhook_sends_payload() {
        let (port, rx) = spawn_mock_http_server(200, "OK").await;
        let url = format!("http://127.0.0.1:{port}/hook");

        let target = CronDeliveryTarget::Webhook {
            url: url.clone(),
            auth_header: Some("Bearer test-token".to_string()),
        };
        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[target], "daily-report", "result body").await;

        assert!(results[0].success, "error: {:?}", results[0].error);

        let captured = rx.await.expect("mock server never received a request");
        assert!(
            captured.body.contains("\"job\":\"daily-report\""),
            "payload missing job name, got: {}",
            captured.body
        );
        assert!(
            captured.body.contains("\"output\":\"result body\""),
            "payload missing output, got: {}",
            captured.body
        );
        assert!(
            captured.body.contains("\"timestamp\""),
            "payload missing timestamp, got: {}",
            captured.body
        );
        assert!(
            captured
                .headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("authorization: Bearer test-token")),
            "missing auth header, got: {:?}",
            captured.headers
        );
    }

    #[tokio::test]
    async fn webhook_reports_non_2xx() {
        let (port, _rx) = spawn_mock_http_server(500, "Internal Server Error").await;
        let url = format!("http://127.0.0.1:{port}/hook");

        let target = CronDeliveryTarget::Webhook {
            url,
            auth_header: None,
        };
        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[target], "job", "output").await;

        assert!(!results[0].success);
        let err = results[0].error.as_deref().unwrap_or("");
        assert!(err.contains("500"), "expected 500 in error, got: {err}");
    }

    // -- Channel target ------------------------------------------------------

    #[tokio::test]
    async fn channel_target_invokes_bridge() {
        let bridge = MockBridge::new();
        let engine = test_engine(bridge.clone());
        let target = CronDeliveryTarget::Channel {
            channel_type: "slack".to_string(),
            recipient: "C12345".to_string(),
        };
        let results = engine.deliver(&[target], "alerts", "fire").await;
        assert!(results[0].success, "error: {:?}", results[0].error);
        let calls = bridge.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "slack");
        assert_eq!(calls[0].1, "C12345");
        assert_eq!(calls[0].2, "fire");
    }

    // -- Mixed success/failure ----------------------------------------------

    #[tokio::test]
    async fn mixed_targets_one_success_one_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let ok_path = tmp.path().join("ok.txt");

        let targets = vec![
            // Will succeed (file write).
            CronDeliveryTarget::LocalFile {
                path: ok_path.to_string_lossy().to_string(),
                append: false,
            },
            // Will fail (mock bridge rejects 'slack').
            CronDeliveryTarget::Channel {
                channel_type: "slack".to_string(),
                recipient: "C1".to_string(),
            },
        ];

        let bridge = MockBridge::failing_on("slack");
        let engine = test_engine(bridge);
        let results = engine.deliver(&targets, "job", "payload").await;

        assert_eq!(results.len(), 2);
        assert!(
            results[0].success,
            "file delivery should succeed: {:?}",
            results[0].error
        );
        assert!(
            !results[1].success,
            "channel delivery should fail, but got success"
        );
        assert!(results[1]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("forced failure"));

        // File was still written even though the other target failed.
        assert_eq!(std::fs::read_to_string(&ok_path).unwrap(), "payload");
    }

    #[tokio::test]
    async fn empty_targets_returns_empty_vec() {
        let engine = test_engine(MockBridge::new());
        let results = engine.deliver(&[], "job", "x").await;
        assert!(results.is_empty());
    }

    // -- Serde round-trip ---------------------------------------------------

    #[test]
    fn serde_roundtrip_channel() {
        let t = CronDeliveryTarget::Channel {
            channel_type: "telegram".into(),
            recipient: "12345".into(),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"type\":\"channel\""), "tag missing: {s}");
        assert!(s.contains("telegram"));
        let back: CronDeliveryTarget = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn serde_roundtrip_webhook() {
        let t = CronDeliveryTarget::Webhook {
            url: "https://example.com/hook".into(),
            auth_header: Some("Bearer x".into()),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"type\":\"webhook\""), "tag missing: {s}");
        let back: CronDeliveryTarget = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn serde_roundtrip_webhook_without_auth() {
        // auth_header should default to None when omitted.
        let json = r#"{"type":"webhook","url":"https://x.test/h"}"#;
        let back: CronDeliveryTarget = serde_json::from_str(json).unwrap();
        assert_eq!(
            back,
            CronDeliveryTarget::Webhook {
                url: "https://x.test/h".into(),
                auth_header: None,
            }
        );
    }

    #[test]
    fn serde_roundtrip_localfile() {
        let t = CronDeliveryTarget::LocalFile {
            path: "/var/log/cron-out.log".into(),
            append: true,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"type\":\"local_file\""), "tag missing: {s}");
        let back: CronDeliveryTarget = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn serde_roundtrip_localfile_default_append() {
        // append should default to false when omitted.
        let json = r#"{"type":"local_file","path":"/tmp/out.log"}"#;
        let back: CronDeliveryTarget = serde_json::from_str(json).unwrap();
        assert_eq!(
            back,
            CronDeliveryTarget::LocalFile {
                path: "/tmp/out.log".into(),
                append: false,
            }
        );
    }

    #[test]
    fn serde_roundtrip_email() {
        let t = CronDeliveryTarget::Email {
            to: "alice@example.com".into(),
            subject_template: Some("Report: {job}".into()),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"type\":\"email\""), "tag missing: {s}");
        let back: CronDeliveryTarget = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn render_subject_substitutes_placeholder() {
        assert_eq!(render_subject(Some("Cron: {job}"), "daily"), "Cron: daily");
        assert_eq!(
            render_subject(Some("no placeholder"), "x"),
            "no placeholder"
        );
        assert_eq!(render_subject(None, "daily"), "Cron: daily");
        assert_eq!(render_subject(Some(""), "daily"), "Cron: daily");
    }

    // -- Minimal HTTP mock ---------------------------------------------------

    struct CapturedRequest {
        headers: Vec<String>,
        body: String,
    }

    /// Spawn a tiny TCP server that serves exactly one request, parses the
    /// HTTP/1.1 request line + headers + body, then responds with the given
    /// status code and reason phrase. Returns `(port, oneshot_rx)` where the
    /// oneshot resolves once the request has been received.
    async fn spawn_mock_http_server(
        status: u16,
        reason: &'static str,
    ) -> (u16, tokio::sync::oneshot::Receiver<CapturedRequest>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };

            // Read until we have full headers and the declared body.
            let mut buf = Vec::with_capacity(4096);
            let mut tmp = [0u8; 1024];
            let mut headers_end = None;
            let mut content_length: Option<usize> = None;
            loop {
                let n = match stream.read(&mut tmp).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => return,
                };
                buf.extend_from_slice(&tmp[..n]);
                if headers_end.is_none() {
                    if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
                        headers_end = Some(pos + 4);
                        // Parse Content-Length.
                        let head_str = String::from_utf8_lossy(&buf[..pos]);
                        for line in head_str.lines() {
                            if let Some(v) = line.strip_prefix("Content-Length: ") {
                                content_length = v.trim().parse::<usize>().ok();
                            } else if let Some(v) = line.strip_prefix("content-length: ") {
                                content_length = v.trim().parse::<usize>().ok();
                            }
                        }
                    }
                }
                if let (Some(end), Some(cl)) = (headers_end, content_length) {
                    if buf.len() >= end + cl {
                        break;
                    }
                }
                if headers_end.is_some() && content_length.is_none() {
                    break;
                }
            }

            // Split into headers + body.
            let head_end = headers_end.unwrap_or(buf.len());
            let head_str = String::from_utf8_lossy(&buf[..head_end.saturating_sub(4)]).to_string();
            let body_bytes = if head_end < buf.len() {
                &buf[head_end..]
            } else {
                &[][..]
            };
            let body = String::from_utf8_lossy(body_bytes).to_string();
            let headers: Vec<String> = head_str.lines().skip(1).map(|l| l.to_string()).collect();

            // Send response.
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;

            let _ = tx.send(CapturedRequest { headers, body });
        });

        (port, rx)
    }

    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|w| w == needle)
    }
}
