//! Channel bridge — connects channel adapters to the OpenFang kernel.
//!
//! Defines `ChannelBridgeHandle` (implemented by openfang-api on the kernel) and
//! `BridgeManager` which owns running adapters and dispatches messages.

use crate::formatter;
use crate::router::AgentRouter;
use crate::types::{
    default_phase_emoji, AgentPhase, ChannelAdapter, ChannelContent, ChannelMessage, ChannelUser,
    LifecycleReaction,
};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use openfang_types::agent::AgentId;
use openfang_types::approval::ApprovalRequest;
use openfang_types::config::{ChannelOverrides, DmPolicy, GroupPolicy, OutputFormat};
use openfang_types::message::ContentBlock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatCommandSpec {
    pub name: &'static str,
    pub desc: &'static str,
    pub help: &'static str,
    pub section: &'static str,
}

const CHANNEL_COMMAND_SPECS: &[ChatCommandSpec] = &[
    ChatCommandSpec { name: "start", desc: "Show welcome message", help: "/start - show welcome message", section: "General" },
    ChatCommandSpec { name: "help", desc: "Show available commands", help: "/help - show this help", section: "General" },
    ChatCommandSpec { name: "agents", desc: "List running agents", help: "/agents - list running agents", section: "Session" },
    ChatCommandSpec { name: "agent", desc: "Select agent (/agent <name>)", help: "/agent <name> - select which agent to talk to", section: "Session" },
    ChatCommandSpec { name: "new", desc: "Reset session (clear history)", help: "/new - reset session (clear messages)", section: "Session" },
    ChatCommandSpec { name: "compact", desc: "Trigger LLM session compaction", help: "/compact - trigger LLM session compaction", section: "Session" },
    ChatCommandSpec { name: "model", desc: "Show or switch model", help: "/model [name] - show or switch agent model", section: "Session" },
    ChatCommandSpec { name: "stop", desc: "Cancel current agent run", help: "/stop - cancel current agent run", section: "Session" },
    ChatCommandSpec { name: "usage", desc: "Show session usage and cost", help: "/usage - show session token usage and cost", section: "Session" },
    ChatCommandSpec { name: "think", desc: "Toggle extended thinking", help: "/think [on|off] - toggle extended thinking", section: "Session" },
    ChatCommandSpec { name: "status", desc: "Show system status", help: "/status - show system status", section: "Info" },
    ChatCommandSpec { name: "models", desc: "List available AI models", help: "/models - list available AI models", section: "Info" },
    ChatCommandSpec { name: "providers", desc: "Show configured providers", help: "/providers - show configured providers", section: "Info" },
    ChatCommandSpec { name: "skills", desc: "List installed skills", help: "/skills - list installed skills", section: "Info" },
    ChatCommandSpec { name: "hands", desc: "List available and active hands", help: "/hands - list available and active hands", section: "Info" },
    ChatCommandSpec { name: "workflows", desc: "List workflows", help: "/workflows - list workflows", section: "Automation" },
    ChatCommandSpec { name: "workflow", desc: "Run workflow (/workflow run <name> [input])", help: "/workflow run <name> [input] - run a workflow", section: "Automation" },
    ChatCommandSpec { name: "triggers", desc: "List event triggers", help: "/triggers - list event triggers", section: "Automation" },
    ChatCommandSpec { name: "trigger", desc: "Manage triggers", help: "/trigger add <agent> <pattern> <prompt> | /trigger del <id>", section: "Automation" },
    ChatCommandSpec { name: "schedules", desc: "List cron jobs", help: "/schedules - list cron jobs", section: "Automation" },
    ChatCommandSpec { name: "schedule", desc: "Manage schedules", help: "/schedule add <agent> <cron-5-fields> <message> | /schedule del <id> | /schedule run <id>", section: "Automation" },
    ChatCommandSpec { name: "approvals", desc: "List pending approvals", help: "/approvals - list pending approvals", section: "Automation" },
    ChatCommandSpec { name: "approve", desc: "Approve request", help: "/approve <id> - approve a request", section: "Automation" },
    ChatCommandSpec { name: "reject", desc: "Reject request", help: "/reject <id> - reject a request", section: "Automation" },
    ChatCommandSpec { name: "budget", desc: "Show spending limits and costs", help: "/budget - show spending limits and current costs", section: "Monitoring" },
    ChatCommandSpec { name: "peers", desc: "Show OFP peer network status", help: "/peers - show OFP peer network status", section: "Monitoring" },
    ChatCommandSpec { name: "a2a", desc: "List discovered external A2A agents", help: "/a2a - list discovered external A2A agents", section: "Monitoring" },
];

pub fn channel_command_specs() -> &'static [ChatCommandSpec] {
    CHANNEL_COMMAND_SPECS
}

fn is_channel_command(name: &str) -> bool {
    channel_command_specs().iter().any(|spec| spec.name == name)
}

fn format_channel_help() -> String {
    let sections = ["General", "Session", "Info", "Automation", "Monitoring"];
    let mut msg = String::from("OpenFang Bot Commands:");

    for section in sections {
        let commands: Vec<&ChatCommandSpec> = channel_command_specs()
            .iter()
            .filter(|spec| spec.section == section)
            .collect();
        if commands.is_empty() {
            continue;
        }
        msg.push_str("\n\n");
        msg.push_str(section);
        msg.push_str(":\n");
        for spec in commands {
            msg.push_str(spec.help);
            msg.push('\n');
        }
        msg.pop();
    }

    msg
}

/// Kernel operations needed by channel adapters.
///
/// Defined here to avoid circular deps (openfang-channels can't depend on openfang-kernel).
/// Implemented in openfang-api on the actual kernel.
#[async_trait]
pub trait ChannelBridgeHandle: Send + Sync {
    /// Send a message to an agent and get the text response.
    async fn send_message(&self, agent_id: AgentId, message: &str) -> Result<String, String>;

    /// Send a message with structured content blocks (text + images) to an agent.
    ///
    /// Default implementation extracts text from blocks and falls back to `send_message()`.
    async fn send_message_with_blocks(
        &self,
        agent_id: AgentId,
        blocks: Vec<ContentBlock>,
    ) -> Result<String, String> {
        // Default: extract text from blocks and send as plain text
        let text: String = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.send_message(agent_id, &text).await
    }

    /// Find an agent by name, returning its ID.
    async fn find_agent_by_name(&self, name: &str) -> Result<Option<AgentId>, String>;

    /// List running agents as (id, name) pairs.
    async fn list_agents(&self) -> Result<Vec<(AgentId, String)>, String>;

    /// Spawn an agent by manifest name, returning its ID.
    async fn spawn_agent_by_name(&self, manifest_name: &str) -> Result<AgentId, String>;

    /// Transcribe raw audio bytes to text.
    async fn transcribe_audio(
        &self,
        _audio_bytes: Vec<u8>,
        _mime_type: &str,
    ) -> Result<String, String> {
        Err("Audio transcription not available.".to_string())
    }

    /// List pending approval requests for a specific agent.
    async fn pending_approvals_for_agent(&self, _agent_id: AgentId) -> Vec<ApprovalRequest> {
        Vec::new()
    }

    /// Return uptime info string (e.g., "2h 15m, 5 agents").
    async fn uptime_info(&self) -> String {
        let agents = self.list_agents().await.unwrap_or_default();
        format!("{} agent(s) running", agents.len())
    }

    /// List available models as formatted text for channel display.
    async fn list_models_text(&self) -> String {
        "Model listing not available.".to_string()
    }

    /// List providers and their auth status as formatted text for channel display.
    async fn list_providers_text(&self) -> String {
        "Provider listing not available.".to_string()
    }

    /// Reset an agent's session (clear messages, fresh session ID).
    async fn reset_session(&self, _agent_id: AgentId) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Trigger LLM-based session compaction for an agent.
    async fn compact_session(&self, _agent_id: AgentId) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Set an agent's model.
    async fn set_model(&self, _agent_id: AgentId, _model: &str) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Stop an agent's current LLM run.
    async fn stop_run(&self, _agent_id: AgentId) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Get session token usage and estimated cost.
    async fn session_usage(&self, _agent_id: AgentId) -> Result<String, String> {
        Err("Not implemented".to_string())
    }

    /// Toggle extended thinking mode for an agent.
    async fn set_thinking(&self, _agent_id: AgentId, _on: bool) -> Result<String, String> {
        Ok("Extended thinking preference saved.".to_string())
    }

    /// List installed skills as formatted text for channel display.
    async fn list_skills_text(&self) -> String {
        "Skill listing not available.".to_string()
    }

    /// List hands (marketplace + active) as formatted text for channel display.
    async fn list_hands_text(&self) -> String {
        "Hand listing not available.".to_string()
    }

    /// Authorize a channel user for an action.
    ///
    /// Returns Ok(()) if the user is allowed, Err(reason) if denied.
    /// Default implementation: allow all (RBAC disabled).
    async fn authorize_channel_user(
        &self,
        _channel_type: &str,
        _platform_id: &str,
        _action: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Get per-channel overrides for a given channel type.
    ///
    /// Returns `None` if the channel is not configured or has no overrides.
    async fn channel_overrides(&self, _channel_type: &str) -> Option<ChannelOverrides> {
        None
    }

    /// Get channel IDs that respond without requiring @mention (free response mode).
    ///
    /// Returns an empty vector if the channel type is not configured or has no free response channels.
    async fn free_response_channels(&self, _channel_type: &str) -> Vec<String> {
        Vec::new()
    }

    /// Record a delivery result for tracking (optional — default no-op).
    ///
    /// `thread_id` preserves Telegram forum-topic context so cron/workflow
    /// delivery can target the same topic later.
    async fn record_delivery(
        &self,
        _agent_id: AgentId,
        _channel: &str,
        _recipient: &str,
        _success: bool,
        _error: Option<&str>,
        _thread_id: Option<&str>,
    ) {
        // Default: no tracking
    }

    /// Check if auto-reply is enabled and the message should trigger one.
    /// Returns Some(reply_text) if auto-reply fires, None otherwise.
    async fn check_auto_reply(&self, _agent_id: AgentId, _message: &str) -> Option<String> {
        None
    }

    // ── Automation: workflows, triggers, schedules, approvals ──

    /// List all registered workflows as formatted text.
    async fn list_workflows_text(&self) -> String {
        "Workflows not available.".to_string()
    }

    /// Run a workflow by name with the given input text.
    async fn run_workflow_text(&self, _name: &str, _input: &str) -> String {
        "Workflows not available.".to_string()
    }

    /// List all registered triggers as formatted text.
    async fn list_triggers_text(&self) -> String {
        "Triggers not available.".to_string()
    }

    /// Create a trigger for an agent with the given pattern and prompt.
    async fn create_trigger_text(
        &self,
        _agent_name: &str,
        _pattern: &str,
        _prompt: &str,
    ) -> String {
        "Triggers not available.".to_string()
    }

    /// Delete a trigger by UUID prefix.
    async fn delete_trigger_text(&self, _id_prefix: &str) -> String {
        "Triggers not available.".to_string()
    }

    /// List all cron jobs as formatted text.
    async fn list_schedules_text(&self) -> String {
        "Schedules not available.".to_string()
    }

    /// Manage a cron job: add, del, or run.
    async fn manage_schedule_text(&self, _action: &str, _args: &[String]) -> String {
        "Schedules not available.".to_string()
    }

    /// List pending approval requests as formatted text.
    async fn list_approvals_text(&self) -> String {
        "No approvals pending.".to_string()
    }

    /// Approve or reject a pending approval by UUID prefix.
    async fn resolve_approval_text(&self, _id_prefix: &str, _approve: bool) -> String {
        "Approvals not available.".to_string()
    }

    // ── Budget, Network, A2A ──

    /// Show global budget status (limits, spend, % used).
    async fn budget_text(&self) -> String {
        "Budget information not available.".to_string()
    }

    /// Show OFP peer network status.
    async fn peers_text(&self) -> String {
        "Peer network not available.".to_string()
    }

    /// List discovered external A2A agents.
    async fn a2a_agents_text(&self) -> String {
        "A2A agents not available.".to_string()
    }
}

/// Per-channel rate limiter tracking message timestamps per user.
///
/// Key: `"{channel_type}:{platform_id}"`, Value: timestamps of recent messages.
#[derive(Debug, Clone, Default)]
pub struct ChannelRateLimiter {
    /// Recent message timestamps per user key.
    buckets: Arc<DashMap<String, Vec<Instant>>>,
}

impl ChannelRateLimiter {
    /// Check if a user is rate-limited. Returns `Ok(())` if allowed, `Err(msg)` if blocked.
    ///
    /// `max_per_minute`: 0 means unlimited.
    pub fn check(
        &self,
        channel_type: &str,
        platform_id: &str,
        max_per_minute: u32,
    ) -> Result<(), String> {
        if max_per_minute == 0 {
            return Ok(());
        }

        let key = format!("{channel_type}:{platform_id}");
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);

        let mut entry = self.buckets.entry(key).or_default();
        // Evict timestamps older than 1 minute
        entry.retain(|&ts| now.duration_since(ts) < window);

        if entry.len() >= max_per_minute as usize {
            return Err(format!(
                "Rate limit exceeded ({max_per_minute} messages/minute). Please wait."
            ));
        }

        entry.push(now);
        Ok(())
    }
}

/// Owns all running channel adapters and dispatches messages to agents.
pub struct BridgeManager {
    handle: Arc<dyn ChannelBridgeHandle>,
    router: Arc<AgentRouter>,
    rate_limiter: ChannelRateLimiter,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl BridgeManager {
    pub fn new(handle: Arc<dyn ChannelBridgeHandle>, router: Arc<AgentRouter>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            handle,
            router,
            rate_limiter: ChannelRateLimiter::default(),
            shutdown_tx,
            shutdown_rx,
            tasks: Vec::new(),
        }
    }

    /// Return a reference to the underlying agent router.
    pub fn router(&self) -> &Arc<AgentRouter> {
        &self.router
    }

    /// Start an adapter: subscribe to its message stream and spawn a dispatch task.
    ///
    /// Each incoming message is dispatched as a concurrent task so that slow LLM
    /// calls (10-30s) don't block subsequent messages. This prevents voice/media
    /// messages sent in quick succession from appearing "lost" — all messages
    /// begin processing immediately. Per-agent serialization (to prevent session
    /// corruption) is handled by the kernel's `agent_msg_locks`.
    ///
    /// A semaphore limits concurrent dispatch tasks to prevent unbounded memory
    /// growth under burst traffic.
    pub async fn start_adapter(
        &mut self,
        adapter: Arc<dyn ChannelAdapter>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stream = adapter.start().await?;
        let handle = self.handle.clone();
        let router = self.router.clone();
        let rate_limiter = self.rate_limiter.clone();
        let adapter_clone = adapter.clone();
        let mut shutdown = self.shutdown_rx.clone();

        // Limit concurrent dispatch tasks to prevent unbounded growth.
        // 32 is generous — most setups have 1-5 concurrent users.
        let semaphore = Arc::new(tokio::sync::Semaphore::new(32));

        let task = tokio::spawn(async move {
            let mut stream = std::pin::pin!(stream);
            loop {
                tokio::select! {
                    msg = stream.next() => {
                        match msg {
                            Some(message) => {
                                // Spawn each dispatch as a concurrent task so the stream
                                // loop is never blocked by slow LLM calls. The kernel's
                                // per-agent lock ensures session integrity.
                                let handle = handle.clone();
                                let router = router.clone();
                                let adapter = adapter_clone.clone();
                                let rate_limiter = rate_limiter.clone();
                                let sem = semaphore.clone();
                                tokio::spawn(async move {
                                    // Acquire semaphore permit (blocks if 32 tasks are in flight).
                                    let _permit = match sem.acquire().await {
                                        Ok(p) => p,
                                        Err(_) => return, // semaphore closed — shutting down
                                    };
                                    dispatch_message(
                                        &message,
                                        &handle,
                                        &router,
                                        adapter.as_ref(),
                                        &adapter,
                                        &rate_limiter,
                                    ).await;
                                });
                            }
                            None => {
                                info!("Channel adapter {} stream ended", adapter_clone.name());
                                break;
                            }
                        }
                    }
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            info!("Shutting down channel adapter {}", adapter_clone.name());
                            break;
                        }
                    }
                }
            }
        });

        self.tasks.push(task);
        Ok(())
    }

    /// Stop all adapters and wait for dispatch tasks to finish.
    pub async fn stop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }
}

/// Resolve channel type to its config string key.
fn channel_type_str(channel: &crate::types::ChannelType) -> &str {
    match channel {
        crate::types::ChannelType::Telegram => "telegram",
        crate::types::ChannelType::Discord => "discord",
        crate::types::ChannelType::Slack => "slack",
        crate::types::ChannelType::WhatsApp => "whatsapp",
        crate::types::ChannelType::Signal => "signal",
        crate::types::ChannelType::Matrix => "matrix",
        crate::types::ChannelType::Email => "email",
        crate::types::ChannelType::Teams => "teams",
        crate::types::ChannelType::Mattermost => "mattermost",
        crate::types::ChannelType::WebChat => "webchat",
        crate::types::ChannelType::CLI => "cli",
        crate::types::ChannelType::Custom(s) => s.as_str(),
        _ => "unknown",
    }
}

/// Send a response, applying output formatting and optional threading.
async fn send_response(
    adapter: &dyn ChannelAdapter,
    user: &ChannelUser,
    text: String,
    thread_id: Option<&str>,
    output_format: OutputFormat,
) {
    let formatted = if adapter.name() == "wecom" {
        formatter::format_for_wecom(&text, output_format)
    } else {
        formatter::format_for_channel(&text, output_format)
    };
    let content = ChannelContent::Text(formatted);

    let result = if let Some(tid) = thread_id {
        adapter.send_in_thread(user, content, tid).await
    } else {
        adapter.send(user, content).await
    };

    if let Err(e) = result {
        error!("Failed to send response: {e}");
    }
}

fn default_output_format_for_channel(channel_type: &str) -> OutputFormat {
    match channel_type {
        "telegram" => OutputFormat::TelegramHtml,
        "slack" => OutputFormat::SlackMrkdwn,
        "wecom" => OutputFormat::PlainText,
        _ => OutputFormat::Markdown,
    }
}

/// Send a lifecycle reaction (best-effort, non-blocking for supported adapters).
///
/// Silently ignores errors — reactions are non-critical UX polish.
/// For Telegram, the underlying HTTP call is already fire-and-forget (spawned internally),
/// so this await returns almost immediately.
async fn send_lifecycle_reaction(
    adapter: &dyn ChannelAdapter,
    user: &ChannelUser,
    message_id: &str,
    phase: AgentPhase,
) {
    let reaction = LifecycleReaction {
        emoji: default_phase_emoji(&phase).to_string(),
        phase,
        remove_previous: true,
    };
    let _ = adapter.send_reaction(user, message_id, &reaction).await;
}

/// Spawn a background task that refreshes the typing indicator every 4 seconds.
///
/// Returns a `JoinHandle` that should be aborted once the LLM call completes.
/// Telegram (and similar platforms) expire typing indicators after ~5 seconds,
/// so refreshing at 4-second intervals keeps the indicator alive for the entire
/// duration of long LLM calls.
fn spawn_typing_loop(
    adapter: Arc<dyn ChannelAdapter>,
    sender: ChannelUser,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let _ = adapter.send_typing(&sender).await;
        }
    })
}

/// Extract the sender's user identity from a message.
///
/// Some adapters (e.g. Slack) set `platform_id` to the channel/conversation ID
/// (needed for the send path) and store the actual user ID in metadata.
/// This helper returns the user ID for RBAC and rate limiting.
fn sender_user_id(message: &ChannelMessage) -> &str {
    message
        .metadata
        .get("sender_user_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&message.sender.platform_id)
}

/// If an error contains "Agent not found", try to re-resolve the channel's default agent
/// by name (the name stored at bridge startup). Returns `Some(new_id)` on success.
async fn try_reresolution(
    err: &str,
    channel_key: &str,
    handle: &Arc<dyn ChannelBridgeHandle>,
    router: &Arc<AgentRouter>,
) -> Option<AgentId> {
    if !err.to_lowercase().contains("agent not found") {
        return None;
    }
    let name = router.channel_default_name(channel_key)?;
    info!(
        channel = channel_key,
        agent_name = %name,
        "Agent not found — attempting re-resolution by name"
    );
    match handle.find_agent_by_name(&name).await {
        Ok(Some(new_id)) => {
            router.update_channel_default(channel_key, new_id);
            info!(
                channel = channel_key,
                agent_name = %name,
                new_id = %new_id,
                "Re-resolved agent successfully"
            );
            Some(new_id)
        }
        _ => {
            warn!(
                channel = channel_key,
                agent_name = %name,
                "Re-resolution failed — agent not found by name"
            );
            None
        }
    }
}

/// Dispatch a single incoming message — handles bot commands or routes to an agent.
///
/// Applies per-channel policies (DM/group filtering, rate limiting, formatting, threading).
async fn dispatch_message(
    message: &ChannelMessage,
    handle: &Arc<dyn ChannelBridgeHandle>,
    router: &Arc<AgentRouter>,
    adapter: &dyn ChannelAdapter,
    adapter_arc: &Arc<dyn ChannelAdapter>,
    rate_limiter: &ChannelRateLimiter,
) {
    let ct_str = channel_type_str(&message.channel);

    // Fetch per-channel overrides (if configured)
    let overrides = handle.channel_overrides(ct_str).await;
    let channel_default_format = default_output_format_for_channel(ct_str);
    let output_format = overrides
        .as_ref()
        .and_then(|o| o.output_format)
        .unwrap_or(channel_default_format);
    let threading_enabled = overrides.as_ref().map(|o| o.threading).unwrap_or(false);
    let lifecycle_reactions = overrides
        .as_ref()
        .map(|o| o.lifecycle_reactions)
        .unwrap_or(true);
    let thread_id = if threading_enabled {
        message.thread_id.as_deref()
    } else {
        None
    };

    // --- DM/Group policy check ---
    if let Some(ref ov) = overrides {
        if message.is_group {
            match ov.group_policy {
                GroupPolicy::Ignore => {
                    debug!("Ignoring group message on {ct_str} (group_policy=ignore)");
                    return;
                }
                GroupPolicy::CommandsOnly => {
                    // Only allow slash commands and ChannelContent::Command
                    let is_command = matches!(&message.content, ChannelContent::Command { .. })
                        || matches!(&message.content, ChannelContent::Text(t) if t.starts_with('/'));
                    if !is_command {
                        debug!("Ignoring non-command group message on {ct_str} (group_policy=commands_only)");
                        return;
                    }
                }
                GroupPolicy::MentionOnly => {
                    // Check if this channel is in the free_response list - if so, allow all messages
                    let free_channels = handle.free_response_channels(ct_str).await;
                    let channel_id = &message.sender.platform_id;
                    let is_free_channel = free_channels.iter().any(|id| id == channel_id);

                    if !is_free_channel {
                        // Only allow messages where the bot was @mentioned or commands.
                        let was_mentioned = message
                            .metadata
                            .get("was_mentioned")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let is_command = matches!(&message.content, ChannelContent::Command { .. });
                        if !was_mentioned && !is_command {
                            debug!("Ignoring group message on {ct_str} (group_policy=mention_only, not mentioned)");
                            return;
                        }
                    }
                }
                GroupPolicy::All => {}
            }
        } else {
            // DM
            match ov.dm_policy {
                DmPolicy::Ignore => {
                    debug!("Ignoring DM on {ct_str} (dm_policy=ignore)");
                    return;
                }
                DmPolicy::AllowedOnly => {
                    // Rely on RBAC authorize_channel_user below
                }
                DmPolicy::Respond => {}
            }
        }
    }

    // --- Rate limiting ---
    if let Some(ref ov) = overrides {
        if ov.rate_limit_per_user > 0 {
            if let Err(msg) =
                rate_limiter.check(ct_str, sender_user_id(message), ov.rate_limit_per_user)
            {
                send_response(adapter, &message.sender, msg, thread_id, output_format).await;
                return;
            }
        }
    }

    // Handle commands first (early return)
    if let ChannelContent::Command { ref name, ref args } = message.content {
        let result = handle_command(name, args, handle, router, &message.sender).await;
        send_response(adapter, &message.sender, result, thread_id, output_format).await;
        return;
    }

    // For images: download, base64 encode, and send as multimodal content blocks
    if let ChannelContent::Image {
        ref url,
        ref caption,
    } = message.content
    {
        let blocks = download_image_to_blocks(url, caption.as_deref()).await;
        if blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::Image { .. }))
        {
            // We have actual image data — send as structured blocks for vision
            dispatch_with_blocks(
                blocks,
                message,
                handle,
                router,
                adapter,
                adapter_arc,
                ct_str,
                thread_id,
                output_format,
                lifecycle_reactions,
            )
            .await;
            return;
        }
        // Image download failed — fall through to text description below
    }

    let text = match &message.content {
        ChannelContent::Text(t) => t.clone(),
        ChannelContent::Command { .. } => unreachable!(), // handled above
        ChannelContent::Image {
            ref url,
            ref caption,
        } => {
            // Fallback when image download failed
            match caption {
                Some(c) => format!("[User sent a photo: {url}]\nCaption: {c}"),
                None => format!("[User sent a photo: {url}]"),
            }
        }
        ChannelContent::File {
            ref url,
            ref filename,
        } => {
            format!("[User sent a file ({filename}): {url}]")
        }
        ChannelContent::Voice {
            ref url,
            duration_seconds,
        } => {
            format!("[User sent a voice message ({duration_seconds}s): {url}]")
        }
        ChannelContent::Location { lat, lon } => {
            format!("[User shared location: {lat}, {lon}]")
        }
        ChannelContent::FileData { ref filename, .. } => {
            format!("[User sent a local file: {filename}]")
        }
    };

    // Check if it's a slash command embedded in text (e.g. "/agents")
    if text.starts_with('/') {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd = &parts[0][1..]; // strip leading '/'
        let args: Vec<String> = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };

        if is_channel_command(cmd) {
            let result = handle_command(cmd, &args, handle, router, &message.sender).await;
            send_response(adapter, &message.sender, result, thread_id, output_format).await;
            return;
        }
        // Other slash commands pass through to the agent
    }

    // Check broadcast routing first
    if router.has_broadcast(&message.sender.platform_id) {
        let targets = router.resolve_broadcast(&message.sender.platform_id);
        if !targets.is_empty() {
            // RBAC check applies to broadcast too
            if let Err(denied) = handle
                .authorize_channel_user(ct_str, sender_user_id(message), "chat")
                .await
            {
                send_response(
                    adapter,
                    &message.sender,
                    format!("Access denied: {denied}"),
                    thread_id,
                    output_format,
                )
                .await;
                return;
            }
            let _ = adapter.send_typing(&message.sender).await;

            let typing_task = spawn_typing_loop(adapter_arc.clone(), message.sender.clone());

            let strategy = router.broadcast_strategy();
            let mut responses = Vec::new();

            match strategy {
                openfang_types::config::BroadcastStrategy::Parallel => {
                    let mut handles_vec = Vec::new();
                    for (name, maybe_id) in &targets {
                        if let Some(aid) = maybe_id {
                            let h = handle.clone();
                            let t = text.clone();
                            let aid = *aid;
                            let name = name.clone();
                            handles_vec.push(tokio::spawn(async move {
                                let result = h.send_message(aid, &t).await;
                                (name, aid, result)
                            }));
                        }
                    }
                    for jh in handles_vec {
                        if let Ok((name, _aid, result)) = jh.await {
                            match result {
                                Ok(r) => responses.push(format!("[{name}]: {r}")),
                                Err(e) => responses.push(format!("[{name}]: Error: {e}")),
                            }
                        }
                    }
                }
                openfang_types::config::BroadcastStrategy::Sequential => {
                    for (name, maybe_id) in &targets {
                        if let Some(aid) = maybe_id {
                            match handle.send_message(*aid, &text).await {
                                Ok(r) => responses.push(format!("[{name}]: {r}")),
                                Err(e) => responses.push(format!("[{name}]: Error: {e}")),
                            }
                        }
                    }
                }
            }

            typing_task.abort();

            let combined = responses.join("\n\n");
            send_response(adapter, &message.sender, combined, thread_id, output_format).await;
            return;
        }
    }

    // Route to agent (standard path)
    let agent_id = router.resolve(
        &message.channel,
        &message.sender.platform_id,
        message.sender.openfang_user.as_deref(),
    );

    let agent_id = match agent_id {
        Some(id) => id,
        None => {
            // Fallback: try "assistant" agent, then first available agent
            let fallback = handle.find_agent_by_name("assistant").await.ok().flatten();
            let fallback = match fallback {
                Some(id) => Some(id),
                None => handle
                    .list_agents()
                    .await
                    .ok()
                    .and_then(|agents| agents.first().map(|(id, _)| *id)),
            };
            match fallback {
                Some(id) => {
                    // Auto-set this as the user's default so future messages route directly
                    router.set_user_default(message.sender.platform_id.clone(), id);
                    id
                }
                None => {
                    send_response(
                        adapter,
                        &message.sender,
                        "No agents available. Start the dashboard at http://127.0.0.1:4200 to create one.".to_string(),
                        thread_id,
                        output_format,
                    ).await;
                    return;
                }
            }
        }
    };

    // RBAC: authorize the user before forwarding to agent
    if let Err(denied) = handle
        .authorize_channel_user(ct_str, sender_user_id(message), "chat")
        .await
    {
        send_response(
            adapter,
            &message.sender,
            format!("Access denied: {denied}"),
            thread_id,
            output_format,
        )
        .await;
        return;
    }

    // Build channel key for re-resolution lookups
    let channel_key = format!("{:?}", message.channel);

    // Auto-reply check — if enabled, the engine decides whether to process this message.
    // If auto-reply is enabled but suppressed for this message, skip agent call entirely.
    if let Some(reply) = handle.check_auto_reply(agent_id, &text).await {
        send_response(adapter, &message.sender, reply, thread_id, output_format).await;
        handle
            .record_delivery(
                agent_id,
                ct_str,
                &message.sender.platform_id,
                true,
                None,
                thread_id,
            )
            .await;
        return;
    }

    // Send typing indicator (best-effort)
    let _ = adapter.send_typing(&message.sender).await;

    // Lifecycle reaction: ⏳ Queued → 🤔 Thinking → ✅ Done / ❌ Error
    let msg_id = &message.platform_message_id;
    if lifecycle_reactions {
        send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Queued).await;
        send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Thinking).await;
    }

    // Continuous typing indicator — refreshes every 4s so platforms like Telegram
    // (which expire typing after ~5s) keep showing it during long LLM calls.
    let typing_task = spawn_typing_loop(adapter_arc.clone(), message.sender.clone());

    // Prepend sender context so the agent knows who is speaking.
    // In group spaces this is essential for multi-user conversations.
    let sender_name = &message.sender.display_name;
    let sender_email = message
        .metadata
        .get("sender_email")
        .and_then(|v| v.as_str());
    let prefixed_text = if !sender_name.is_empty() {
        match sender_email {
            Some(email) => format!("[From: {sender_name} <{email}>] {text}"),
            None => format!("[From: {sender_name}] {text}"),
        }
    } else {
        text.clone()
    };

    // Send to agent and relay response
    let result = handle.send_message(agent_id, &prefixed_text).await;

    // Stop the typing refresh now that we have a response
    typing_task.abort();

    match result {
        Ok(response) => {
            if lifecycle_reactions {
                send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Done).await;
            }
            send_response(adapter, &message.sender, response, thread_id, output_format).await;
            handle
                .record_delivery(
                    agent_id,
                    ct_str,
                    &message.sender.platform_id,
                    true,
                    None,
                    thread_id,
                )
                .await;
        }
        Err(e) => {
            // Try re-resolution before reporting error
            if let Some(new_id) = try_reresolution(&e, &channel_key, handle, router).await {
                let typing_task2 = spawn_typing_loop(adapter_arc.clone(), message.sender.clone());
                let retry = handle.send_message(new_id, &text).await;
                typing_task2.abort();
                match retry {
                    Ok(response) => {
                        if lifecycle_reactions {
                            send_lifecycle_reaction(
                                adapter,
                                &message.sender,
                                msg_id,
                                AgentPhase::Done,
                            )
                            .await;
                        }
                        send_response(adapter, &message.sender, response, thread_id, output_format)
                            .await;
                        handle
                            .record_delivery(
                                new_id,
                                ct_str,
                                &message.sender.platform_id,
                                true,
                                None,
                                thread_id,
                            )
                            .await;
                    }
                    Err(e2) => {
                        if lifecycle_reactions {
                            send_lifecycle_reaction(
                                adapter,
                                &message.sender,
                                msg_id,
                                AgentPhase::Error,
                            )
                            .await;
                        }
                        warn!("Agent error after re-resolution for {new_id}: {e2}");
                        let err_msg = sanitize_agent_error(&e2.to_string());
                        if !adapter.suppress_error_responses() {
                            send_response(
                                adapter,
                                &message.sender,
                                err_msg.clone(),
                                thread_id,
                                output_format,
                            )
                            .await;
                        }
                        handle
                            .record_delivery(
                                new_id,
                                ct_str,
                                &message.sender.platform_id,
                                false,
                                Some(&err_msg),
                                thread_id,
                            )
                            .await;
                    }
                }
                return;
            }

            if lifecycle_reactions {
                send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Error).await;
            }
            warn!("Agent error for {agent_id}: {e}");
            let err_msg = sanitize_agent_error(&e.to_string());
            if !adapter.suppress_error_responses() {
                send_response(
                    adapter,
                    &message.sender,
                    err_msg.clone(),
                    thread_id,
                    output_format,
                )
                .await;
            }
            handle
                .record_delivery(
                    agent_id,
                    ct_str,
                    &message.sender.platform_id,
                    false,
                    Some(&err_msg),
                    thread_id,
                )
                .await;
        }
    }
}

fn sanitize_agent_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    if lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("429")
        || lower.contains("too many requests")
        || lower.contains("resource_exhausted")
    {
        return "Rate limit reached, please try again later.".to_string();
    }

    if lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid x-goog-api-key")
        || lower.contains("incorrect api key")
        || lower.contains("permission denied")
        || lower.contains("billing")
        || lower.contains("quota exceeded")
    {
        return "Service temporarily unavailable.".to_string();
    }

    if lower.contains("context length")
        || lower.contains("token limit")
        || lower.contains("too many tokens")
        || lower.contains("maximum context")
        || lower.contains("max_tokens")
        || lower.contains("context window")
    {
        return "Message too long, try a shorter request.".to_string();
    }

    if lower.contains("overloaded")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("server error")
        || lower.contains("internal error")
    {
        return "The AI service is temporarily overloaded, please try again shortly.".to_string();
    }

    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline") {
        return "Request timed out, please try again.".to_string();
    }

    if lower.contains("model not found") || lower.contains("model_not_found") {
        return "The requested model is currently unavailable.".to_string();
    }

    let cleaned = raw
        .strip_prefix("LLM driver error: ")
        .or_else(|| raw.strip_prefix("Agent error: "))
        .unwrap_or(raw);

    if let Some(first_sentence_end) = cleaned.find(". ") {
        let first = &cleaned[..=first_sentence_end];
        if first.len() < cleaned.len() / 2 {
            return format!("Agent error: {first}");
        }
    }

    if cleaned.contains('{') || cleaned.len() > 200 {
        return "Something went wrong processing your request. Please try again.".to_string();
    }

    format!("Agent error: {cleaned}")
}

/// Detect image format from the first few magic bytes.
///
/// Returns `Some("image/...")` for JPEG, PNG, GIF, and WebP.
fn detect_image_magic(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return Some("image/jpeg".to_string());
    }
    if bytes.len() >= 4 && bytes[..4] == [0x89, 0x50, 0x4E, 0x47] {
        return Some("image/png".to_string());
    }
    if bytes.len() >= 4 && bytes[..4] == [0x47, 0x49, 0x46, 0x38] {
        return Some("image/gif".to_string());
    }
    if bytes.len() >= 12
        && bytes[..4] == [0x52, 0x49, 0x46, 0x46]
        && bytes[8..12] == [0x57, 0x45, 0x42, 0x50]
    {
        return Some("image/webp".to_string());
    }
    None
}

/// Guess image media type from the URL file extension.
fn media_type_from_url(url: &str) -> String {
    if url.contains(".png") {
        "image/png".to_string()
    } else if url.contains(".gif") {
        "image/gif".to_string()
    } else if url.contains(".webp") {
        "image/webp".to_string()
    } else {
        // JPEG is the most common image format — safe default
        "image/jpeg".to_string()
    }
}

/// Download an image from a URL and build content blocks for multimodal LLM input.
///
/// Returns a `Vec<ContentBlock>` containing an image block (base64-encoded) and
/// optionally a text block for the caption. If the download fails, returns a
/// text-only block describing the failure.
async fn download_image_to_blocks(url: &str, caption: Option<&str>) -> Vec<ContentBlock> {
    use base64::Engine;

    // 5 MB limit to prevent memory abuse from oversized images
    const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

    let client = reqwest::Client::new();
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to download image from channel: {e}");
            return vec![ContentBlock::Text {
                text: format!("[Image download failed: {e}]"),
                provider_metadata: None,
            }];
        }
    };

    // Detect media type from Content-Type header — but only trust it if it's
    // actually an image/* type. Many APIs (Telegram, S3 pre-signed URLs) return
    // `application/octet-stream` for all files, which breaks vision.
    let header_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.split(';').next().unwrap_or(ct).trim().to_string())
        .filter(|ct| ct.starts_with("image/"));

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read image bytes: {e}");
            return vec![ContentBlock::Text {
                text: format!("[Image read failed: {e}]"),
                provider_metadata: None,
            }];
        }
    };

    // Three-tier media type detection:
    // 1. Trusted Content-Type header (only if image/*)
    // 2. Magic byte sniffing (most reliable for binary data)
    // 3. URL extension fallback
    let media_type = header_type
        .unwrap_or_else(|| detect_image_magic(&bytes).unwrap_or_else(|| media_type_from_url(url)));

    if bytes.len() > MAX_IMAGE_BYTES {
        warn!(
            "Image too large ({} bytes), skipping vision — sending as text",
            bytes.len()
        );
        let desc = match caption {
            Some(c) => format!(
                "[Image too large for vision ({} KB)]\nCaption: {c}",
                bytes.len() / 1024
            ),
            None => format!("[Image too large for vision ({} KB)]", bytes.len() / 1024),
        };
        return vec![ContentBlock::Text {
            text: desc,
            provider_metadata: None,
        }];
    }

    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let mut blocks = Vec::new();

    // Caption as text block first (gives the LLM context about the image)
    if let Some(cap) = caption {
        if !cap.is_empty() {
            blocks.push(ContentBlock::Text {
                text: cap.to_string(),
                provider_metadata: None,
            });
        }
    }

    blocks.push(ContentBlock::Image { media_type, data });

    blocks
}

/// Dispatch a multimodal message (content blocks) to an agent, handling routing
/// and RBAC the same way as the text path.
#[allow(clippy::too_many_arguments)]
async fn dispatch_with_blocks(
    blocks: Vec<ContentBlock>,
    message: &ChannelMessage,
    handle: &Arc<dyn ChannelBridgeHandle>,
    router: &Arc<AgentRouter>,
    adapter: &dyn ChannelAdapter,
    adapter_arc: &Arc<dyn ChannelAdapter>,
    ct_str: &str,
    thread_id: Option<&str>,
    output_format: OutputFormat,
    lifecycle_reactions: bool,
) {
    // Route to agent (same logic as text path)
    let agent_id = router.resolve(
        &message.channel,
        &message.sender.platform_id,
        message.sender.openfang_user.as_deref(),
    );

    let agent_id = match agent_id {
        Some(id) => id,
        None => {
            let fallback = handle.find_agent_by_name("assistant").await.ok().flatten();
            let fallback = match fallback {
                Some(id) => Some(id),
                None => handle
                    .list_agents()
                    .await
                    .ok()
                    .and_then(|agents| agents.first().map(|(id, _)| *id)),
            };
            match fallback {
                Some(id) => {
                    router.set_user_default(message.sender.platform_id.clone(), id);
                    id
                }
                None => {
                    send_response(
                        adapter,
                        &message.sender,
                        "No agents available. Start the dashboard at http://127.0.0.1:4200 to create one.".to_string(),
                        thread_id,
                        output_format,
                    ).await;
                    return;
                }
            }
        }
    };

    // Build channel key for re-resolution lookups
    let channel_key = format!("{:?}", message.channel);

    // RBAC check
    if let Err(denied) = handle
        .authorize_channel_user(ct_str, sender_user_id(message), "chat")
        .await
    {
        send_response(
            adapter,
            &message.sender,
            format!("Access denied: {denied}"),
            thread_id,
            output_format,
        )
        .await;
        return;
    }

    let _ = adapter.send_typing(&message.sender).await;

    // Lifecycle reaction: ⏳ Queued → 🤔 Thinking → ✅ Done / ❌ Error
    let msg_id = &message.platform_message_id;
    if lifecycle_reactions {
        send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Queued).await;
        send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Thinking).await;
    }

    // Continuous typing indicator (see spawn_typing_loop doc)
    let typing_task = spawn_typing_loop(adapter_arc.clone(), message.sender.clone());

    let result = handle
        .send_message_with_blocks(agent_id, blocks.clone())
        .await;

    typing_task.abort();

    match result {
        Ok(response) => {
            if lifecycle_reactions {
                send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Done).await;
            }
            send_response(adapter, &message.sender, response, thread_id, output_format).await;
            handle
                .record_delivery(
                    agent_id,
                    ct_str,
                    &message.sender.platform_id,
                    true,
                    None,
                    thread_id,
                )
                .await;
        }
        Err(e) => {
            // Try re-resolution before reporting error
            if let Some(new_id) = try_reresolution(&e, &channel_key, handle, router).await {
                let typing_task2 = spawn_typing_loop(adapter_arc.clone(), message.sender.clone());
                let retry = handle.send_message_with_blocks(new_id, blocks).await;
                typing_task2.abort();
                match retry {
                    Ok(response) => {
                        if lifecycle_reactions {
                            send_lifecycle_reaction(
                                adapter,
                                &message.sender,
                                msg_id,
                                AgentPhase::Done,
                            )
                            .await;
                        }
                        send_response(adapter, &message.sender, response, thread_id, output_format)
                            .await;
                        handle
                            .record_delivery(
                                new_id,
                                ct_str,
                                &message.sender.platform_id,
                                true,
                                None,
                                thread_id,
                            )
                            .await;
                    }
                    Err(e2) => {
                        if lifecycle_reactions {
                            send_lifecycle_reaction(
                                adapter,
                                &message.sender,
                                msg_id,
                                AgentPhase::Error,
                            )
                            .await;
                        }
                        warn!("Agent error after re-resolution for {new_id}: {e2}");
                        let err_msg = sanitize_agent_error(&e2.to_string());
                        if !adapter.suppress_error_responses() {
                            send_response(
                                adapter,
                                &message.sender,
                                err_msg.clone(),
                                thread_id,
                                output_format,
                            )
                            .await;
                        }
                        handle
                            .record_delivery(
                                new_id,
                                ct_str,
                                &message.sender.platform_id,
                                false,
                                Some(&err_msg),
                                thread_id,
                            )
                            .await;
                    }
                }
                return;
            }

            if lifecycle_reactions {
                send_lifecycle_reaction(adapter, &message.sender, msg_id, AgentPhase::Error).await;
            }
            warn!("Agent error for {agent_id}: {e}");
            let err_msg = sanitize_agent_error(&e.to_string());
            if !adapter.suppress_error_responses() {
                send_response(
                    adapter,
                    &message.sender,
                    err_msg.clone(),
                    thread_id,
                    output_format,
                )
                .await;
            }
            handle
                .record_delivery(
                    agent_id,
                    ct_str,
                    &message.sender.platform_id,
                    false,
                    Some(&err_msg),
                    thread_id,
                )
                .await;
        }
    }
}

/// Handle a bot command (returns the response text).
async fn handle_command(
    name: &str,
    args: &[String],
    handle: &Arc<dyn ChannelBridgeHandle>,
    router: &Arc<AgentRouter>,
    sender: &ChannelUser,
) -> String {
    match name {
        "start" => {
            let agents = handle.list_agents().await.unwrap_or_default();
            let mut msg = "Welcome to OpenFang! I connect you to AI agents.\n\nAvailable agents:\n"
                .to_string();
            if agents.is_empty() {
                msg.push_str("  (none running)\n");
            } else {
                for (_, name) in &agents {
                    msg.push_str(&format!("  - {name}\n"));
                }
            }
            msg.push_str("\nCommands:\n/agents - list agents\n/agent <name> - select an agent\n/help - show this help");
            msg
        }
        "help" => format_channel_help(),
        "status" => handle.uptime_info().await,
        "agents" => {
            let agents = handle.list_agents().await.unwrap_or_default();
            if agents.is_empty() {
                "No agents running.".to_string()
            } else {
                let mut msg = "Running agents:\n".to_string();
                for (_, name) in &agents {
                    msg.push_str(&format!("  - {name}\n"));
                }
                msg
            }
        }
        "agent" => {
            if args.is_empty() {
                let agents = handle.list_agents().await.unwrap_or_default();
                if agents.is_empty() {
                    return "No agents running. Usage: /agent <name>".to_string();
                }
                let mut msg = "Usage: /agent <name>\n\nAvailable agents:\n".to_string();
                for (_, name) in &agents {
                    msg.push_str(&format!("  - {name}\n"));
                }
                return msg.trim_end().to_string();
            }
            let agent_name = &args[0];
            match handle.find_agent_by_name(agent_name).await {
                Ok(Some(agent_id)) => {
                    router.set_user_default(sender.platform_id.clone(), agent_id);
                    format!("Now talking to agent: {agent_name}")
                }
                Ok(None) => {
                    // Try to spawn it
                    match handle.spawn_agent_by_name(agent_name).await {
                        Ok(agent_id) => {
                            router.set_user_default(sender.platform_id.clone(), agent_id);
                            format!("Spawned and connected to agent: {agent_name}")
                        }
                        Err(e) => {
                            format!("Agent '{agent_name}' not found and could not spawn: {e}")
                        }
                    }
                }
                Err(e) => format!("Error finding agent: {e}"),
            }
        }
        "new" => {
            // Need to resolve the user's current agent
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => handle
                    .reset_session(aid)
                    .await
                    .unwrap_or_else(|e| format!("Error: {e}")),
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "compact" => {
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => handle
                    .compact_session(aid)
                    .await
                    .unwrap_or_else(|e| format!("Error: {e}")),
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "model" => {
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => {
                    if args.is_empty() {
                        // Show current model
                        handle
                            .set_model(aid, "")
                            .await
                            .unwrap_or_else(|e| format!("Error: {e}"))
                    } else {
                        handle
                            .set_model(aid, &args[0])
                            .await
                            .unwrap_or_else(|e| format!("Error: {e}"))
                    }
                }
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "stop" => {
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => handle
                    .stop_run(aid)
                    .await
                    .unwrap_or_else(|e| format!("Error: {e}")),
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "usage" => {
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => handle
                    .session_usage(aid)
                    .await
                    .unwrap_or_else(|e| format!("Error: {e}")),
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "think" => {
            let agent_id = router.resolve(
                &crate::types::ChannelType::CLI,
                &sender.platform_id,
                sender.openfang_user.as_deref(),
            );
            match agent_id {
                Some(aid) => {
                    let on = args.first().map(|a| a == "on").unwrap_or(true);
                    handle
                        .set_thinking(aid, on)
                        .await
                        .unwrap_or_else(|e| format!("Error: {e}"))
                }
                None => "No agent selected. Use /agent <name> first.".to_string(),
            }
        }
        "models" => handle.list_models_text().await,
        "providers" => handle.list_providers_text().await,
        "skills" => handle.list_skills_text().await,
        "hands" => handle.list_hands_text().await,

        // ── Automation: workflows, triggers, schedules, approvals ──
        "workflows" => handle.list_workflows_text().await,
        "workflow" => {
            if args.len() >= 2 && args[0] == "run" {
                let wf_name = &args[1];
                let input = if args.len() > 2 {
                    args[2..].join(" ")
                } else {
                    String::new()
                };
                handle.run_workflow_text(wf_name, &input).await
            } else {
                "Usage: /workflow run <name> [input]".to_string()
            }
        }
        "triggers" => handle.list_triggers_text().await,
        "trigger" => {
            if args.len() >= 4 && args[0] == "add" {
                let agent_name = &args[1];
                let pattern = &args[2];
                let prompt = args[3..].join(" ");
                handle
                    .create_trigger_text(agent_name, pattern, &prompt)
                    .await
            } else if args.len() >= 2 && args[0] == "del" {
                handle.delete_trigger_text(&args[1]).await
            } else {
                "Usage:\n  /trigger add <agent> <pattern> <prompt>\n  /trigger del <id-prefix>"
                    .to_string()
            }
        }
        "schedules" => handle.list_schedules_text().await,
        "schedule" => {
            if args.is_empty() {
                return "Usage:\n  /schedule add <agent> <cron-5-fields> <message>\n  /schedule del <id-prefix>\n  /schedule run <id-prefix>".to_string();
            }
            let action = args[0].as_str();
            match action {
                "add" | "del" | "run" => {
                    handle.manage_schedule_text(action, &args[1..]).await
                }
                _ => "Usage:\n  /schedule add <agent> <cron-5-fields> <message>\n  /schedule del <id-prefix>\n  /schedule run <id-prefix>".to_string(),
            }
        }
        "approvals" => handle.list_approvals_text().await,
        "approve" => {
            if args.is_empty() {
                "Usage: /approve <id-prefix>".to_string()
            } else {
                handle.resolve_approval_text(&args[0], true).await
            }
        }
        "reject" => {
            if args.is_empty() {
                "Usage: /reject <id-prefix>".to_string()
            } else {
                handle.resolve_approval_text(&args[0], false).await
            }
        }

        // ── Budget, Network, A2A ──
        "budget" => handle.budget_text().await,
        "peers" => handle.peers_text().await,
        "a2a" => handle.a2a_agents_text().await,

        _ => format!("Unknown command: /{name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChannelType;
    use std::sync::Mutex;

    /// Mock kernel handle for testing.
    struct MockHandle {
        agents: Mutex<Vec<(AgentId, String)>>,
    }

    #[async_trait]
    impl ChannelBridgeHandle for MockHandle {
        async fn send_message(&self, _agent_id: AgentId, message: &str) -> Result<String, String> {
            Ok(format!("Echo: {message}"))
        }
        async fn find_agent_by_name(&self, name: &str) -> Result<Option<AgentId>, String> {
            let agents = self.agents.lock().unwrap();
            Ok(agents.iter().find(|(_, n)| n == name).map(|(id, _)| *id))
        }
        async fn list_agents(&self) -> Result<Vec<(AgentId, String)>, String> {
            Ok(self.agents.lock().unwrap().clone())
        }
        async fn spawn_agent_by_name(&self, _manifest_name: &str) -> Result<AgentId, String> {
            Err("spawn not implemented in mock".to_string())
        }
    }

    #[test]
    fn test_command_parsing() {
        // Verify slash commands are parsed correctly from text
        let text = "/agent hello-world";
        assert!(text.starts_with('/'));
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd = &parts[0][1..];
        assert_eq!(cmd, "agent");
        let args: Vec<String> = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };
        assert_eq!(args, vec!["hello-world"]);
    }

    #[tokio::test]
    async fn test_dispatch_routes_to_correct_agent() {
        let agent_id = AgentId::new();
        let mock = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "test-agent".to_string())]),
        });

        let handle: Arc<dyn ChannelBridgeHandle> = mock;

        // Verify find_agent_by_name works
        let found = handle.find_agent_by_name("test-agent").await.unwrap();
        assert_eq!(found, Some(agent_id));

        let not_found = handle.find_agent_by_name("nonexistent").await.unwrap();
        assert_eq!(not_found, None);

        // Verify send_message echoes
        let response = handle.send_message(agent_id, "hello").await.unwrap();
        assert_eq!(response, "Echo: hello");
    }

    #[tokio::test]
    async fn test_handle_command_agents() {
        let agent_id = AgentId::new();
        let handle: Arc<dyn ChannelBridgeHandle> = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "coder".to_string())]),
        });
        let router = Arc::new(AgentRouter::new());
        let sender = ChannelUser {
            platform_id: "user1".to_string(),
            display_name: "Test".to_string(),
            openfang_user: None,
        };

        let result = handle_command("agents", &[], &handle, &router, &sender).await;
        assert!(result.contains("coder"));

        let result = handle_command("help", &[], &handle, &router, &sender).await;
        assert!(result.contains("/agents"));
    }

    #[tokio::test]
    async fn test_handle_command_agent_select() {
        let agent_id = AgentId::new();
        let handle: Arc<dyn ChannelBridgeHandle> = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "coder".to_string())]),
        });
        let router = Arc::new(AgentRouter::new());
        let sender = ChannelUser {
            platform_id: "user1".to_string(),
            display_name: "Test".to_string(),
            openfang_user: None,
        };

        // Select existing agent
        let result =
            handle_command("agent", &["coder".to_string()], &handle, &router, &sender).await;
        assert!(result.contains("Now talking to agent: coder"));

        // Verify router was updated
        let resolved = router.resolve(&ChannelType::Telegram, "user1", None);
        assert_eq!(resolved, Some(agent_id));
    }

    #[tokio::test]
    async fn test_handle_command_agent_without_args_lists_agents() {
        let agent_id = AgentId::new();
        let handle: Arc<dyn ChannelBridgeHandle> = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "coder".to_string())]),
        });
        let router = Arc::new(AgentRouter::new());
        let sender = ChannelUser {
            platform_id: "user1".to_string(),
            display_name: "Test".to_string(),
            openfang_user: None,
        };

        let result = handle_command("agent", &[], &handle, &router, &sender).await;
        assert!(result.contains("Usage: /agent <name>"));
        assert!(result.contains("coder"));
    }

    #[test]
    fn test_channel_command_specs_include_agent_and_help() {
        let specs = channel_command_specs();
        assert!(specs.iter().any(|spec| spec.name == "help"));
        assert!(specs.iter().any(|spec| spec.name == "agent"));
        assert!(specs.iter().any(|spec| spec.name == "agents"));
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = ChannelRateLimiter::default();
        assert!(limiter.check("telegram", "user1", 5).is_ok());
        assert!(limiter.check("telegram", "user1", 5).is_ok());
        assert!(limiter.check("telegram", "user1", 5).is_ok());
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = ChannelRateLimiter::default();
        for _ in 0..3 {
            limiter.check("telegram", "user1", 3).unwrap();
        }
        // 4th should be blocked
        let result = limiter.check("telegram", "user1", 3);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_rate_limiter_zero_means_unlimited() {
        let limiter = ChannelRateLimiter::default();
        for _ in 0..100 {
            assert!(limiter.check("telegram", "user1", 0).is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_separate_users() {
        let limiter = ChannelRateLimiter::default();
        for _ in 0..3 {
            limiter.check("telegram", "user1", 3).unwrap();
        }
        // user1 is blocked
        assert!(limiter.check("telegram", "user1", 3).is_err());
        // user2 should still be ok
        assert!(limiter.check("telegram", "user2", 3).is_ok());
    }

    #[test]
    fn test_dm_policy_filtering() {
        // Test that DmPolicy::Ignore would be checked
        assert_eq!(DmPolicy::default(), DmPolicy::Respond);
        assert_eq!(GroupPolicy::default(), GroupPolicy::MentionOnly);
    }

    #[test]
    fn test_channel_type_str() {
        assert_eq!(channel_type_str(&ChannelType::Telegram), "telegram");
        assert_eq!(channel_type_str(&ChannelType::Matrix), "matrix");
        assert_eq!(channel_type_str(&ChannelType::Email), "email");
        assert_eq!(
            channel_type_str(&ChannelType::Custom("irc".to_string())),
            "irc"
        );
    }

    #[test]
    fn test_default_output_format_for_channel() {
        assert_eq!(
            default_output_format_for_channel("telegram"),
            OutputFormat::TelegramHtml
        );
        assert_eq!(
            default_output_format_for_channel("slack"),
            OutputFormat::SlackMrkdwn
        );
        assert_eq!(
            default_output_format_for_channel("wecom"),
            OutputFormat::PlainText
        );
        assert_eq!(
            default_output_format_for_channel("discord"),
            OutputFormat::Markdown
        );
    }

    #[tokio::test]
    async fn test_send_message_with_blocks_default_fallback() {
        // The default implementation of send_message_with_blocks extracts text
        // from blocks and calls send_message
        let agent_id = AgentId::new();
        let handle: Arc<dyn ChannelBridgeHandle> = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "vision-agent".to_string())]),
        });

        let blocks = vec![
            ContentBlock::Text {
                text: "What is in this photo?".to_string(),
                provider_metadata: None,
            },
            ContentBlock::Image {
                media_type: "image/jpeg".to_string(),
                data: "base64data".to_string(),
            },
        ];

        // Default impl should extract text and call send_message
        let result = handle
            .send_message_with_blocks(agent_id, blocks)
            .await
            .unwrap();
        assert_eq!(result, "Echo: What is in this photo?");
    }

    #[tokio::test]
    async fn test_send_message_with_blocks_image_only() {
        // When there's no text block, the default should still work
        let agent_id = AgentId::new();
        let handle: Arc<dyn ChannelBridgeHandle> = Arc::new(MockHandle {
            agents: Mutex::new(vec![(agent_id, "vision-agent".to_string())]),
        });

        let blocks = vec![ContentBlock::Image {
            media_type: "image/png".to_string(),
            data: "base64data".to_string(),
        }];

        // Default impl sends empty text when no text blocks
        let result = handle
            .send_message_with_blocks(agent_id, blocks)
            .await
            .unwrap();
        assert_eq!(result, "Echo: ");
    }

    #[test]
    fn test_detect_image_magic_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(detect_image_magic(&bytes), Some("image/jpeg".to_string()));
    }

    #[test]
    fn test_detect_image_magic_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_image_magic(&bytes), Some("image/png".to_string()));
    }

    #[test]
    fn test_detect_image_magic_gif() {
        let bytes = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
        assert_eq!(detect_image_magic(&bytes), Some("image/gif".to_string()));
    }

    #[test]
    fn test_detect_image_magic_webp() {
        let bytes = [
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x00, 0x00, 0x00, 0x00, // size (don't care)
            0x57, 0x45, 0x42, 0x50, // WEBP
        ];
        assert_eq!(detect_image_magic(&bytes), Some("image/webp".to_string()));
    }

    #[test]
    fn test_detect_image_magic_unknown() {
        let bytes = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_image_magic(&bytes), None);
    }

    #[test]
    fn test_detect_image_magic_empty() {
        assert_eq!(detect_image_magic(&[]), None);
    }

    #[test]
    fn test_media_type_from_url() {
        assert_eq!(
            media_type_from_url("https://example.com/photo.png"),
            "image/png"
        );
        assert_eq!(
            media_type_from_url("https://example.com/anim.gif"),
            "image/gif"
        );
        assert_eq!(
            media_type_from_url("https://example.com/img.webp"),
            "image/webp"
        );
        assert_eq!(
            media_type_from_url("https://example.com/photo.jpg"),
            "image/jpeg"
        );
        // No extension — defaults to JPEG
        assert_eq!(
            media_type_from_url("https://api.telegram.org/file/bot123/photos/file_42"),
            "image/jpeg"
        );
    }
}
