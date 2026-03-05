//! Email channel adapter (IMAP + SMTP).
//!
//! Polls IMAP for new emails and sends responses via SMTP using `lettre`.
//! Uses the subject line for agent routing (e.g., "\[coder\] Fix this bug").

use crate::types::{ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use futures::Stream;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::AsyncSmtpTransport;
use lettre::AsyncTransport;
use lettre::Tokio1Executor;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

/// Reply context for email threading (In-Reply-To / Subject continuity).
#[derive(Debug, Clone)]
struct ReplyCtx {
    subject: String,
    message_id: String,
}

/// Email channel adapter using IMAP for receiving and SMTP for sending.
pub struct EmailAdapter {
    /// IMAP server host.
    imap_host: String,
    /// IMAP port (993 for TLS).
    imap_port: u16,
    /// SMTP server host.
    smtp_host: String,
    /// SMTP port (587 for STARTTLS, 465 for implicit TLS).
    smtp_port: u16,
    /// Email address (used for both IMAP and SMTP).
    username: String,
    /// SECURITY: Password is zeroized on drop.
    password: Zeroizing<String>,
    /// How often to check for new emails.
    poll_interval: Duration,
    /// Which IMAP folders to monitor.
    folders: Vec<String>,
    /// Only process emails from these senders (empty = all).
    allowed_senders: Vec<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Tracks reply context per sender for email threading.
    reply_ctx: Arc<DashMap<String, ReplyCtx>>,
}

impl EmailAdapter {
    /// Create a new email adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        imap_host: String,
        imap_port: u16,
        smtp_host: String,
        smtp_port: u16,
        username: String,
        password: String,
        poll_interval_secs: u64,
        folders: Vec<String>,
        allowed_senders: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            imap_host,
            imap_port,
            smtp_host,
            smtp_port,
            username,
            password: Zeroizing::new(password),
            poll_interval: Duration::from_secs(poll_interval_secs),
            folders: if folders.is_empty() {
                vec!["INBOX".to_string()]
            } else {
                folders
            },
            allowed_senders,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            reply_ctx: Arc::new(DashMap::new()),
        }
    }

    /// Check if a sender is in the allowlist (empty = allow all). Used in tests.
    #[allow(dead_code)]
    fn is_allowed_sender(&self, sender: &str) -> bool {
        self.allowed_senders.is_empty() || self.allowed_senders.iter().any(|s| sender.contains(s))
    }

    /// Extract agent name from subject line brackets, e.g., "[coder] Fix the bug" -> Some("coder")
    fn extract_agent_from_subject(subject: &str) -> Option<String> {
        let subject = subject.trim();
        if subject.starts_with('[') {
            if let Some(end) = subject.find(']') {
                let agent = &subject[1..end];
                if !agent.is_empty() {
                    return Some(agent.to_string());
                }
            }
        }
        None
    }

    /// Strip the agent tag from a subject line.
    fn strip_agent_tag(subject: &str) -> String {
        let subject = subject.trim();
        if subject.starts_with('[') {
            if let Some(end) = subject.find(']') {
                return subject[end + 1..].trim().to_string();
            }
        }
        subject.to_string()
    }

    /// Build an async SMTP transport for sending emails.
    async fn build_smtp_transport(
        &self,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, Box<dyn std::error::Error>> {
        let creds = Credentials::new(self.username.clone(), self.password.as_str().to_string());

        let transport = if self.smtp_port == 465 {
            // Implicit TLS (port 465)
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)?
                .port(self.smtp_port)
                .credentials(creds)
                .build()
        } else {
            // STARTTLS (port 587 or other)
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.smtp_host)?
                .port(self.smtp_port)
                .credentials(creds)
                .build()
        };

        Ok(transport)
    }
}

/// Extract `user@domain` from a potentially formatted email string like `"Name <user@domain>"`.
fn extract_email_addr(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(start) = raw.find('<') {
        if let Some(end) = raw.find('>') {
            if end > start {
                return raw[start + 1..end].trim().to_string();
            }
        }
    }
    raw.to_string()
}

/// Get a specific header value from a parsed email.
fn get_header(parsed: &mailparse::ParsedMail<'_>, name: &str) -> Option<String> {
    parsed
        .headers
        .iter()
        .find(|h| h.get_key().eq_ignore_ascii_case(name))
        .map(|h| h.get_value())
}

/// Extract the text/plain body from a parsed email (handles multipart).
fn extract_text_body(parsed: &mailparse::ParsedMail<'_>) -> String {
    if parsed.subparts.is_empty() {
        return parsed.get_body().unwrap_or_default();
    }
    // Walk subparts looking for text/plain
    for part in &parsed.subparts {
        let ct = part.ctype.mimetype.to_lowercase();
        if ct == "text/plain" {
            return part.get_body().unwrap_or_default();
        }
    }
    // Fallback: first subpart body
    parsed
        .subparts
        .first()
        .and_then(|p| p.get_body().ok())
        .unwrap_or_default()
}

/// Fetch unseen emails from IMAP using blocking I/O.
/// Returns a Vec of (from_addr, subject, message_id, body).
fn fetch_unseen_emails(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    folders: &[String],
) -> Result<Vec<(String, String, String, String)>, String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS connector error: {e}"))?;

    let client =
        imap::connect((host, port), host, &tls).map_err(|e| format!("IMAP connect failed: {e}"))?;

    let mut session = client
        .login(username, password)
        .map_err(|(e, _)| format!("IMAP login failed: {e}"))?;

    let mut results = Vec::new();

    for folder in folders {
        if let Err(e) = session.select(folder) {
            warn!(folder, error = %e, "IMAP SELECT failed, skipping folder");
            continue;
        }

        let uids = match session.uid_search("UNSEEN") {
            Ok(uids) => uids,
            Err(e) => {
                warn!(folder, error = %e, "IMAP SEARCH UNSEEN failed");
                continue;
            }
        };

        if uids.is_empty() {
            debug!(folder, "No unseen emails");
            continue;
        }

        // Fetch in batches of up to 50 to avoid huge responses
        let uid_list: Vec<u32> = uids.into_iter().take(50).collect();
        let uid_set: String = uid_list
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let fetches = match session.uid_fetch(&uid_set, "RFC822") {
            Ok(f) => f,
            Err(e) => {
                warn!(folder, error = %e, "IMAP FETCH failed");
                continue;
            }
        };

        for fetch in fetches.iter() {
            let body_bytes = match fetch.body() {
                Some(b) => b,
                None => continue,
            };

            let parsed = match mailparse::parse_mail(body_bytes) {
                Ok(p) => p,
                Err(e) => {
                    warn!(error = %e, "Failed to parse email");
                    continue;
                }
            };

            let from = get_header(&parsed, "From").unwrap_or_default();
            let subject = get_header(&parsed, "Subject").unwrap_or_default();
            let message_id = get_header(&parsed, "Message-ID").unwrap_or_default();
            let text_body = extract_text_body(&parsed);

            let from_addr = extract_email_addr(&from);
            results.push((from_addr, subject, message_id, text_body));
        }

        // Mark fetched messages as Seen
        if let Err(e) = session.uid_store(&uid_set, "+FLAGS (\\Seen)") {
            warn!(error = %e, "Failed to mark emails as Seen");
        }
    }

    let _ = session.logout();
    Ok(results)
}

#[async_trait]
impl ChannelAdapter for EmailAdapter {
    fn name(&self) -> &str {
        "email"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let poll_interval = self.poll_interval;
        let imap_host = self.imap_host.clone();
        let imap_port = self.imap_port;
        let username = self.username.clone();
        let password = self.password.clone();
        let folders = self.folders.clone();
        let allowed_senders = self.allowed_senders.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let reply_ctx = self.reply_ctx.clone();

        info!(
            "Starting email adapter (IMAP: {}:{}, SMTP: {}:{}, polling every {:?})",
            imap_host, imap_port, self.smtp_host, self.smtp_port, poll_interval
        );

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Email adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                // IMAP operations are blocking I/O — run in spawn_blocking
                let host = imap_host.clone();
                let port = imap_port;
                let user = username.clone();
                let pass = password.clone();
                let fldrs = folders.clone();

                let emails = tokio::task::spawn_blocking(move || {
                    fetch_unseen_emails(&host, port, &user, pass.as_str(), &fldrs)
                })
                .await;

                let emails = match emails {
                    Ok(Ok(emails)) => emails,
                    Ok(Err(e)) => {
                        error!("IMAP poll error: {e}");
                        continue;
                    }
                    Err(e) => {
                        error!("IMAP spawn_blocking panic: {e}");
                        continue;
                    }
                };

                for (from_addr, subject, message_id, body) in emails {
                    // Check allowed senders
                    if !allowed_senders.is_empty()
                        && !allowed_senders.iter().any(|s| from_addr.contains(s))
                    {
                        debug!(from = %from_addr, "Email from non-allowed sender, skipping");
                        continue;
                    }

                    // Store reply context for threading
                    if !message_id.is_empty() {
                        reply_ctx.insert(
                            from_addr.clone(),
                            ReplyCtx {
                                subject: subject.clone(),
                                message_id: message_id.clone(),
                            },
                        );
                    }

                    // Extract target agent from subject brackets (stored in metadata for router)
                    let _target_agent = EmailAdapter::extract_agent_from_subject(&subject);
                    let clean_subject = EmailAdapter::strip_agent_tag(&subject);

                    // Build the message body: prepend subject context
                    let text = if clean_subject.is_empty() {
                        body.trim().to_string()
                    } else {
                        format!("Subject: {clean_subject}\n\n{}", body.trim())
                    };

                    let msg = ChannelMessage {
                        channel: ChannelType::Email,
                        platform_message_id: message_id.clone(),
                        sender: ChannelUser {
                            platform_id: from_addr.clone(),
                            display_name: from_addr.clone(),
                            openfang_user: None,
                        },
                        content: ChannelContent::Text(text),
                        target_agent: None, // Routing handled by bridge AgentRouter
                        timestamp: Utc::now(),
                        is_group: false,
                        thread_id: None,
                        metadata: std::collections::HashMap::new(),
                    };

                    if tx.send(msg).await.is_err() {
                        info!("Email channel receiver dropped, stopping poll");
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                // Parse recipient address
                let to_addr = extract_email_addr(&user.platform_id);
                let to_mailbox: Mailbox = to_addr
                    .parse()
                    .map_err(|e| format!("Invalid recipient email '{}': {}", to_addr, e))?;

                let from_mailbox: Mailbox = self
                    .username
                    .parse()
                    .map_err(|e| format!("Invalid sender email '{}': {}", self.username, e))?;

                // Extract subject from text body convention: "Subject: ...\n\n..."
                let (subject, body) = if text.starts_with("Subject: ") {
                    if let Some(pos) = text.find("\n\n") {
                        let subj = text[9..pos].trim().to_string();
                        let body = text[pos + 2..].to_string();
                        (subj, body)
                    } else {
                        ("OpenFang Reply".to_string(), text)
                    }
                } else {
                    // Check reply context for subject continuity
                    let subj = self
                        .reply_ctx
                        .get(&to_addr)
                        .map(|ctx| format!("Re: {}", ctx.subject))
                        .unwrap_or_else(|| "OpenFang Reply".to_string());
                    (subj, text)
                };

                // Build email message
                let mut builder = lettre::Message::builder()
                    .from(from_mailbox)
                    .to(to_mailbox)
                    .subject(&subject);

                // Add In-Reply-To header for threading
                if let Some(ctx) = self.reply_ctx.get(&to_addr) {
                    if !ctx.message_id.is_empty() {
                        builder = builder.in_reply_to(ctx.message_id.clone());
                    }
                }

                let email = builder
                    .body(body)
                    .map_err(|e| format!("Failed to build email: {e}"))?;

                // Send via SMTP
                let transport = self.build_smtp_transport().await?;
                transport
                    .send(email)
                    .await
                    .map_err(|e| format!("SMTP send failed: {e}"))?;

                info!(
                    to = %to_addr,
                    subject = %subject,
                    "Email sent successfully via SMTP"
                );
            }
            _ => {
                warn!(
                    "Unsupported email content type for {}, only text is supported",
                    user.platform_id
                );
            }
        }
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_adapter_creation() {
        let adapter = EmailAdapter::new(
            "imap.gmail.com".to_string(),
            993,
            "smtp.gmail.com".to_string(),
            587,
            "user@gmail.com".to_string(),
            "password".to_string(),
            30,
            vec![],
            vec![],
        );
        assert_eq!(adapter.name(), "email");
        assert_eq!(adapter.folders, vec!["INBOX".to_string()]);
    }

    #[test]
    fn test_allowed_senders() {
        let adapter = EmailAdapter::new(
            "imap.example.com".to_string(),
            993,
            "smtp.example.com".to_string(),
            587,
            "bot@example.com".to_string(),
            "pass".to_string(),
            30,
            vec![],
            vec!["boss@company.com".to_string()],
        );
        assert!(adapter.is_allowed_sender("boss@company.com"));
        assert!(!adapter.is_allowed_sender("random@other.com"));

        let open = EmailAdapter::new(
            "imap.example.com".to_string(),
            993,
            "smtp.example.com".to_string(),
            587,
            "bot@example.com".to_string(),
            "pass".to_string(),
            30,
            vec![],
            vec![],
        );
        assert!(open.is_allowed_sender("anyone@anywhere.com"));
    }

    #[test]
    fn test_extract_agent_from_subject() {
        assert_eq!(
            EmailAdapter::extract_agent_from_subject("[coder] Fix the bug"),
            Some("coder".to_string())
        );
        assert_eq!(
            EmailAdapter::extract_agent_from_subject("[researcher] Find papers on AI"),
            Some("researcher".to_string())
        );
        assert_eq!(
            EmailAdapter::extract_agent_from_subject("No brackets here"),
            None
        );
        assert_eq!(
            EmailAdapter::extract_agent_from_subject("[] Empty brackets"),
            None
        );
    }

    #[test]
    fn test_strip_agent_tag() {
        assert_eq!(
            EmailAdapter::strip_agent_tag("[coder] Fix the bug"),
            "Fix the bug"
        );
        assert_eq!(EmailAdapter::strip_agent_tag("No brackets"), "No brackets");
    }

    #[test]
    fn test_extract_email_addr() {
        assert_eq!(
            extract_email_addr("John Doe <john@example.com>"),
            "john@example.com"
        );
        assert_eq!(extract_email_addr("user@example.com"), "user@example.com");
        assert_eq!(extract_email_addr("<user@test.com>"), "user@test.com");
    }

    #[test]
    fn test_subject_extraction_from_body() {
        let text = "Subject: Test Subject\n\nThis is the body.";
        assert!(text.starts_with("Subject: "));
        let pos = text.find("\n\n").unwrap();
        let subject = &text[9..pos];
        let body = &text[pos + 2..];
        assert_eq!(subject, "Test Subject");
        assert_eq!(body, "This is the body.");
    }

    #[test]
    fn test_reply_ctx_threading() {
        let ctx_map: DashMap<String, ReplyCtx> = DashMap::new();
        ctx_map.insert(
            "user@test.com".to_string(),
            ReplyCtx {
                subject: "Original Subject".to_string(),
                message_id: "<msg-123@test.com>".to_string(),
            },
        );
        let ctx = ctx_map.get("user@test.com").unwrap();
        assert_eq!(ctx.subject, "Original Subject");
        assert_eq!(ctx.message_id, "<msg-123@test.com>");
    }
}
