//! Discord Gateway adapter for the OpenFang channel bridge.
//!
//! Uses Discord Gateway WebSocket (v10) for receiving messages and the REST API
//! for sending responses. No external Discord crate — just `tokio-tungstenite` + `reqwest`.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use url::Url;
use zeroize::Zeroizing;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const DISCORD_MSG_LIMIT: usize = 2000;
/// Multipart field name Discord requires for the first attachment payload.
/// Test-only fixture: the production call site uses `format!("files[{i}]")`
/// directly; this constant pins the wire format in
/// `test_attachment_field_name_pinned` so a `file[0]` typo or future
/// refactor can't slip past review.
#[cfg(test)]
const ATTACHMENT_FIELD_NAME: &str = "files[0]";
/// Floor on the rate-limit retry delay. Discord occasionally returns
/// `retry_after: 0` (or a missing header), which would busy-loop the retry.
const RETRY_AFTER_FLOOR_SECS: f64 = 0.05;
/// Cap on the rate-limit retry delay so a misbehaving response can't park us
/// for a long time on a one-shot retry.
const RETRY_AFTER_CEIL_SECS: f64 = 30.0;
/// Per-request timeout for outbound URL fetches (File/Image arms). Matches the
/// adapter's existing 15s budget for other REST calls so a slow remote can't
/// stall the send pipeline.
const URL_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Hard cap on the size of a fetched URL body, both via Content-Length pre-flight
/// and via streamed accumulation. Discord itself caps non-Nitro uploads at
/// 25 MiB; matching that here means we reject before paying for bytes Discord
/// would refuse anyway.
const URL_FETCH_MAX_BYTES: usize = 25 * 1024 * 1024;
/// Maximum number of attachments per multipart POST. Discord's REST API caps
/// `files[N]` at 10 per request; the multipart helper relies on the caller
/// having pre-chunked.
const ATTACHMENTS_PER_CHUNK: usize = 10;
/// Aggregate byte cap on a single multipart POST's attachment payload.
/// Discord caps non-Nitro requests at 25 MiB total (multipart envelope +
/// payload_json + every `files[i]`); 24 MiB leaves ~1 MiB of headroom for the
/// envelope so an over-budget attempt can't silently 413. Files larger than
/// this cap end up in their own single-attachment chunk; Discord still
/// rejects them, but the caller sees the same error they did before this
/// cap landed.
const CHUNK_TOTAL_CAP_BYTES: usize = 24 * 1024 * 1024;
/// Cap on the number of HTTP redirects we'll follow on URL fetches. Each hop
/// is independently SSRF-rechecked at the literal-IP level.
const URL_FETCH_MAX_REDIRECTS: usize = 3;
/// User-Agent we identify as on outbound URL fetches. Pinned so a future test
/// can assert on it; remote operators looking at access logs see a single
/// stable identifier instead of reqwest's default.
const URL_FETCH_USER_AGENT: &str = concat!("openfang-channels-discord/", env!("CARGO_PKG_VERSION"));

/// Discord Gateway opcodes.
mod opcode {
    pub const DISPATCH: u64 = 0;
    pub const HEARTBEAT: u64 = 1;
    pub const IDENTIFY: u64 = 2;
    pub const RESUME: u64 = 6;
    pub const RECONNECT: u64 = 7;
    pub const INVALID_SESSION: u64 = 9;
    pub const HELLO: u64 = 10;
    pub const HEARTBEAT_ACK: u64 = 11;
}

/// Build a Discord gateway heartbeat (opcode 1) payload.
///
/// Per the Discord gateway spec, the payload `d` field is the last received
/// dispatch sequence number, or `null` if no dispatch has been received yet.
/// See: <https://discord.com/developers/docs/topics/gateway#sending-heartbeats>
fn build_heartbeat_payload(last_sequence: Option<u64>) -> serde_json::Value {
    serde_json::json!({
        "op": opcode::HEARTBEAT,
        "d": last_sequence,
    })
}

/// Format a URL for log/error messages with the query string and fragment
/// stripped. Discord CDN URLs carry HMAC-style query params (`ex`, `is`, `hm`,
/// `__cf_bm`) that grant time-limited access; logging them at warn/error level
/// would leak credential-equivalent material into operator log aggregators.
fn redact_url(u: &Url) -> String {
    format!(
        "{}://{}{}",
        u.scheme(),
        u.host_str().unwrap_or(""),
        u.path()
    )
}

/// Returns true if the IP address is one we refuse to fetch from to prevent
/// SSRF: loopback, RFC1918 / link-local / unique-local, multicast, unspecified,
/// or the literal cloud-metadata IP. IPv4-mapped IPv6 addresses are unwrapped
/// to their underlying v4 and re-checked.
fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => {
            // Strip IPv4-mapped wrappers (::ffff:a.b.c.d) before re-checking.
            // `Ipv6Addr::to_ipv4_mapped` is stable as of 1.63 but we use the
            // older `to_ipv4` which also covers IPv4-compatible ::a.b.c.d.
            if let Some(v4) = v6.to_ipv4() {
                if is_blocked_v4(v4) {
                    return true;
                }
            }
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return true;
            }
            // Link-local fe80::/10. `Ipv6Addr::is_unicast_link_local` is
            // unstable; check the prefix manually.
            let seg0 = v6.segments()[0];
            if (seg0 & 0xffc0) == 0xfe80 {
                return true;
            }
            // Unique local fc00::/7.
            if (seg0 & 0xfe00) == 0xfc00 {
                return true;
            }
            false
        }
    }
}

fn is_blocked_v4(v4: Ipv4Addr) -> bool {
    if v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_multicast()
        || v4.is_broadcast()
    {
        return true;
    }
    // 169.254.169.254 is technically link-local (covered above) but make the
    // intent explicit — a future stdlib change to `is_link_local` shouldn't
    // silently re-open cloud-metadata exfiltration.
    if v4.octets() == [169, 254, 169, 254] {
        return true;
    }
    // Carrier-grade NAT 100.64.0.0/10. Not in `Ipv4Addr::is_private` but is
    // commonly internal.
    if v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40 {
        return true;
    }
    false
}

/// Synchronous SSRF check on a parsed URL: scheme allowlist + literal-IP host
/// range check. Hostname (DNS) resolution is the caller's responsibility (see
/// [`resolve_and_check_host`]); this function intentionally avoids DNS so it
/// can run inside the sync `redirect::Policy::custom` callback on every hop.
///
/// Threat model note: the redirect callback can only do this literal-IP
/// recheck, not DNS, because reqwest's redirect policy is sync. A malicious
/// DNS server that returns a public IP at first lookup and a private IP on a
/// second lookup is *not* in the threat model — the threat is a malicious URL
/// the agent was tricked into emitting. Literal-IP redirects are still
/// blocked at every hop, which closes the most obvious bypass.
fn check_url_scheme_and_literal_ip(u: &Url) -> Result<(), String> {
    match u.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "URL fetch refused: scheme {other:?} not allowed (need http/https) for {}",
                redact_url(u)
            ));
        }
    }
    if let Some(host) = u.host() {
        match host {
            url::Host::Ipv4(v4) => {
                if is_blocked_v4(v4) {
                    return Err(format!(
                        "URL fetch refused: blocked IPv4 host for {}",
                        redact_url(u)
                    ));
                }
            }
            url::Host::Ipv6(v6) => {
                if is_blocked_ip(IpAddr::V6(v6)) {
                    return Err(format!(
                        "URL fetch refused: blocked IPv6 host for {}",
                        redact_url(u)
                    ));
                }
            }
            url::Host::Domain(_) => {}
        }
    } else {
        return Err(format!(
            "URL fetch refused: missing host for {}",
            redact_url(u)
        ));
    }
    Ok(())
}

/// Typed intermediate produced by the single classification pass over a
/// `ChannelContent::Multipart`'s blocks. Carrying enough information per
/// variant lets the subsequent resolve step operate on this enum alone
/// without a second walk over the original `Vec<ChannelContent>`.
enum AttachmentSource {
    /// Already fully-resolved attachment (came from a `FileData` block).
    Resolved {
        bytes: bytes::Bytes,
        filename: String,
        mime: String,
    },
    /// URL-backed image; `Fetcher` resolves the bytes, then
    /// `resolve_image_mime` / `resolve_image_filename` derive the metadata.
    UrlImage { url: String },
    /// URL-backed file with caller-supplied filename/mime hints; `Fetcher`
    /// resolves the bytes, then `resolve_file_mime` / `resolve_file_filename`
    /// reconcile against the response Content-Type.
    UrlFile {
        url: String,
        filename: String,
        mime: Option<String>,
    },
}

/// Abstraction over "fetch a URL into memory" so production and tests share
/// the same wire-level HTTP code while differing only in whether SSRF
/// validation runs first.
///
/// - [`ProductionFetcher`] performs scheme + DNS-resolved IP checks via
///   [`resolve_and_check_host`] before issuing the request.
/// - [`PermissiveFetcher`] (test-only) skips the SSRF preflight so tests
///   can hit `127.0.0.1` fixture servers via the same wire path.
///
/// Returns the body as `Bytes` plus the response's `Content-Type` with any
/// MIME parameters (e.g. `; charset=utf-8`) stripped.
#[async_trait]
trait Fetcher: Send + Sync {
    async fn fetch(
        &self,
        url: &str,
    ) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>>;
}

/// Production fetcher: parses the URL, runs the SSRF preflight, then performs
/// the HTTP fetch via [`do_http_fetch`].
struct ProductionFetcher;

#[async_trait]
impl Fetcher for ProductionFetcher {
    async fn fetch(
        &self,
        url: &str,
    ) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>> {
        let parsed = Url::parse(url).map_err(|e| format!("URL fetch refused: parse error: {e}"))?;
        resolve_and_check_host(&parsed).await?;
        do_http_fetch(&parsed).await
    }
}

/// Test-only fetcher that performs the same wire fetch but skips the SSRF
/// preflight, so tests can point `Image{url}` / `File{url}` blocks at local
/// stub servers without bypassing the production code path.
#[cfg(test)]
struct PermissiveFetcher;

#[cfg(test)]
#[async_trait]
impl Fetcher for PermissiveFetcher {
    async fn fetch(
        &self,
        url: &str,
    ) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>> {
        let parsed = Url::parse(url).map_err(|e| format!("URL fetch refused: parse error: {e}"))?;
        do_http_fetch(&parsed).await
    }
}

/// Resolve the URL's host (DNS if hostname; identity if IP literal) and reject
/// if any resolved address fails the SSRF check. Performs both the scheme
/// check and the per-IP check.
async fn resolve_and_check_host(u: &Url) -> Result<(), String> {
    check_url_scheme_and_literal_ip(u)?;
    let host = match u.host() {
        Some(url::Host::Domain(d)) => d.to_string(),
        // IP literals already passed the literal-IP check above; no DNS needed.
        Some(_) => return Ok(()),
        None => {
            return Err(format!(
                "URL fetch refused: missing host for {}",
                redact_url(u)
            ))
        }
    };
    let port = u.port_or_known_default().unwrap_or(0);
    let hostport = format!("{host}:{port}");
    let addrs = tokio::net::lookup_host(hostport.as_str())
        .await
        .map_err(|_| format!("URL fetch refused: DNS lookup failed for {}", redact_url(u)))?;
    for sa in addrs {
        if is_blocked_ip(sa.ip()) {
            return Err(format!(
                "URL fetch refused: host resolved to blocked address for {}",
                redact_url(u)
            ));
        }
    }
    Ok(())
}

/// Discord Gateway adapter using WebSocket.
pub struct DiscordAdapter {
    /// SECURITY: Bot token is zeroized on drop to prevent memory disclosure.
    token: Zeroizing<String>,
    client: reqwest::Client,
    allowed_guilds: Vec<String>,
    allowed_users: Vec<String>,
    ignore_bots: bool,
    intents: u64,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own user ID (populated after READY event).
    bot_user_id: Arc<RwLock<Option<String>>>,
    /// Session ID for resume (populated after READY event).
    session_id: Arc<RwLock<Option<String>>>,
    /// Resume gateway URL.
    resume_gateway_url: Arc<RwLock<Option<String>>>,
    /// Override for the Discord REST API base URL. `None` in production (uses
    /// `DISCORD_API_BASE`). Set by tests that spin up a local stub server.
    #[cfg(test)]
    api_base_override: Option<String>,
    /// Resolver for outbound URL fetches (`Image{url}` / `File{url}`). In
    /// production this is [`ProductionFetcher`] which runs the SSRF preflight;
    /// tests can swap in [`PermissiveFetcher`] to point at local stubs without
    /// bypassing the wire path.
    fetcher: Arc<dyn Fetcher>,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        allowed_guilds: Vec<String>,
        allowed_users: Vec<String>,
        ignore_bots: bool,
        intents: u64,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            allowed_guilds,
            allowed_users,
            ignore_bots,
            intents,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
            session_id: Arc::new(RwLock::new(None)),
            resume_gateway_url: Arc::new(RwLock::new(None)),
            #[cfg(test)]
            api_base_override: None,
            fetcher: Arc::new(ProductionFetcher),
        }
    }

    /// Returns the Discord REST API base URL, honouring the test override when
    /// present. In production this is always `DISCORD_API_BASE`.
    #[cfg(test)]
    fn api_base(&self) -> &str {
        self.api_base_override
            .as_deref()
            .unwrap_or(DISCORD_API_BASE)
    }

    #[cfg(not(test))]
    #[inline(always)]
    fn api_base(&self) -> &str {
        DISCORD_API_BASE
    }

    /// Get the WebSocket gateway URL from the Discord API.
    async fn get_gateway_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{DISCORD_API_BASE}/gateway/bot");
        let resp: serde_json::Value = self
            .client
            .get(&url)
            .header("Authorization", format!("Bot {}", self.token.as_str()))
            .send()
            .await?
            .json()
            .await?;

        let ws_url = resp["url"]
            .as_str()
            .ok_or("Missing 'url' in gateway response")?;

        Ok(format!("{ws_url}/?v=10&encoding=json"))
    }

    /// Send a message to a Discord channel via REST API.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{channel_id}/messages", self.api_base());
        let chunks = split_message(text, DISCORD_MSG_LIMIT);

        for chunk in chunks {
            let body = serde_json::json!({ "content": chunk });
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.token.as_str()))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                warn!("Discord sendMessage failed: {body_text}");
            }
        }
        Ok(())
    }

    /// Send a file attachment to a Discord channel via REST multipart upload.
    ///
    /// Thin wrapper around `api_send_attachments` for the common single-file
    /// case. `Bytes::clone` is a refcount bump so passing through is free.
    async fn api_send_attachment(
        &self,
        channel_id: &str,
        data: impl Into<bytes::Bytes>,
        filename: &str,
        mime_type: &str,
        caption: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_attachments(
            channel_id,
            vec![(data.into(), filename.to_string(), mime_type.to_string())],
            caption,
        )
        .await
    }

    /// Send one or more file attachments in a single multipart POST.
    ///
    /// Builds a `multipart/form-data` request with `payload_json` plus
    /// `files[0]`…`files[N-1]` parts (N ≤ 10, per Discord's limit). The
    /// caller is responsible for chunking larger batches.
    ///
    /// On HTTP 429 we honor `Retry-After` once before giving up. Higher-tier
    /// rate-limit handling can land later if needed.
    async fn api_send_attachments(
        &self,
        channel_id: &str,
        attachments: Vec<(bytes::Bytes, String, String)>,
        caption: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{channel_id}/messages", self.api_base());

        // Discord caps message content at DISCORD_MSG_LIMIT chars; truncate
        // explicitly so a long caption doesn't silently 400.
        let payload_json = build_attachment_payload_json(caption);

        // Pre-compute lengths. `Bytes::clone` is a refcount bump so the
        // retry-path form rebuild is allocation-free for the file data.
        let parts_meta: Vec<(bytes::Bytes, u64, String, String)> = attachments
            .into_iter()
            .map(|(b, name, mime)| {
                let len = b.len() as u64;
                (b, len, name, mime)
            })
            .collect();

        let build_form = || -> Result<reqwest::multipart::Form, Box<dyn std::error::Error>> {
            let mut form =
                reqwest::multipart::Form::new().text("payload_json", payload_json.clone());
            for (i, (bytes, body_len, filename, mime_type)) in parts_meta.iter().enumerate() {
                let body = reqwest::Body::from(bytes.clone());
                let file_part = reqwest::multipart::Part::stream_with_length(body, *body_len)
                    .file_name(filename.clone())
                    .mime_str(mime_type)?;
                // Discord requires field names `files[0]`, `files[1]`, etc.
                // The wire format is pinned by `test_attachment_field_name_pinned`
                // (asserts `format!("files[{}]", 0) == ATTACHMENT_FIELD_NAME`).
                let field_name = format!("files[{i}]");
                form = form.part(field_name, file_part);
            }
            Ok(form)
        };

        let mut attempts = 0u8;
        loop {
            attempts += 1;
            let form = build_form()?;
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.token.as_str()))
                .multipart(form)
                .send()
                .await?;

            let status = resp.status();
            if status.is_success() {
                return Ok(());
            }

            // Honor Retry-After once on 429. Discord puts the canonical
            // `retry_after` in the JSON body; the HTTP header is a fallback
            // (and is sometimes absent on per-route limits).
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempts == 1 {
                let header_secs = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<f64>().ok());
                let body_text = resp.text().await.unwrap_or_default();
                let body_secs = serde_json::from_str::<serde_json::Value>(&body_text)
                    .ok()
                    .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()));
                let retry_after_secs = body_secs
                    .or(header_secs)
                    .unwrap_or(1.0)
                    .clamp(RETRY_AFTER_FLOOR_SECS, RETRY_AFTER_CEIL_SECS);
                warn!(
                    "Discord sendAttachments rate-limited; retrying after {retry_after_secs:.2}s"
                );
                tokio::time::sleep(Duration::from_millis((retry_after_secs * 1000.0) as u64)).await;
                continue;
            }

            let body_text = resp.text().await.unwrap_or_default();
            warn!("Discord sendAttachments failed ({status}): {body_text}");
            return Err(format!("Discord sendAttachments failed ({status}): {body_text}").into());
        }
    }

    /// Resolve a single [`AttachmentSource`] into the
    /// `(bytes, filename, mime)` tuple consumed by `api_send_attachments`.
    ///
    /// `Resolved` returns immediately; URL variants delegate to `Fetcher` and
    /// then run their respective resolver chains. Errors are wrapped with the
    /// `"Multipart fetch failed for {url}: …"` prefix the existing tests pin.
    ///
    /// The error type is `Box<dyn Error + Send + Sync>` (not the looser
    /// `Box<dyn Error>` used elsewhere) so the resulting future is `Send` —
    /// required by `try_join_all` in the multipart resolve step. The
    /// conversion to `Box<dyn Error>` happens implicitly at the call site
    /// via `?`.
    async fn resolve_attachment_source(
        &self,
        source: AttachmentSource,
    ) -> Result<(bytes::Bytes, String, String), Box<dyn std::error::Error + Send + Sync>> {
        match source {
            AttachmentSource::Resolved {
                bytes,
                filename,
                mime,
            } => Ok((bytes, filename, mime)),
            AttachmentSource::UrlImage { url } => {
                // `Fetcher::fetch` returns `Box<dyn Error>` (no Send);
                // stringify so the error becomes `Send + Sync` for `?`.
                let (bytes, response_ct) = self
                    .fetcher
                    .fetch(&url)
                    .await
                    .map_err(|e| format!("Multipart fetch failed for {url}: {e}"))?;
                let resolved_mime = resolve_image_mime(response_ct.as_deref(), &url);
                let resolved_filename = resolve_image_filename(&url, &resolved_mime);
                Ok((bytes, resolved_filename, resolved_mime))
            }
            AttachmentSource::UrlFile {
                url,
                filename,
                mime,
            } => {
                let (bytes, response_ct) = self
                    .fetcher
                    .fetch(&url)
                    .await
                    .map_err(|e| format!("Multipart fetch failed for {url}: {e}"))?;
                let resolved_filename = resolve_file_filename(Some(filename.as_str()), &url);
                let resolved_mime = resolve_file_mime(
                    mime.as_deref(),
                    response_ct.as_deref(),
                    &resolved_filename,
                );
                Ok((bytes, resolved_filename, resolved_mime))
            }
        }
    }

    /// Send typing indicator to a Discord channel.
    async fn api_send_typing(&self, channel_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{channel_id}/typing", self.api_base());
        let _ = self
            .client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.token.as_str()))
            .send()
            .await?;
        Ok(())
    }
}

/// Wire-level HTTP fetch shared by [`ProductionFetcher`] and
/// [`PermissiveFetcher`]. Assumes the caller has already done any SSRF
/// preflight on `parsed`. Performs:
///
///   1. Per-request reqwest client with a redirect policy that caps at
///      [`URL_FETCH_MAX_REDIRECTS`] hops and re-applies the literal-IP SSRF
///      check on every hop's URL.
///   2. Two-stage size enforcement: Content-Length pre-flight, then streaming
///      chunk accumulation that aborts mid-stream on overrun.
///
/// Errors are scrubbed via [`redact_url`] so Discord CDN HMAC params don't
/// land in operator logs.
async fn do_http_fetch(
    parsed: &Url,
) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>> {
    // Per-request client with a custom redirect policy. We cannot reuse
    // a shared client because its redirect policy is fixed at build time.
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= URL_FETCH_MAX_REDIRECTS {
            return attempt.error(format!("redirect cap ({URL_FETCH_MAX_REDIRECTS}) exceeded"));
        }
        // Sync context: we can only do the literal-IP recheck here; DNS
        // requires async. The original hostname was DNS-checked before
        // the request started, so the only new bypass to close at this
        // layer is a redirect to a literal private IP.
        if let Err(e) = check_url_scheme_and_literal_ip(attempt.url()) {
            return attempt.error(e);
        }
        attempt.follow()
    });
    let client = reqwest::Client::builder()
        .redirect(redirect_policy)
        .user_agent(URL_FETCH_USER_AGENT)
        .timeout(URL_FETCH_TIMEOUT)
        .build()?;

    let resp = client.get(parsed.as_str()).send().await.map_err(|e| {
        // reqwest's Display impl for Error includes the URL it was
        // fetching (with query string). Replace it with the redacted
        // form to keep CDN HMAC params out of error logs.
        //
        // For redirect-policy errors, reqwest's outer Display is the
        // generic "error following redirect"; the actual cause (e.g.
        // "URL fetch refused: blocked IPv4 host for ...") lives in the
        // source chain. Walk it so the operator sees *why* we refused.
        let stripped = e.without_url();
        let mut msg = stripped.to_string();
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(&stripped);
        while let Some(s) = src {
            use std::fmt::Write as _;
            let _ = write!(msg, ": {s}");
            src = s.source();
        }
        format!("URL fetch failed for {}: {msg}", redact_url(parsed))
    })?;

    let status = resp.status();
    if !status.is_success() {
        // Read up to 512B of the body for diagnostics; ignore errors.
        let snippet: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(512)
            .collect();
        return Err(format!(
            "URL fetch failed ({status}) for {}: {snippet}",
            redact_url(parsed)
        )
        .into());
    }

    // Pre-flight: trust Content-Length when present so we can fail fast
    // without buffering 26 MiB before erroring.
    let content_length = resp.content_length();
    if let Some(len) = content_length {
        if len as usize > URL_FETCH_MAX_BYTES {
            return Err(format!(
                "URL fetch refused: Content-Length {len} exceeds cap {URL_FETCH_MAX_BYTES} for {}",
                redact_url(parsed)
            )
            .into());
        }
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(strip_mime_params)
        .filter(|s| !s.is_empty());

    // Pre-size the buffer: if we have a trustworthy Content-Length, use it
    // (clamped to the cap); otherwise start at 64 KiB so the happy path
    // doesn't pay ~24 doublings on a 25 MiB body.
    let initial_cap = std::cmp::min(
        content_length.unwrap_or(64 * 1024) as usize,
        URL_FETCH_MAX_BYTES,
    );
    let mut buf = bytes::BytesMut::with_capacity(initial_cap);
    let mut resp = resp;
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() + chunk.len() > URL_FETCH_MAX_BYTES {
            return Err(format!(
                "URL fetch refused: streamed body exceeds cap {URL_FETCH_MAX_BYTES} for {}",
                redact_url(parsed)
            )
            .into());
        }
        buf.extend_from_slice(&chunk);
    }

    Ok((buf.freeze(), content_type))
}

#[async_trait]
impl ChannelAdapter for DiscordAdapter {
    fn name(&self) -> &str {
        "discord"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let gateway_url = self.get_gateway_url().await?;
        info!("Discord gateway URL obtained");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let token = self.token.clone();
        let intents = self.intents;
        let allowed_guilds = self.allowed_guilds.clone();
        let allowed_users = self.allowed_users.clone();
        let ignore_bots = self.ignore_bots;
        let bot_user_id = self.bot_user_id.clone();
        let session_id_store = self.session_id.clone();
        let resume_url_store = self.resume_gateway_url.clone();
        let mut shutdown = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;
            let mut connect_url = gateway_url;
            // Sequence persists across reconnections for RESUME
            let sequence: Arc<RwLock<Option<u64>>> = Arc::new(RwLock::new(None));

            loop {
                if *shutdown.borrow() {
                    break;
                }

                info!("Connecting to Discord gateway...");

                let ws_result = tokio_tungstenite::connect_async(&connect_url).await;
                let ws_stream = match ws_result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!("Discord gateway connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                backoff = INITIAL_BACKOFF;
                info!("Discord gateway connected");

                let (ws_tx_raw, mut ws_rx) = ws_stream.split();
                // Wrap the sink so the periodic heartbeat task and the inner
                // loop can both write to it.
                let ws_tx = Arc::new(Mutex::new(ws_tx_raw));
                let mut heartbeat_handle: Option<JoinHandle<()>> = None;
                // Tracks whether the most recent heartbeat we sent has been
                // ACKed (opcode 11). Initialized to `true` so the first
                // heartbeat is always allowed to fire.
                let heartbeat_acked = Arc::new(AtomicBool::new(true));

                // Inner message loop — returns true if we should reconnect
                let should_reconnect = 'inner: loop {
                    let msg = tokio::select! {
                        msg = ws_rx.next() => msg,
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                info!("Discord shutdown requested");
                                if let Some(h) = heartbeat_handle.take() {
                                    h.abort();
                                }
                                let _ = ws_tx.lock().await.close().await;
                                return;
                            }
                            continue;
                        }
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Discord WebSocket error: {e}");
                            break 'inner true;
                        }
                        None => {
                            info!("Discord WebSocket closed");
                            break 'inner true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("Discord gateway closed by server");
                            break 'inner true;
                        }
                        _ => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Discord: failed to parse gateway message: {e}");
                            continue;
                        }
                    };

                    let op = payload["op"].as_u64().unwrap_or(999);

                    // Update sequence number from any payload that carries one
                    // (typically dispatch events, opcode 0).
                    if let Some(s) = payload["s"].as_u64() {
                        *sequence.write().await = Some(s);
                    }

                    match op {
                        opcode::HELLO => {
                            let interval =
                                payload["d"]["heartbeat_interval"].as_u64().unwrap_or(45000);
                            debug!("Discord HELLO: heartbeat_interval={interval}ms");

                            // Spawn the periodic heartbeat task BEFORE we send
                            // IDENTIFY/RESUME, per the Discord gateway flow.
                            // Abort any stale handle from a previous attempt
                            // first (defensive — should normally be None here).
                            if let Some(h) = heartbeat_handle.take() {
                                h.abort();
                            }
                            heartbeat_acked.store(true, Ordering::Relaxed);
                            let hb_sink = ws_tx.clone();
                            let hb_seq = sequence.clone();
                            let hb_acked = heartbeat_acked.clone();
                            let mut hb_shutdown = shutdown.clone();
                            heartbeat_handle = Some(tokio::spawn(async move {
                                let mut ticker =
                                    tokio::time::interval(Duration::from_millis(interval));
                                // Skip the immediate first tick — we want to
                                // wait one full interval before the first beat.
                                ticker.tick().await;
                                loop {
                                    tokio::select! {
                                        _ = ticker.tick() => {}
                                        _ = hb_shutdown.changed() => {
                                            if *hb_shutdown.borrow() {
                                                return;
                                            }
                                            continue;
                                        }
                                    }

                                    // If the previous heartbeat was never
                                    // ACKed, the connection is zombied — close
                                    // the sink so the read loop sees EOF and
                                    // triggers a reconnect (Discord spec).
                                    if !hb_acked.swap(false, Ordering::Relaxed) {
                                        warn!(
                                            "Discord: previous heartbeat not ACKed, \
                                             forcing reconnect"
                                        );
                                        let _ = hb_sink.lock().await.close().await;
                                        return;
                                    }

                                    let seq = *hb_seq.read().await;
                                    let payload = build_heartbeat_payload(seq);
                                    let text = match serde_json::to_string(&payload) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            error!("Discord: failed to serialize heartbeat: {e}");
                                            return;
                                        }
                                    };
                                    let send_res = hb_sink
                                        .lock()
                                        .await
                                        .send(tokio_tungstenite::tungstenite::Message::Text(text))
                                        .await;
                                    if let Err(e) = send_res {
                                        warn!("Discord: failed to send heartbeat: {e}");
                                        return;
                                    }
                                    debug!("Discord heartbeat sent (seq={:?})", seq);
                                }
                            }));

                            // Try RESUME if we have a session, otherwise IDENTIFY
                            let has_session = session_id_store.read().await.is_some();
                            let has_seq = sequence.read().await.is_some();

                            let gateway_msg = if has_session && has_seq {
                                let sid = session_id_store.read().await.clone().unwrap();
                                let seq = *sequence.read().await;
                                info!("Discord: sending RESUME (session={sid})");
                                serde_json::json!({
                                    "op": opcode::RESUME,
                                    "d": {
                                        "token": token.as_str(),
                                        "session_id": sid,
                                        "seq": seq
                                    }
                                })
                            } else {
                                info!("Discord: sending IDENTIFY");
                                serde_json::json!({
                                    "op": opcode::IDENTIFY,
                                    "d": {
                                        "token": token.as_str(),
                                        "intents": intents,
                                        "properties": {
                                            "os": "linux",
                                            "browser": "openfang",
                                            "device": "openfang"
                                        }
                                    }
                                })
                            };

                            if let Err(e) = ws_tx
                                .lock()
                                .await
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&gateway_msg).unwrap(),
                                ))
                                .await
                            {
                                error!("Discord: failed to send IDENTIFY/RESUME: {e}");
                                break 'inner true;
                            }
                        }

                        opcode::DISPATCH => {
                            let event_name = payload["t"].as_str().unwrap_or("");
                            let d = &payload["d"];

                            match event_name {
                                "READY" => {
                                    let user_id =
                                        d["user"]["id"].as_str().unwrap_or("").to_string();
                                    let username =
                                        d["user"]["username"].as_str().unwrap_or("unknown");
                                    let sid = d["session_id"].as_str().unwrap_or("").to_string();
                                    let resume_url =
                                        d["resume_gateway_url"].as_str().unwrap_or("").to_string();

                                    *bot_user_id.write().await = Some(user_id.clone());
                                    *session_id_store.write().await = Some(sid);
                                    if !resume_url.is_empty() {
                                        *resume_url_store.write().await = Some(resume_url);
                                    }

                                    info!("Discord bot ready: {username} ({user_id})");
                                }

                                "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                                    if let Some(msg) = parse_discord_message(
                                        d,
                                        &bot_user_id,
                                        &allowed_guilds,
                                        &allowed_users,
                                        ignore_bots,
                                    )
                                    .await
                                    {
                                        debug!(
                                            "Discord {event_name} from {}: {:?}",
                                            msg.sender.display_name, msg.content
                                        );
                                        if tx.send(msg).await.is_err() {
                                            return;
                                        }
                                    }
                                }

                                "RESUMED" => {
                                    info!("Discord session resumed successfully");
                                }

                                _ => {
                                    debug!("Discord event: {event_name}");
                                }
                            }
                        }

                        opcode::HEARTBEAT => {
                            // Server requests immediate heartbeat
                            let seq = *sequence.read().await;
                            let hb = build_heartbeat_payload(seq);
                            let _ = ws_tx
                                .lock()
                                .await
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&hb).unwrap(),
                                ))
                                .await;
                            // The server-requested heartbeat counts as a fresh
                            // beat — reset the ACK gate so the periodic task
                            // doesn't see a stale "unacked" flag.
                            heartbeat_acked.store(false, Ordering::Relaxed);
                        }

                        opcode::HEARTBEAT_ACK => {
                            debug!("Discord heartbeat ACK received");
                            heartbeat_acked.store(true, Ordering::Relaxed);
                        }

                        opcode::RECONNECT => {
                            info!("Discord: server requested reconnect");
                            break 'inner true;
                        }

                        opcode::INVALID_SESSION => {
                            let resumable = payload["d"].as_bool().unwrap_or(false);
                            if resumable {
                                info!("Discord: invalid session (resumable)");
                            } else {
                                info!("Discord: invalid session (not resumable), clearing session");
                                *session_id_store.write().await = None;
                                *sequence.write().await = None;
                            }
                            break 'inner true;
                        }

                        _ => {
                            debug!("Discord: unknown opcode {op}");
                        }
                    }
                };

                // Tear down the heartbeat task before we either exit or
                // reconnect, so it doesn't outlive its WebSocket sink.
                if let Some(h) = heartbeat_handle.take() {
                    h.abort();
                }

                if !should_reconnect || *shutdown.borrow() {
                    break;
                }

                // Try resume URL if available
                if let Some(ref url) = *resume_url_store.read().await {
                    connect_url = format!("{url}/?v=10&encoding=json");
                }

                warn!("Discord: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("Discord gateway loop stopped");
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // platform_id is the channel_id for Discord
        let channel_id = &user.platform_id;
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(channel_id, &text).await?;
            }
            ChannelContent::FileData {
                data,
                filename,
                mime_type,
            } => {
                self.api_send_attachment(channel_id, data, &filename, &mime_type, None)
                    .await?;
            }
            ChannelContent::File {
                url,
                filename,
                mime,
                size: _,
            } => {
                // Fetch then route through the existing multipart helper.
                // `Fetcher::fetch` enforces SSRF + 15s timeout + 25 MiB cap.
                let (bytes, response_ct) = self.fetcher.fetch(&url).await?;
                let resolved_filename = resolve_file_filename(Some(filename.as_str()), &url);
                let resolved_mime =
                    resolve_file_mime(mime.as_deref(), response_ct.as_deref(), &resolved_filename);
                // No caption on a bare File; captions travel via Multipart([Text, File]).
                self.api_send_attachment(
                    channel_id,
                    bytes,
                    &resolved_filename,
                    &resolved_mime,
                    None,
                )
                .await?;
            }
            ChannelContent::Image { url, caption } => {
                let (bytes, response_ct) = self.fetcher.fetch(&url).await?;
                let resolved_mime = resolve_image_mime(response_ct.as_deref(), &url);
                let resolved_filename = resolve_image_filename(&url, &resolved_mime);
                let caption_ref = caption.as_deref().filter(|s| !s.is_empty());
                self.api_send_attachment(
                    channel_id,
                    bytes,
                    &resolved_filename,
                    &resolved_mime,
                    caption_ref,
                )
                .await?;
            }
            ChannelContent::Multipart(parts) => {
                // Single pass over `parts`: bucket each block into caption
                // pieces, an `AttachmentSource` for later resolution, or a
                // logged-unknown name. The two-pass classify/resolve split is
                // collapsed by carrying enough info on `AttachmentSource` for
                // the resolve step to operate on the typed intermediate alone.
                let mut caption_pieces: Vec<String> = Vec::new();
                let mut sources: Vec<AttachmentSource> = Vec::with_capacity(parts.len());
                let mut unknown_names: Vec<&str> = Vec::new();

                for part in parts {
                    match part {
                        ChannelContent::Text(t) => caption_pieces.push(t),
                        ChannelContent::FileData {
                            data,
                            filename,
                            mime_type,
                        } => sources.push(AttachmentSource::Resolved {
                            bytes: bytes::Bytes::from(data),
                            filename,
                            mime: mime_type,
                        }),
                        // Per-Image inner captions are ignored inside Multipart;
                        // the outer caption_pieces form the single caption.
                        ChannelContent::Image { url, caption: _ } => {
                            sources.push(AttachmentSource::UrlImage { url })
                        }
                        ChannelContent::File {
                            url,
                            filename,
                            mime,
                            size: _,
                        } => sources.push(AttachmentSource::UrlFile {
                            url,
                            filename,
                            mime,
                        }),
                        ChannelContent::Voice { .. } => unknown_names.push("Voice"),
                        ChannelContent::Location { .. } => unknown_names.push("Location"),
                        ChannelContent::Command { .. } => unknown_names.push("Command"),
                        ChannelContent::Multipart(_) => unknown_names.push("Multipart"),
                    }
                }

                if !unknown_names.is_empty() {
                    warn!(
                        "Discord Multipart: skipping unknown/unsupported nested variant(s): {:?}",
                        unknown_names
                    );
                }

                // Build the single caption string from all Text blocks.
                let caption_str = caption_pieces.join("\n\n");
                let caption_str = caption_str.trim();
                let caption_opt: Option<&str> = if caption_str.is_empty() {
                    None
                } else {
                    Some(caption_str)
                };

                // Resolve sources concurrently. `try_join_all` preserves
                // input order in the output Vec (so `files[i]` still lines
                // up with the source's original position) and fails fast on
                // the first error, cancelling the rest — matching the spec
                // and the previous serial behavior. For an N-URL Multipart
                // this drops latency from sum-of-RTTs to max-of-RTT.
                let attachments_resolved: Vec<(bytes::Bytes, String, String)> =
                    futures::future::try_join_all(
                        sources
                            .into_iter()
                            .map(|s| self.resolve_attachment_source(s)),
                    )
                    .await
                    // Widen `Box<dyn Error + Send + Sync>` (needed so the
                    // resolve future is `Send` for `try_join_all`) to the
                    // looser `Box<dyn Error>` returned by `send`. Unsizing
                    // a trait object by removing auto traits is allowed but
                    // not exposed via `From`, so we coerce explicitly.
                    .map_err(|e| -> Box<dyn std::error::Error> { e })?;

                if attachments_resolved.is_empty() {
                    // Caption-only Multipart (all blocks were Text/unknown).
                    if let Some(cap) = caption_opt {
                        self.api_send_message(channel_id, cap).await?;
                    } else {
                        warn!("Discord Multipart: all blocks empty or unknown, nothing to send");
                    }
                    return Ok(());
                }

                // Chunk by both count (≤ ATTACHMENTS_PER_CHUNK) and aggregate
                // bytes (≤ CHUNK_TOTAL_CAP_BYTES) so a 10×3 MiB Multipart
                // doesn't 413 on Discord's per-request size limit. Order is
                // preserved; oversized single attachments get their own
                // chunk (they'll still be rejected by Discord, but with the
                // same error path as before this cap existed).
                let chunks: Vec<Vec<(bytes::Bytes, String, String)>> =
                    chunk_attachments(attachments_resolved);
                let total_chunks = chunks.len();
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let chunk_caption = if i == 0 { caption_opt } else { None };
                    if let Err(e) = self
                        .api_send_attachments(channel_id, chunk, chunk_caption)
                        .await
                    {
                        if i > 0 {
                            // Standalone WARN with structured fields so an
                            // operator grepping for "why are some files
                            // showing and some not?" can find this in one
                            // search instead of parsing prose. The failed
                            // chunk index is recoverable as `chunks_sent`
                            // (the count of chunks that succeeded before
                            // this one).
                            warn!(
                                event = "discord_multipart_partial_send",
                                chunks_sent = i,
                                chunks_total = total_chunks,
                                "discord multipart partial send: chunk {}/{} failed after {} chunk(s) already on the wire",
                                i + 1,
                                total_chunks,
                                i
                            );
                        }
                        return Err(e);
                    }
                }
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_typing(&user.platform_id).await
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

/// Maximum byte size for an attachment to be classified as a vision-eligible
/// image. Anthropic's image content blocks are capped at 5 MB; oversize images
/// fall through to `File` so the bridge passes the URL as text instead of
/// attempting an inline image block.
const VISION_IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Build the `payload_json` body for an outbound attachment request.
///
/// Discord's `POST /channels/{id}/messages` multipart endpoint expects a
/// `payload_json` part containing the same JSON the JSON-only variant would
/// take. Captions longer than `DISCORD_MSG_LIMIT` chars must be truncated
/// explicitly; otherwise the API responds 400 and silently drops the upload.
/// Greedy-pack attachments into chunks subject to two caps:
///
///   1. At most [`ATTACHMENTS_PER_CHUNK`] entries per chunk (Discord's
///      `files[N]` limit).
///   2. At most [`CHUNK_TOTAL_CAP_BYTES`] aggregate bytes per chunk (Discord's
///      ~25 MiB request size limit, with headroom for multipart overhead).
///
/// Order of inputs is preserved across the output. If a single attachment
/// alone exceeds the byte cap, it lands in its own chunk and is forwarded
/// untouched — Discord will reject the request, but that mirrors the
/// pre-existing behavior where the per-file cap was the only gate.
fn chunk_attachments(
    attachments: Vec<(bytes::Bytes, String, String)>,
) -> Vec<Vec<(bytes::Bytes, String, String)>> {
    let mut chunks: Vec<Vec<(bytes::Bytes, String, String)>> = Vec::new();
    let mut current: Vec<(bytes::Bytes, String, String)> = Vec::new();
    let mut current_bytes: usize = 0;

    for item in attachments {
        let item_len = item.0.len();
        // Start a new chunk when adding this item would push us over the
        // count or byte cap — but only if the current chunk isn't empty.
        // (Empty + oversized item: keep going so we always make progress.)
        let would_exceed_count = current.len() >= ATTACHMENTS_PER_CHUNK;
        let would_exceed_bytes =
            current_bytes.saturating_add(item_len) > CHUNK_TOTAL_CAP_BYTES;
        if !current.is_empty() && (would_exceed_count || would_exceed_bytes) {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes = current_bytes.saturating_add(item_len);
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn build_attachment_payload_json(caption: Option<&str>) -> String {
    match caption {
        Some(c) if !c.is_empty() => {
            let truncated: String = c.chars().take(DISCORD_MSG_LIMIT).collect();
            serde_json::json!({ "content": truncated }).to_string()
        }
        _ => serde_json::json!({}).to_string(),
    }
}

/// Strip MIME parameters (e.g. `; charset=utf-8`) so downstream comparisons
/// against canonical types like `image/png` work. Lower-cases and trims so
/// `IMAGE/PNG ; charset=utf-8` and `image/png` both normalize to `image/png`.
fn strip_mime_params(raw: &str) -> String {
    raw.split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Derive a filename from a URL path: take the segment after the last `/`,
/// drop any query/fragment, percent-decode best-effort. Returns None if the
/// URL has no useful path segment (e.g. `https://host/`).
fn derive_filename_from_url(url: &str) -> Option<String> {
    // Strip scheme://host. We only care about the path-ish suffix; doing
    // this without a real URL parser keeps the helper dep-free and total
    // (a malformed URL still gets a best-effort answer).
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = after_scheme.split_once('/').map(|(_, r)| r).unwrap_or("");
    // Drop query and fragment.
    let path = path.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').next().unwrap_or("");
    if last.is_empty() {
        return None;
    }
    // Percent-decode best-effort; fall back to the raw segment on failure.
    let decoded = percent_decode_lossy(last);
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

/// Tiny percent-decoder. We don't pull in `percent-encoding` for this — the
/// adapter already avoids new deps and we only need it to prettify Discord
/// CDN paths like `photo%20final.png` → `photo final.png`.
fn percent_decode_lossy(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Pick a filename for an outbound `File` arm. Preference order: explicit
/// `filename` field → URL path tail → `"file"`.
fn resolve_file_filename(field: Option<&str>, url: &str) -> String {
    field
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| derive_filename_from_url(url))
        .unwrap_or_else(|| "file".to_string())
}

/// Pick a MIME for an outbound `File` arm. Preference order: explicit `mime`
/// field → response Content-Type → extension lookup from filename →
/// `application/octet-stream`.
fn resolve_file_mime(field: Option<&str>, response_ct: Option<&str>, filename: &str) -> String {
    field
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| response_ct.map(str::to_string))
        .or_else(|| mime_from_extension(filename).map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Pick a filename for an outbound `Image` arm. Preference order: URL path
/// tail → `"image" + extension` inferred from the resolved MIME (default
/// `.png`).
fn resolve_image_filename(url: &str, resolved_mime: &str) -> String {
    if let Some(name) = derive_filename_from_url(url) {
        return name;
    }
    let ext = match resolved_mime {
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/heic" => ".heic",
        "image/heif" => ".heif",
        _ => ".png",
    };
    format!("image{ext}")
}

/// Pick a MIME for an outbound `Image` arm. Preference order: response
/// Content-Type → extension lookup from URL tail → `image/png`.
fn resolve_image_mime(response_ct: Option<&str>, url: &str) -> String {
    if let Some(ct) = response_ct.filter(|s| !s.is_empty()) {
        return ct.to_string();
    }
    if let Some(tail) = derive_filename_from_url(url) {
        if let Some(ext) = mime_from_extension(&tail) {
            return ext.to_string();
        }
    }
    "image/png".to_string()
}

/// Best-effort MIME inference from a filename extension. Used as a fallback
/// when Discord's `content_type` field is missing or empty (we've observed
/// this on some bot-relayed attachments).
fn mime_from_extension(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "heic" => Some("image/heic"),
        "heif" => Some("image/heif"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "md" => Some("text/markdown"),
        "json" => Some("application/json"),
        "mp4" => Some("video/mp4"),
        "mov" => Some("video/quicktime"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        _ => None,
    }
}

/// Classify a single Discord attachment JSON object into a `ChannelContent`
/// block. Vision-eligible image MIME types (jpeg/png/gif/webp) under
/// `VISION_IMAGE_MAX_BYTES` become `Image`; everything else becomes `File`
/// (URL-pass-through; the bridge will surface it as a text descriptor in v1).
///
/// MIME resolution chain: `attachments[].content_type` (if non-empty) →
/// extension lookup → `application/octet-stream`.
fn classify_discord_attachment(att: &serde_json::Value) -> ChannelContent {
    let url = att["url"].as_str().unwrap_or("").to_string();
    let filename = att["filename"].as_str().unwrap_or("file").to_string();
    let size = att["size"].as_u64();

    let resolved_mime: String = att["content_type"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| mime_from_extension(&filename).map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let is_vision_mime = matches!(
        resolved_mime.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    );
    // If size is unknown, optimistically allow the image — the bridge will
    // surface a 4xx if Anthropic rejects it, which is better than silently
    // demoting to a text URL.
    let within_vision_limit = size.map(|s| s <= VISION_IMAGE_MAX_BYTES).unwrap_or(true);

    if is_vision_mime && within_vision_limit {
        ChannelContent::Image { url, caption: None }
    } else {
        ChannelContent::File {
            url,
            filename,
            mime: Some(resolved_mime),
            size,
        }
    }
}

/// Parse a Discord MESSAGE_CREATE or MESSAGE_UPDATE payload into a `ChannelMessage`.
async fn parse_discord_message(
    d: &serde_json::Value,
    bot_user_id: &Arc<RwLock<Option<String>>>,
    allowed_guilds: &[String],
    allowed_users: &[String],
    ignore_bots: bool,
) -> Option<ChannelMessage> {
    // Diagnostic: dump the raw Discord payload so we can ground attachment
    // parsing in real JSON. Gated by RUST_LOG; silent at default `info` level.
    // Enable with: RUST_LOG=openfang_channels::discord=debug
    debug!(target: "openfang_channels::discord", payload = %d, "discord raw message payload");

    let author = d.get("author")?;
    let author_id = author["id"].as_str()?;

    // Filter out bot's own messages
    if let Some(ref bid) = *bot_user_id.read().await {
        if author_id == bid {
            return None;
        }
    }

    // Filter out other bots (configurable via ignore_bots)
    if ignore_bots && author["bot"].as_bool() == Some(true) {
        return None;
    }

    // Filter by allowed users
    if !allowed_users.is_empty() && !allowed_users.iter().any(|u| u == author_id) {
        debug!("Discord: ignoring message from unlisted user {author_id}");
        return None;
    }

    // Filter by allowed guilds
    if !allowed_guilds.is_empty() {
        if let Some(guild_id) = d["guild_id"].as_str() {
            if !allowed_guilds.iter().any(|g| g == guild_id) {
                return None;
            }
        }
    }

    let content_text = d["content"].as_str().unwrap_or("");
    let channel_id = d["channel_id"].as_str()?;
    let message_id = d["id"].as_str().unwrap_or("0");
    let username = author["username"].as_str().unwrap_or("Unknown");
    let discriminator = author["discriminator"].as_str().unwrap_or("0000");
    let display_name = if discriminator == "0" {
        username.to_string()
    } else {
        format!("{username}#{discriminator}")
    };

    let timestamp = d["timestamp"]
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    // Parse commands (messages starting with /). Commands do not carry
    // attachments in v1; attachment processing only runs in the non-command path.
    let content = if content_text.starts_with('/') {
        let parts: Vec<&str> = content_text.splitn(2, ' ').collect();
        let cmd_name = &parts[0][1..];
        let args = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };
        ChannelContent::Command {
            name: cmd_name.to_string(),
            args,
        }
    } else {
        let attachment_blocks: Vec<ChannelContent> = d["attachments"]
            .as_array()
            .map(|arr| arr.iter().map(classify_discord_attachment).collect())
            .unwrap_or_default();

        match (content_text.is_empty(), attachment_blocks.len()) {
            // No text, no attachments → nothing to ingest.
            (true, 0) => return None,
            // Text only.
            (false, 0) => ChannelContent::Text(content_text.to_string()),
            // Single attachment, no caption.
            (true, 1) => attachment_blocks.into_iter().next().unwrap(),
            // Single attachment + caption: emit Multipart with the caption as
            // a sibling Text block. This keeps the caption visible to providers
            // that flatten content to text only (e.g. claude-code/*, which
            // currently drops Image blocks) — the user gets a coherent
            // text-only response instead of a hallucination. Vision-capable
            // providers see the same blocks and dispatch multimodally.
            (false, 1) => {
                let block = attachment_blocks.into_iter().next().unwrap();
                let normalized = match block {
                    // Drop any caption that classify_discord_attachment may have
                    // attached; the sibling Text block is now the caption.
                    ChannelContent::Image { url, caption: _ } => {
                        ChannelContent::Image { url, caption: None }
                    }
                    other => other,
                };
                ChannelContent::Multipart(vec![
                    ChannelContent::Text(content_text.to_string()),
                    normalized,
                ])
            }
            // Multiple attachments, no caption.
            (true, _) => ChannelContent::Multipart(attachment_blocks),
            // Multiple attachments + caption: text first, then attachments
            // (matches Discord's visual ordering: text above attachments).
            (false, _) => {
                let mut blocks = Vec::with_capacity(attachment_blocks.len() + 1);
                blocks.push(ChannelContent::Text(content_text.to_string()));
                blocks.extend(attachment_blocks);
                ChannelContent::Multipart(blocks)
            }
        }
    };

    // Determine if this is a group message (guild_id present = server channel)
    let is_group = d["guild_id"].as_str().is_some();

    // Check if bot was @mentioned (for MentionOnly policy enforcement)
    let was_mentioned = if let Some(ref bid) = *bot_user_id.read().await {
        // Check Discord mentions array
        let mentioned_in_array = d["mentions"]
            .as_array()
            .map(|arr| arr.iter().any(|m| m["id"].as_str() == Some(bid.as_str())))
            .unwrap_or(false);
        // Also check content for <@bot_id> or <@!bot_id> patterns
        let mentioned_in_content = content_text.contains(&format!("<@{bid}>"))
            || content_text.contains(&format!("<@!{bid}>"));
        mentioned_in_array || mentioned_in_content
    } else {
        false
    };

    let mut metadata = HashMap::new();
    if was_mentioned {
        metadata.insert("was_mentioned".to_string(), serde_json::json!(true));
    }
    // Stash the Discord author ID so the router can key bindings on user, not channel.
    // (`sender.platform_id` below is the channel ID, used for the send path.)
    metadata.insert("sender_user_id".to_string(), serde_json::json!(author_id));

    Some(ChannelMessage {
        channel: ChannelType::Discord,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: channel_id.to_string(),
            display_name,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_payload_no_caption() {
        // No caption → empty JSON object so Discord doesn't reject it.
        assert_eq!(build_attachment_payload_json(None), "{}");
        assert_eq!(build_attachment_payload_json(Some("")), "{}");
    }

    #[test]
    fn test_attachment_payload_short_caption() {
        let json = build_attachment_payload_json(Some("hello"));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["content"], "hello");
    }

    #[test]
    fn test_attachment_payload_truncates_long_caption() {
        // 3000 chars → must truncate to DISCORD_MSG_LIMIT (2000) so Discord
        // accepts the request instead of 400-ing on a too-long content field.
        let long = "a".repeat(3000);
        let json = build_attachment_payload_json(Some(&long));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["content"].as_str().unwrap().chars().count(),
            DISCORD_MSG_LIMIT
        );
    }

    #[test]
    fn test_attachment_payload_truncation_is_char_safe() {
        // Multibyte chars must not be split mid-codepoint.
        let s: String = "héllo ".repeat(500); // 6 chars per chunk → 3000 chars total
        let json = build_attachment_payload_json(Some(&s));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Round-trip through serde guarantees we didn't produce invalid UTF-8.
        assert_eq!(
            v["content"].as_str().unwrap().chars().count(),
            DISCORD_MSG_LIMIT
        );
    }

    #[test]
    fn test_attachment_field_name_pinned() {
        // Discord rejects the upload silently if the multipart field isn't
        // exactly `files[0]` (a `file[0]` typo would fail at runtime, per
        // attachment, with no useful error). Pin the wire format here so a
        // typo at the call site is impossible without also changing this test.
        // Both invariants matter: the constant's literal value AND the
        // `format!("files[{i}]", i=0)` we now use at the call site must agree.
        assert_eq!(ATTACHMENT_FIELD_NAME, "files[0]");
        assert_eq!(format!("files[{}]", 0), ATTACHMENT_FIELD_NAME);
    }

    #[test]
    fn test_multipart_part_accepts_common_mimes() {
        // Validate that mime_str() doesn't reject the MIME types we map from
        // tool_runner.rs::channel_send. If any of these started failing we'd
        // surface as a runtime upload error per file.
        for mime in [
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "application/pdf",
            "text/plain",
            "application/json",
            "application/octet-stream",
            "video/mp4",
        ] {
            let part = reqwest::multipart::Part::bytes(b"x".to_vec())
                .file_name("f.bin")
                .mime_str(mime);
            assert!(part.is_ok(), "mime_str rejected {mime}");
        }
    }

    #[tokio::test]
    async fn test_parse_discord_message_basic() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hello agent!",
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Discord);
        assert_eq!(msg.sender.display_name, "alice");
        assert_eq!(msg.sender.platform_id, "ch1");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello agent!"));
    }

    #[tokio::test]
    async fn test_parse_discord_message_filters_bot() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "My own message",
            "author": {
                "id": "bot123",
                "username": "openfang",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_message_filters_other_bots() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Bot message",
            "author": {
                "id": "other_bot",
                "username": "somebot",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_ignore_bots_false_allows_other_bots() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Bot message",
            "author": {
                "id": "other_bot",
                "username": "somebot",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // With ignore_bots=false, other bots' messages should be allowed
        let msg = parse_discord_message(&d, &bot_id, &[], &[], false).await;
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert_eq!(msg.sender.display_name, "somebot");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Bot message"));
    }

    #[tokio::test]
    async fn test_parse_discord_ignore_bots_false_still_filters_self() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "My own message",
            "author": {
                "id": "bot123",
                "username": "openfang",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Even with ignore_bots=false, the bot's own messages must still be filtered
        let msg = parse_discord_message(&d, &bot_id, &[], &[], false).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_message_guild_filter() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "guild_id": "999",
            "content": "Hello",
            "author": {
                "id": "user1",
                "username": "bob",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Not in allowed guilds
        let msg =
            parse_discord_message(&d, &bot_id, &["111".into(), "222".into()], &[], true).await;
        assert!(msg.is_none());

        // In allowed guilds
        let msg = parse_discord_message(&d, &bot_id, &["999".into()], &[], true).await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_discord_command() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "/agent hello-world",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_discord_empty_content() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_discriminator() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hi",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "1234"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.sender.display_name, "alice#1234");
    }

    #[tokio::test]
    async fn test_parse_discord_message_update() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Edited message content",
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00",
            "edited_timestamp": "2024-01-01T00:01:00+00:00"
        });

        // MESSAGE_UPDATE uses the same parse function as MESSAGE_CREATE
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Discord);
        assert!(
            matches!(msg.content, ChannelContent::Text(ref t) if t == "Edited message content")
        );
    }

    #[tokio::test]
    async fn test_parse_discord_allowed_users_filter() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hello",
            "author": {
                "id": "user999",
                "username": "bob",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Not in allowed users
        let msg = parse_discord_message(
            &d,
            &bot_id,
            &[],
            &["user111".into(), "user222".into()],
            true,
        )
        .await;
        assert!(msg.is_none());

        // In allowed users
        let msg = parse_discord_message(&d, &bot_id, &[], &["user999".into()], true).await;
        assert!(msg.is_some());

        // Empty allowed_users = allow all
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_discord_mention_detection() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));

        // Message with bot mentioned in mentions array
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "guild_id": "guild1",
            "content": "Hey <@bot123> help me",
            "mentions": [{"id": "bot123", "username": "openfang"}],
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(msg.is_group);
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );

        // Message without mention in group
        let d2 = serde_json::json!({
            "id": "msg2",
            "channel_id": "ch1",
            "guild_id": "guild1",
            "content": "Just chatting",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg2 = parse_discord_message(&d2, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(msg2.is_group);
        assert!(!msg2.metadata.contains_key("was_mentioned"));
    }

    #[tokio::test]
    async fn test_parse_discord_dm_not_group() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "dm-ch1",
            "content": "Hello",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(!msg.is_group);
    }

    #[test]
    fn test_build_heartbeat_payload_with_sequence() {
        let payload = build_heartbeat_payload(Some(42));
        assert_eq!(payload["op"], 1);
        assert_eq!(payload["d"], 42);
        // Round-trip through serde_json::to_string and re-parse to assert
        // valid JSON matching {"op":1,"d":42} regardless of key ordering.
        let s = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, serde_json::json!({"op": 1, "d": 42}));
    }

    #[test]
    fn test_build_heartbeat_payload_without_sequence() {
        let payload = build_heartbeat_payload(None);
        assert_eq!(payload["op"], 1);
        assert!(payload["d"].is_null());
        let s = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({"op": 1, "d": serde_json::Value::Null})
        );
    }

    #[test]
    fn test_discord_adapter_creation() {
        let adapter = DiscordAdapter::new(
            "test-token".to_string(),
            vec!["123".to_string(), "456".to_string()],
            vec![],
            true,
            37376,
        );
        assert_eq!(adapter.name(), "discord");
        assert_eq!(adapter.channel_type(), ChannelType::Discord);
    }

    // -- Multipart / attachment parsing tests (commit 4) ----------------------

    fn att(filename: &str, content_type: Option<&str>, size: u64) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "url": format!("https://cdn.discordapp.com/attachments/1/2/{filename}"),
            "filename": filename,
            "size": size,
        });
        if let Some(ct) = content_type {
            obj["content_type"] = serde_json::Value::String(ct.to_string());
        }
        obj
    }

    fn payload_with(content: &str, attachments: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": content,
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00",
            "attachments": attachments,
        })
    }

    #[tokio::test]
    async fn test_parse_image_only_no_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("photo.png", Some("image/png"), 100_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Image { caption, url } => {
                assert!(caption.is_none());
                assert!(url.contains("photo.png"));
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_image_with_caption() {
        // Single image + caption is emitted as Multipart([Text, Image]) so the
        // caption survives providers that flatten content blocks to text only
        // (e.g. claude-code/*). The Image carries no caption of its own; the
        // sibling Text block IS the caption.
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "look at this",
            vec![att("photo.jpg", Some("image/jpeg"), 50_000)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "look at this"));
                match &parts[1] {
                    ChannelContent::Image { caption, url } => {
                        assert!(
                            caption.is_none(),
                            "image caption should be None; the sibling Text block is the caption"
                        );
                        assert!(url.contains("photo.jpg"));
                    }
                    other => panic!("expected Image as second part, got {other:?}"),
                }
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_multi_image_no_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "",
            vec![
                att("a.png", Some("image/png"), 10_000),
                att("b.png", Some("image/png"), 20_000),
            ],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(parts
                    .iter()
                    .all(|p| matches!(p, ChannelContent::Image { .. })));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_multi_image_with_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "two pics",
            vec![
                att("a.png", Some("image/png"), 10_000),
                att("b.png", Some("image/png"), 20_000),
            ],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 3);
                // Text first, then images.
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "two pics"));
                assert!(matches!(&parts[1], ChannelContent::Image { .. }));
                assert!(matches!(&parts[2], ChannelContent::Image { .. }));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_heic_falls_to_file() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("photo.heic", Some("image/heic"), 100_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::File { mime, filename, .. } => {
                assert_eq!(filename, "photo.heic");
                assert_eq!(mime.as_deref(), Some("image/heic"));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_oversize_image_falls_to_file() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        // 6 MB exceeds VISION_IMAGE_MAX_BYTES (5 MB).
        let d = payload_with(
            "",
            vec![att("huge.png", Some("image/png"), 6 * 1024 * 1024)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::File {
                filename,
                mime,
                size,
                ..
            } => {
                assert_eq!(filename, "huge.png");
                assert_eq!(mime.as_deref(), Some("image/png"));
                assert_eq!(size, Some(6 * 1024 * 1024));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_file_with_caption_yields_multipart() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "see attached",
            vec![att("doc.pdf", Some("application/pdf"), 200_000)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "see attached"));
                assert!(matches!(&parts[1], ChannelContent::File { .. }));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_extension_fallback_when_content_type_missing() {
        // Discord occasionally omits content_type on bot-relayed attachments;
        // we should fall back to the filename extension.
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("pic.png", None, 50_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(matches!(msg.content, ChannelContent::Image { .. }));
    }

    // -- Pure helper tests for File/Image arm fallback chains --------------

    #[test]
    fn test_strip_mime_params_basic() {
        assert_eq!(strip_mime_params("image/png"), "image/png");
        assert_eq!(strip_mime_params("image/png; charset=utf-8"), "image/png");
        assert_eq!(
            strip_mime_params("  IMAGE/PNG ; charset=utf-8 "),
            "image/png"
        );
        assert_eq!(strip_mime_params(""), "");
    }

    #[test]
    fn test_derive_filename_from_url() {
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/b/photo.png"),
            Some("photo.png".to_string())
        );
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/b/photo.png?ex=1&hm=2"),
            Some("photo.png".to_string())
        );
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/photo%20final.png"),
            Some("photo final.png".to_string())
        );
        // Trailing slash → no filename derivable.
        assert_eq!(derive_filename_from_url("https://cdn.example.com/"), None);
        assert_eq!(derive_filename_from_url("https://cdn.example.com"), None);
    }

    #[test]
    fn test_resolve_file_filename_chain() {
        // Field wins over URL.
        assert_eq!(
            resolve_file_filename(Some("explicit.bin"), "https://x/y/url.dat"),
            "explicit.bin"
        );
        // Empty field → URL fallback.
        assert_eq!(
            resolve_file_filename(Some(""), "https://x/y/url.dat"),
            "url.dat"
        );
        // None → URL fallback.
        assert_eq!(
            resolve_file_filename(None, "https://x/y/url.dat"),
            "url.dat"
        );
        // No URL tail → "file".
        assert_eq!(resolve_file_filename(None, "https://x/"), "file");
    }

    #[test]
    fn test_resolve_file_mime_chain() {
        // Field wins.
        assert_eq!(
            resolve_file_mime(Some("application/pdf"), Some("text/plain"), "f.txt"),
            "application/pdf"
        );
        // No field → response Content-Type.
        assert_eq!(
            resolve_file_mime(None, Some("text/plain"), "f.txt"),
            "text/plain"
        );
        // No field, no CT → extension lookup.
        assert_eq!(resolve_file_mime(None, None, "f.pdf"), "application/pdf");
        // Nothing → default.
        assert_eq!(
            resolve_file_mime(None, None, "no-ext"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_resolve_image_filename_chain() {
        // URL tail wins.
        assert_eq!(
            resolve_image_filename("https://x/y/picture.jpg", "image/jpeg"),
            "picture.jpg"
        );
        // No URL tail → image + ext from MIME.
        assert_eq!(
            resolve_image_filename("https://x/", "image/jpeg"),
            "image.jpg"
        );
        assert_eq!(
            resolve_image_filename("https://x/", "image/png"),
            "image.png"
        );
        assert_eq!(
            resolve_image_filename("https://x/", "image/webp"),
            "image.webp"
        );
        // Unknown MIME → .png default.
        assert_eq!(
            resolve_image_filename("https://x/", "application/octet-stream"),
            "image.png"
        );
    }

    #[test]
    fn test_resolve_image_mime_chain() {
        // Response CT wins.
        assert_eq!(
            resolve_image_mime(Some("image/jpeg"), "https://x/y/foo.png"),
            "image/jpeg"
        );
        // No CT → URL extension.
        assert_eq!(resolve_image_mime(None, "https://x/y/foo.png"), "image/png");
        // No CT, no extension → default.
        assert_eq!(resolve_image_mime(None, "https://x/y/blob"), "image/png");
    }

    // -- Fetcher::fetch / do_http_fetch size-cap tests ----------------------

    /// Spawn a hand-rolled HTTP server that replies with a fixed status,
    /// optional Content-Length header (lying or omitted), and a body produced
    /// by the supplied closure. This intentionally does NOT use axum's
    /// `Body::from(Vec<u8>)` (which always sends the real Content-Length); we
    /// need control over the header to exercise both pre-flight and streaming
    /// rejection paths.
    async fn spawn_raw_http_server(
        status_line: &'static str,
        content_length: Option<&'static str>,
        body: bytes::Bytes,
    ) -> String {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // Accept exactly one connection — sufficient for a single
            // do_http_fetch test invocation.
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            // Drain the request line + headers (best-effort; we don't parse).
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;
            let mut header = format!("HTTP/1.1 {status_line}\r\n");
            header.push_str("Content-Type: application/octet-stream\r\n");
            if let Some(cl) = content_length {
                header.push_str(&format!("Content-Length: {cl}\r\n"));
            }
            header.push_str("Connection: close\r\n\r\n");
            let _ = sock.write_all(header.as_bytes()).await;
            let _ = sock.write_all(&body).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/file")
    }

    /// Like `spawn_raw_http_server` but with a configurable Content-Type
    /// header and a configurable path suffix in the returned URL. Used by
    /// mixed-type Multipart tests where different URLs must carry different
    /// content-types (e.g. `image/png` for the Image block, `application/pdf`
    /// for the File block).
    async fn spawn_fixture_server(
        content_type: &'static str,
        path: &'static str,
        body: bytes::Bytes,
    ) -> String {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;
            let body_len = body.len();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = sock.write_all(header.as_bytes()).await;
            let _ = sock.write_all(&body).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/{path}")
    }

    /// Spawn a server that omits Content-Length (so the pre-flight CL check
    /// can't short-circuit) and streams `actual_len` bytes in 1 MiB chunks
    /// using HTTP/1.1 Connection: close framing. After each successful chunk
    /// write, increments `bytes_sent`. The test asserts on the counter to
    /// prove the client aborted *mid-stream* rather than buffering the
    /// entire body and complaining at the end.
    async fn spawn_chunked_streaming_server(
        actual_len: usize,
        bytes_sent: Arc<std::sync::atomic::AtomicUsize>,
    ) -> String {
        use std::sync::atomic::Ordering;
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            let mut hbuf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut hbuf).await;
            let header = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n";
            if sock.write_all(header.as_bytes()).await.is_err() {
                return;
            }
            let chunk = vec![0u8; 1024 * 1024];
            let mut written = 0usize;
            while written < actual_len {
                let n = std::cmp::min(chunk.len(), actual_len - written);
                if sock.write_all(&chunk[..n]).await.is_err() {
                    // Client aborted (which is exactly what we expect once it
                    // hits the cap). Stop writing further chunks.
                    break;
                }
                // Flush so the kernel doesn't coalesce chunks beyond what the
                // client has actually pulled — that would let the server
                // "appear" to write 100 MiB instantly while the client has
                // only consumed 25 MiB. With small SO_SNDBUF + flush the
                // counter approximates client-consumed bytes.
                let _ = sock.flush().await;
                written += n;
                bytes_sent.fetch_add(n, Ordering::SeqCst);
            }
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/file")
    }

    fn test_adapter() -> DiscordAdapter {
        DiscordAdapter::new("test-token".into(), vec![], vec![], true, 0)
    }

    /// Test-only helper: drive the wire-level fetch directly so tests against
    /// 127.0.0.1 fixture servers don't trip the SSRF preflight. Production
    /// callers always go through `Fetcher::fetch` and inherit the guard.
    async fn test_fetch(
        _adapter: &DiscordAdapter,
        url: &str,
    ) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>> {
        let parsed = Url::parse(url).unwrap();
        do_http_fetch(&parsed).await
    }

    #[tokio::test]
    async fn test_download_size_cap_via_content_length() {
        // Server advertises an oversized Content-Length and sends a tiny body.
        // The adapter must reject before reading anything significant.
        let oversized = (URL_FETCH_MAX_BYTES + 1).to_string();
        // Leak the string so we can hand &'static str to the spawn helper.
        let cl: &'static str = Box::leak(oversized.into_boxed_str());
        let url = spawn_raw_http_server("200 OK", Some(cl), bytes::Bytes::from_static(b"x")).await;

        let adapter = test_adapter();
        let res = test_fetch(&adapter, &url).await;
        assert!(res.is_err(), "expected Err on oversized Content-Length");
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("Content-Length"),
            "err should mention CL: {err}"
        );
    }

    #[tokio::test]
    async fn test_download_size_cap_via_streaming_aborts_midstream() {
        // The strengthened version: server claims a believable Content-Length
        // (just under the cap) so the pre-flight check passes, then streams
        // chunks past the cap. We assert via a side-channel counter that the
        // server stopped writing well before the full body went out — proving
        // the client aborted mid-stream rather than buffering everything and
        // erroring at the end.
        use std::sync::atomic::Ordering;
        let actual = URL_FETCH_MAX_BYTES * 4; // ~100 MiB worth of chunks queued
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let url = spawn_chunked_streaming_server(actual, counter.clone()).await;

        let adapter = test_adapter();
        let res = test_fetch(&adapter, &url).await;
        assert!(res.is_err(), "expected Err on oversized streamed body");
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("streamed body exceeds cap"),
            "err should mention streaming cap: {err}"
        );
        // Give the server task a beat to observe the closed socket.
        tokio::time::sleep(Duration::from_millis(250)).await;
        let sent = counter.load(Ordering::SeqCst);
        // Allow generous slack for kernel/userland buffering on top of the
        // 25 MiB cap. The regression we're guarding against is "client buffers
        // the entire 100 MiB then errors" — that would show ~100 MiB sent.
        // Allowing up to 2*cap covers reasonable in-flight buffering without
        // letting the regression slip through.
        let allowed = URL_FETCH_MAX_BYTES * 2;
        assert!(
            sent <= allowed,
            "server pushed {sent} bytes (allowed {allowed}); client did not abort mid-stream"
        );
        assert!(
            sent < actual,
            "server pushed full payload ({sent} of {actual}); client did not abort mid-stream"
        );
    }

    #[tokio::test]
    async fn test_download_under_cap_succeeds_and_returns_bytes_and_ct() {
        // Sanity check: a normal small payload returns the bytes and the
        // stripped Content-Type. Uses axum so Content-Length is set correctly.
        use axum::{routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/f",
            get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "image/png; charset=utf-8")],
                    bytes::Bytes::from_static(b"PNGDATA"),
                )
            }),
        );
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let url = format!("http://{addr}/f");

        let adapter = test_adapter();
        let (bytes, ct) = test_fetch(&adapter, &url).await.unwrap();
        assert_eq!(&bytes[..], b"PNGDATA");
        // Content-Type parameters must be stripped.
        assert_eq!(ct.as_deref(), Some("image/png"));
    }

    // -- SSRF guard tests ---------------------------------------------------

    async fn assert_ssrf_blocked(url: &str) {
        let adapter = test_adapter();
        let res = adapter.fetcher.fetch(url).await;
        let err = res
            .err()
            .unwrap_or_else(|| panic!("expected SSRF block for {url}"))
            .to_string();
        assert!(
            err.contains("refused") || err.contains("not allowed") || err.contains("blocked"),
            "expected SSRF refusal for {url}, got: {err}"
        );
        // The query string must not appear in the error (log-scrubbing).
        assert!(
            !err.contains("?"),
            "SSRF error must not leak query string for {url}: {err}"
        );
    }

    #[tokio::test]
    async fn test_ssrf_blocks_loopback() {
        assert_ssrf_blocked("http://127.0.0.1/secret?token=abc").await;
    }

    #[tokio::test]
    async fn test_ssrf_blocks_private_10() {
        assert_ssrf_blocked("http://10.0.0.1/admin?key=v").await;
    }

    #[tokio::test]
    async fn test_ssrf_blocks_private_192() {
        assert_ssrf_blocked("http://192.168.1.1/router?op=reboot").await;
    }

    #[tokio::test]
    async fn test_ssrf_blocks_link_local() {
        // Cloud metadata: explicit canary URL from the spec.
        assert_ssrf_blocked(
            "http://169.254.169.254/latest/meta-data/iam/security-credentials/role",
        )
        .await;
    }

    #[tokio::test]
    async fn test_ssrf_blocks_non_http_scheme() {
        for u in [
            "file:///etc/passwd",
            "gopher://127.0.0.1:25/_HELO",
            "ftp://example.com/x",
            "data:text/plain,hello",
        ] {
            let adapter = test_adapter();
            let res = adapter.fetcher.fetch(u).await;
            let err = res
                .err()
                .unwrap_or_else(|| panic!("expected scheme refusal for {u}"))
                .to_string();
            assert!(
                err.contains("scheme") || err.contains("refused"),
                "expected scheme refusal for {u}: {err}"
            );
        }
    }

    #[tokio::test]
    async fn test_ssrf_allows_public_ip_literal_check() {
        // Validate the *check*, not the network round-trip: a public IP literal
        // must pass `resolve_and_check_host` so we know the guard isn't
        // accidentally over-blocking.
        let u = Url::parse("http://1.1.1.1/").unwrap();
        resolve_and_check_host(&u)
            .await
            .expect("public IP must pass SSRF check");
    }

    #[tokio::test]
    async fn test_ssrf_blocks_ipv6_loopback_and_metadata_mapped() {
        // Bracketed IPv6 loopback.
        assert_ssrf_blocked("http://[::1]/secret?x=1").await;
        // IPv4-mapped IPv6 of the cloud metadata IP must also be blocked.
        assert_ssrf_blocked("http://[::ffff:169.254.169.254]/latest?creds=1").await;
    }

    #[test]
    fn test_redact_url_strips_query() {
        let u = Url::parse(
            "https://cdn.discordapp.com/attachments/1/2/file.png?ex=abc&is=def&hm=secret#frag",
        )
        .unwrap();
        let r = redact_url(&u);
        assert_eq!(r, "https://cdn.discordapp.com/attachments/1/2/file.png");
        assert!(!r.contains("ex="));
        assert!(!r.contains("hm="));
        assert!(!r.contains("frag"));
    }

    #[test]
    fn test_is_blocked_v4_canary() {
        // Explicit assertion that the cloud-metadata IP is rejected even if
        // some future stdlib change widens or narrows `is_link_local`.
        assert!(is_blocked_v4(Ipv4Addr::new(169, 254, 169, 254)));
        assert!(is_blocked_v4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_blocked_v4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_blocked_v4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_blocked_v4(Ipv4Addr::new(172, 16, 0, 1)));
        assert!(is_blocked_v4(Ipv4Addr::new(0, 0, 0, 0)));
        assert!(is_blocked_v4(Ipv4Addr::new(100, 64, 0, 1))); // CGNAT
                                                              // Public addresses must pass.
        assert!(!is_blocked_v4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(!is_blocked_v4(Ipv4Addr::new(8, 8, 8, 8)));
    }

    #[tokio::test]
    async fn test_parse_empty_message_with_no_attachments_returns_none() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    // ==========================================================================
    // Outbound Multipart send() tests
    // ==========================================================================
    //
    // Test helpers: spin up an axum stub that accepts multipart POSTs to
    // `/channels/:id/messages`, captures the `payload_json` field text and
    // the number of file parts, and stores them in a shared Arc<Mutex<_>>.
    // We point the adapter at the stub via `api_base_override`.

    use tokio::sync::Mutex as TokioMutex;

    #[derive(Debug, Default, Clone)]
    struct CapturedFile {
        field_name: String,
        filename: Option<String>,
        content_type: Option<String>,
    }

    #[derive(Debug, Default, Clone)]
    struct CapturedPost {
        payload_json: String,
        /// Bare field names (legacy, kept so existing assertions continue to compile).
        file_field_names: Vec<String>,
        /// Richer per-file metadata captured from each `files[*]` part.
        files: Vec<CapturedFile>,
    }

    /// Build an axum stub that captures one or more multipart POSTs to
    /// `/channels/test/messages` and records them into `captured`.
    async fn spawn_discord_stub(captured: Arc<TokioMutex<Vec<CapturedPost>>>) -> String {
        use axum::{
            extract::{DefaultBodyLimit, Multipart},
            http::StatusCode,
            routing::post,
            Extension, Router,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let app = Router::new()
            .route(
                "/channels/test/messages",
                post(
                    |Extension(store): Extension<Arc<TokioMutex<Vec<CapturedPost>>>>,
                     mut multipart: Multipart| async move {
                        let mut post = CapturedPost::default();
                        while let Ok(Some(field)) = multipart.next_field().await {
                            let name = field.name().unwrap_or("").to_string();
                            if name == "payload_json" {
                                post.payload_json = field.text().await.unwrap_or_default();
                            } else {
                                let filename = field.file_name().map(str::to_string);
                                let content_type = field.content_type().map(str::to_string);
                                // Drain the file bytes so axum doesn't error.
                                let _ = field.bytes().await;
                                post.files.push(CapturedFile {
                                    field_name: name.clone(),
                                    filename,
                                    content_type,
                                });
                                post.file_field_names.push(name);
                            }
                        }
                        store.lock().await.push(post);
                        StatusCode::OK
                    },
                ),
            )
            // Default 2 MiB body limit would reject the byte-cap chunking
            // test's ~20 MiB chunks; disable it on the stub.
            .layer(DefaultBodyLimit::disable())
            .layer(Extension(captured));

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    fn make_channel_user(channel_id: &str) -> ChannelUser {
        ChannelUser {
            platform_id: channel_id.to_string(),
            display_name: "test-user".to_string(),
            openfang_user: None,
        }
    }

    fn test_adapter_with_base(base: String) -> DiscordAdapter {
        let mut a = test_adapter();
        a.api_base_override = Some(base);
        a
    }

    /// Like `test_adapter_with_base` but installs [`PermissiveFetcher`] so
    /// `Image{url}` / `File{url}` blocks pointing at localhost stub servers
    /// can flow through the normal `Fetcher::fetch` path without tripping
    /// the SSRF preflight.
    fn test_adapter_with_base_and_ssrf_bypass(base: String) -> DiscordAdapter {
        let mut a = test_adapter_with_base(base);
        a.fetcher = Arc::new(PermissiveFetcher);
        a
    }

    // ---- required test a: caption concatenation --------------------------------

    #[tokio::test]
    async fn test_multipart_outbound_caption_concatenation() {
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let content = ChannelContent::Multipart(vec![
            ChannelContent::Text("hello".to_string()),
            ChannelContent::Text("world".to_string()),
            ChannelContent::FileData {
                data: b"payload".to_vec(),
                filename: "file.txt".to_string(),
                mime_type: "text/plain".to_string(),
            },
        ]);

        adapter.send(&user, content).await.unwrap();

        let posts = captured.lock().await;
        assert_eq!(posts.len(), 1, "expected exactly one POST");
        let v: serde_json::Value = serde_json::from_str(&posts[0].payload_json).unwrap();
        assert_eq!(
            v["content"].as_str().unwrap_or(""),
            "hello\n\nworld",
            "caption should be the two Text blocks joined by \\n\\n"
        );
    }

    // ---- required test b: empty/whitespace caption suppressed ------------------
    //
    // Image URL fetches go through the SSRF guard which blocks 127.0.0.1, so
    // this test uses FileData to avoid the network requirement while still
    // exercising the caption-suppression logic path.

    #[tokio::test]
    async fn test_multipart_outbound_empty_caption_suppressed() {
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let content = ChannelContent::Multipart(vec![
            ChannelContent::Text("".to_string()),
            ChannelContent::Text("   ".to_string()),
            ChannelContent::FileData {
                data: b"bytes".to_vec(),
                filename: "f.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
            },
        ]);
        adapter.send(&user, content).await.unwrap();

        let posts = captured.lock().await;
        assert_eq!(posts.len(), 1, "expected one POST");
        let v: serde_json::Value = serde_json::from_str(&posts[0].payload_json).unwrap();
        assert!(
            v.get("content").is_none(),
            "empty/whitespace caption must produce payload_json without 'content' field; got: {}",
            posts[0].payload_json
        );
    }

    // ---- required test c: chunking >10 ----------------------------------------

    #[tokio::test]
    async fn test_multipart_outbound_chunking_gt10() {
        // 23 FileData blocks should produce ceil(23/10) = 3 POSTs.
        // First chunk: caption + files[0..10) (10 files)
        // Second chunk: no caption + files[0..10) (10 files)
        // Third chunk:  no caption + files[0..3)  (3 files)
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let mut parts = vec![ChannelContent::Text("cap".to_string())];
        for i in 0..23u32 {
            parts.push(ChannelContent::FileData {
                data: format!("data{i}").into_bytes(),
                filename: format!("f{i}.txt"),
                mime_type: "text/plain".to_string(),
            });
        }

        adapter
            .send(&user, ChannelContent::Multipart(parts))
            .await
            .unwrap();

        let posts = captured.lock().await;
        assert_eq!(
            posts.len(),
            3,
            "23 files should produce 3 POSTs (chunks of 10)"
        );

        // First chunk carries the caption.
        let v0: serde_json::Value = serde_json::from_str(&posts[0].payload_json).unwrap();
        assert_eq!(
            v0["content"].as_str().unwrap_or(""),
            "cap",
            "first chunk must carry the caption"
        );
        assert_eq!(
            posts[0].file_field_names.len(),
            10,
            "first chunk must have 10 files"
        );

        // Second chunk has no caption.
        let v1: serde_json::Value = serde_json::from_str(&posts[1].payload_json).unwrap();
        assert!(
            v1.get("content").is_none(),
            "second chunk must not carry the caption"
        );
        assert_eq!(
            posts[1].file_field_names.len(),
            10,
            "second chunk must have 10 files"
        );

        // Third chunk has no caption and only 3 files.
        let v2: serde_json::Value = serde_json::from_str(&posts[2].payload_json).unwrap();
        assert!(
            v2.get("content").is_none(),
            "third chunk must not carry the caption"
        );
        assert_eq!(
            posts[2].file_field_names.len(),
            3,
            "third chunk must have 3 files"
        );
    }

    // ---- required test d: caption-only fallback --------------------------------

    /// Checks that a Multipart with only Text blocks sends exactly one plain
    /// text message (no multipart POST) via the `api_send_message` path.
    #[tokio::test]
    async fn test_multipart_outbound_caption_only_fallback() {
        // The Discord stub only handles `/channels/test/messages` POSTs.
        // api_send_message sends JSON (not multipart), so we use a simple
        // axum stub that accepts any POST and records the Content-Type.
        use axum::{extract::Request, http::StatusCode, routing::post, Extension, Router};

        let calls: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(Vec::new()));
        let calls_clone = calls.clone();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app =
            Router::new()
                .route(
                    "/channels/test/messages",
                    post(
                        |Extension(store): Extension<Arc<TokioMutex<Vec<String>>>>,
                         req: Request| async move {
                            let ct = req
                                .headers()
                                .get("content-type")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .to_string();
                            store.lock().await.push(ct);
                            StatusCode::OK
                        },
                    ),
                )
                .layer(Extension(calls_clone));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let adapter = test_adapter_with_base(format!("http://{addr}"));
        let user = make_channel_user("test");

        let content =
            ChannelContent::Multipart(vec![ChannelContent::Text("only text".to_string())]);
        adapter.send(&user, content).await.unwrap();

        let cts = calls.lock().await;
        assert_eq!(cts.len(), 1, "expected exactly one POST for caption-only");
        // Plain message (JSON), not multipart.
        assert!(
            cts[0].contains("application/json"),
            "caption-only should send JSON, not multipart; content-type was: {}",
            cts[0]
        );
    }

    // ---- required test e: mixed Image{url}+File{url} resolver dispatch ----------

    /// Verifies that a `Multipart([Text, Image{url}, File{url}])` block routes
    /// each attachment through the correct resolver branch:
    ///
    /// - `Image{url}` → `resolve_image_mime` / `resolve_image_filename`:
    ///   the response Content-Type is used as-is and the filename is derived
    ///   from the URL path or inferred from the MIME (e.g. `image.png`).
    ///
    /// - `File{url, filename, mime}` → `resolve_file_mime` /
    ///   `resolve_file_filename`: the explicitly supplied filename and MIME
    ///   from the `File{}` block take precedence over the server's
    ///   Content-Type.
    ///
    /// The test spins up two local HTTP fixture servers (bypassing the SSRF
    /// guard via `ssrf_bypass`), one per URL, then asserts on the
    /// per-part filename and Content-Type captured by the Discord stub.
    #[tokio::test]
    async fn test_multipart_outbound_mixed_types_single_post() {
        // Spawn a fixture server for the Image block — serves image/png bytes.
        let image_url = spawn_fixture_server(
            "image/png",
            "photo.png",
            bytes::Bytes::from_static(b"\x89PNG\r\n\x1a\n"), // minimal PNG magic
        )
        .await;

        // Spawn a fixture server for the File block — serves application/pdf
        // bytes. The File block supplies an explicit filename and MIME so the
        // resolver must prefer those over the server's Content-Type.
        let file_url = spawn_fixture_server(
            "application/octet-stream", // server sends generic; resolver should prefer field mime
            "ignored-server-name.bin",
            bytes::Bytes::from_static(b"%PDF-1.4"),
        )
        .await;

        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base_and_ssrf_bypass(base);
        let user = make_channel_user("test");

        let content = ChannelContent::Multipart(vec![
            ChannelContent::Text("mixed".to_string()),
            ChannelContent::Image {
                url: image_url.clone(),
                caption: None,
            },
            ChannelContent::File {
                url: file_url.clone(),
                filename: "report.pdf".to_string(),
                mime: Some("application/pdf".to_string()),
                size: None,
            },
        ]);

        adapter.send(&user, content).await.unwrap();

        let posts = captured.lock().await;
        assert_eq!(
            posts.len(),
            1,
            "expected exactly one POST for mixed Multipart"
        );

        // Caption preserved.
        let v: serde_json::Value = serde_json::from_str(&posts[0].payload_json).unwrap();
        assert_eq!(v["content"].as_str().unwrap_or(""), "mixed");

        // Both files appeared in a single POST.
        assert_eq!(
            posts[0].files.len(),
            2,
            "expected two file parts in the POST"
        );

        // ---- Image block assertions ----
        // resolve_image_mime: server sent image/png → resolved mime = "image/png"
        // resolve_image_filename: URL path tail is "photo.png" → filename = "photo.png"
        let img_part = posts[0]
            .files
            .iter()
            .find(|f| f.field_name == "files[0]")
            .expect("files[0] must be present");
        assert_eq!(
            img_part.content_type.as_deref(),
            Some("image/png"),
            "Image block must use resolve_image_mime (server Content-Type preserved)"
        );
        assert_eq!(
            img_part.filename.as_deref(),
            Some("photo.png"),
            "Image block must use resolve_image_filename (URL path tail)"
        );

        // ---- File block assertions ----
        // resolve_file_filename: explicit filename "report.pdf" takes precedence over URL
        // resolve_file_mime: explicit mime "application/pdf" takes precedence over server CT
        let file_part = posts[0]
            .files
            .iter()
            .find(|f| f.field_name == "files[1]")
            .expect("files[1] must be present");
        assert_eq!(
            file_part.content_type.as_deref(),
            Some("application/pdf"),
            "File block must use resolve_file_mime (explicit mime from File{{}} block)"
        );
        assert_eq!(
            file_part.filename.as_deref(),
            Some("report.pdf"),
            "File block must use resolve_file_filename (explicit filename from File{{}} block)"
        );
    }

    // ---- should-have: mid-batch fetch failure ----------------------------------

    #[tokio::test]
    async fn test_multipart_outbound_fetch_failure_returns_err() {
        // A File block with an SSRF-blocked URL should cause send() to return Err.
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let content = ChannelContent::Multipart(vec![
            ChannelContent::Text("cap".to_string()),
            ChannelContent::File {
                url: "http://127.0.0.1/secret".to_string(),
                filename: "s.txt".to_string(),
                mime: None,
                size: None,
            },
        ]);

        let result = adapter.send(&user, content).await;
        assert!(result.is_err(), "expected Err on fetch failure");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Multipart fetch failed") || err.contains("refused"),
            "error should mention failing fetch; got: {err}"
        );
        // No POST should have been made (fetch failed before send).
        let posts = captured.lock().await;
        assert!(posts.is_empty(), "no POST should occur if fetch fails");
    }

    // ---- should-have: empty Multipart ------------------------------------------

    #[tokio::test]
    async fn test_multipart_outbound_empty_is_ok_no_posts() {
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let result = adapter.send(&user, ChannelContent::Multipart(vec![])).await;
        assert!(result.is_ok(), "empty Multipart should return Ok");
        let posts = captured.lock().await;
        assert!(
            posts.is_empty(),
            "empty Multipart must not produce any POSTs"
        );
    }

    // ---- should-have: unknown nested variant is logged, not fatal --------------

    #[tokio::test]
    async fn test_multipart_outbound_unknown_nested_variant_skipped() {
        // A Multipart containing a nested Multipart (and a FileData) should
        // warn but still send the FileData.
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let content = ChannelContent::Multipart(vec![
            ChannelContent::Text("x".to_string()),
            ChannelContent::Multipart(vec![]), // unknown nesting
            ChannelContent::FileData {
                data: b"f".to_vec(),
                filename: "f.txt".to_string(),
                mime_type: "text/plain".to_string(),
            },
        ]);

        let result = adapter.send(&user, content).await;
        assert!(result.is_ok(), "unknown nested variant must not be fatal");
        let posts = captured.lock().await;
        assert_eq!(posts.len(), 1, "FileData should still be sent");
    }

    // ---- multi-file 429 retry --------------------------------------------------

    /// Spawn a stub at `/channels/test/messages` that returns
    /// `first_response` on attempt 0 and 200 OK on every subsequent attempt.
    /// Captures every POST's parsed multipart fields into the returned
    /// `Arc<...Vec<CapturedPost>>` for assertions. Used by the 429 retry
    /// tests to vary only the 429 response shape (body+header vs header-only)
    /// while sharing the rest of the scaffolding.
    async fn spawn_429_then_ok_stub(
        first_response: Arc<
            dyn Fn() -> axum::response::Response + Send + Sync + 'static,
        >,
    ) -> (String, Arc<TokioMutex<Vec<CapturedPost>>>) {
        use axum::{
            extract::{DefaultBodyLimit, Multipart},
            http::StatusCode,
            response::IntoResponse,
            routing::post,
            Extension, Router,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};

        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let attempt = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let captured_clone = captured.clone();
        let attempt_clone = attempt.clone();
        let app = Router::new()
            .route(
                "/channels/test/messages",
                post(
                    move |Extension(_): Extension<()>, mut multipart: Multipart| {
                        let captured = captured_clone.clone();
                        let attempt = attempt_clone.clone();
                        let first_response = first_response.clone();
                        async move {
                            let n = attempt.fetch_add(1, Ordering::SeqCst);
                            let mut post_rec = CapturedPost::default();
                            while let Ok(Some(field)) = multipart.next_field().await {
                                let name = field.name().unwrap_or("").to_string();
                                if name == "payload_json" {
                                    post_rec.payload_json =
                                        field.text().await.unwrap_or_default();
                                } else {
                                    let filename = field.file_name().map(str::to_string);
                                    let content_type = field.content_type().map(str::to_string);
                                    let _ = field.bytes().await;
                                    post_rec.files.push(CapturedFile {
                                        field_name: name.clone(),
                                        filename,
                                        content_type,
                                    });
                                    post_rec.file_field_names.push(name);
                                }
                            }
                            captured.lock().await.push(post_rec);
                            if n == 0 {
                                first_response()
                            } else {
                                StatusCode::OK.into_response()
                            }
                        }
                    },
                ),
            )
            .layer(DefaultBodyLimit::disable())
            .layer(Extension(()));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), captured)
    }

    /// Build a `ChannelContent::Multipart` with `n` `FileData` blocks named
    /// `a.txt`, `b.txt`, … (for tests that only care about field-name
    /// ordering, not content). `n` must be ≤ 26.
    fn caption_plus_n_files(caption: &str, n: usize) -> ChannelContent {
        assert!(n <= 26, "caption_plus_n_files: n must fit in a-z");
        let mut parts = vec![ChannelContent::Text(caption.to_string())];
        for i in 0..n {
            let ch = (b'a' + i as u8) as char;
            parts.push(ChannelContent::FileData {
                data: vec![ch as u8],
                filename: format!("{ch}.txt"),
                mime_type: "text/plain".to_string(),
            });
        }
        ChannelContent::Multipart(parts)
    }

    /// Assert every captured POST's multipart fields are exactly
    /// `["files[0]", "files[1]", …, "files[n-1]"]`.
    async fn assert_all_attempts_carry_files(
        captured: &Arc<TokioMutex<Vec<CapturedPost>>>,
        n: usize,
    ) {
        let expected: Vec<String> = (0..n).map(|i| format!("files[{i}]")).collect();
        let posts = captured.lock().await;
        for (i, p) in posts.iter().enumerate() {
            assert_eq!(
                p.file_field_names, expected,
                "attempt {i} must include files[0..{n})"
            );
        }
    }

    /// 429 response with both `Retry-After: 0` header and a JSON body
    /// containing `retry_after: 0.0`. Sending a 3-attachment Multipart
    /// must produce exactly 2 POSTs and both must carry the full file set.
    /// Locks in the body-aware retry path (body wins over header per the
    /// adapter's `body_secs.or(header_secs)`).
    #[tokio::test]
    async fn test_multipart_outbound_multifile_429_retries_once() {
        use axum::{http::StatusCode, response::IntoResponse};
        let first: Arc<dyn Fn() -> axum::response::Response + Send + Sync> = Arc::new(|| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, "0")],
                r#"{"retry_after":0.0,"global":false}"#,
            )
                .into_response()
        });
        let (base, captured) = spawn_429_then_ok_stub(first).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        adapter
            .send(&user, caption_plus_n_files("cap", 3))
            .await
            .unwrap();

        assert_eq!(
            captured.lock().await.len(),
            2,
            "expected 2 POSTs (one 429-rejected, one 200) for the same chunk"
        );
        assert_all_attempts_carry_files(&captured, 3).await;
    }

    /// 429 response with **only** the `Retry-After` header (empty body).
    /// The header-fallback path (`body_secs.or(header_secs)`) must still
    /// trigger the retry, so a regression that drops header parsing fails
    /// here independently of the body-present test.
    #[tokio::test]
    async fn test_multipart_outbound_multifile_429_header_only_retries_once() {
        use axum::{http::StatusCode, response::IntoResponse};
        let first: Arc<dyn Fn() -> axum::response::Response + Send + Sync> = Arc::new(|| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, "0")],
                "",
            )
                .into_response()
        });
        let (base, captured) = spawn_429_then_ok_stub(first).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        adapter
            .send(&user, caption_plus_n_files("cap", 2))
            .await
            .unwrap();

        assert_eq!(
            captured.lock().await.len(),
            2,
            "header-only 429 must still trigger one retry"
        );
        assert_all_attempts_carry_files(&captured, 2).await;
    }

    // ---- aggregate per-chunk byte cap ------------------------------------------

    /// Three 10 MiB FileData blocks must split into two chunks under the
    /// 24 MiB per-chunk byte cap (20 MiB + 10 MiB). The caption rides only
    /// on the first chunk; chunk-2 has no caption.
    #[tokio::test]
    async fn test_multipart_outbound_chunking_by_byte_cap() {
        let captured: Arc<TokioMutex<Vec<CapturedPost>>> = Arc::new(TokioMutex::new(Vec::new()));
        let base = spawn_discord_stub(captured.clone()).await;
        let adapter = test_adapter_with_base(base);
        let user = make_channel_user("test");

        let big = vec![0u8; 10 * 1024 * 1024];
        let parts = vec![
            ChannelContent::Text("cap".to_string()),
            ChannelContent::FileData {
                data: big.clone(),
                filename: "a.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
            },
            ChannelContent::FileData {
                data: big.clone(),
                filename: "b.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
            },
            ChannelContent::FileData {
                data: big,
                filename: "c.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
            },
        ];

        adapter
            .send(&user, ChannelContent::Multipart(parts))
            .await
            .unwrap();

        let posts = captured.lock().await;
        assert_eq!(
            posts.len(),
            2,
            "3×10 MiB attachments should split into 2 chunks under the 24 MiB cap"
        );
        // Chunk 1: caption + 2 files (a, b).
        let v0: serde_json::Value = serde_json::from_str(&posts[0].payload_json).unwrap();
        assert_eq!(v0["content"].as_str().unwrap_or(""), "cap");
        assert_eq!(posts[0].files.len(), 2, "first chunk holds first 2 files");
        // Chunk 2: no caption + 1 file (c).
        let v1: serde_json::Value = serde_json::from_str(&posts[1].payload_json).unwrap();
        assert!(
            v1.get("content").is_none(),
            "second chunk must not carry the caption"
        );
        assert_eq!(posts[1].files.len(), 1, "second chunk holds the 3rd file");
    }

    /// Direct unit test of the chunking helper: verifies count cap, byte cap,
    /// and the oversize-single-attachment edge case (lands in its own chunk
    /// instead of stalling progress).
    #[test]
    fn test_chunk_attachments_count_and_byte_caps() {
        // 12 small files → 2 chunks of 10 + 2 (count cap dominates).
        let small: Vec<_> = (0..12)
            .map(|i| {
                (
                    bytes::Bytes::from(vec![0u8; 1024]),
                    format!("f{i}.bin"),
                    "application/octet-stream".to_string(),
                )
            })
            .collect();
        let chunks = chunk_attachments(small);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 10);
        assert_eq!(chunks[1].len(), 2);

        // Three 10 MiB items → 2 chunks (byte cap dominates: 20 + 10 ≤ 24).
        let big_payload = bytes::Bytes::from(vec![0u8; 10 * 1024 * 1024]);
        let big = vec![
            (big_payload.clone(), "a".to_string(), "x".to_string()),
            (big_payload.clone(), "b".to_string(), "x".to_string()),
            (big_payload.clone(), "c".to_string(), "x".to_string()),
        ];
        let chunks = chunk_attachments(big);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 2);
        assert_eq!(chunks[1].len(), 1);

        // One oversized attachment by itself → single chunk holding it.
        // (Discord rejects, but the helper mustn't loop forever or drop it.)
        let oversized = bytes::Bytes::from(vec![0u8; CHUNK_TOTAL_CAP_BYTES + 1]);
        let solo = vec![(oversized, "huge".to_string(), "x".to_string())];
        let chunks = chunk_attachments(solo);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 1);

        // Empty input → no chunks.
        let chunks = chunk_attachments(Vec::new());
        assert!(chunks.is_empty());
    }
}
