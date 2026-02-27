//! Channel bridge wiring — connects the OpenFang kernel to channel adapters.
//!
//! Implements `ChannelBridgeHandle` on `OpenFangKernel` and provides the
//! `start_channel_bridge()` entry point called by the daemon.

use openfang_channels::bridge::{BridgeManager, ChannelBridgeHandle};
use openfang_channels::discord::DiscordAdapter;
use openfang_channels::email::EmailAdapter;
use openfang_channels::google_chat::GoogleChatAdapter;
use openfang_channels::irc::IrcAdapter;
use openfang_channels::matrix::MatrixAdapter;
use openfang_channels::mattermost::MattermostAdapter;
use openfang_channels::rocketchat::RocketChatAdapter;
use openfang_channels::router::AgentRouter;
use openfang_channels::signal::SignalAdapter;
use openfang_channels::slack::SlackAdapter;
use openfang_channels::teams::TeamsAdapter;
use openfang_channels::telegram::TelegramAdapter;
use openfang_channels::twitch::TwitchAdapter;
use openfang_channels::types::ChannelAdapter;
use openfang_channels::whatsapp::WhatsAppAdapter;
use openfang_channels::xmpp::XmppAdapter;
use openfang_channels::zulip::ZulipAdapter;
// Wave 3
use openfang_channels::bluesky::BlueskyAdapter;
use openfang_channels::feishu::FeishuAdapter;
use openfang_channels::line::LineAdapter;
use openfang_channels::mastodon::MastodonAdapter;
use openfang_channels::messenger::MessengerAdapter;
use openfang_channels::reddit::RedditAdapter;
use openfang_channels::revolt::RevoltAdapter;
use openfang_channels::viber::ViberAdapter;
// Wave 4
use openfang_channels::flock::FlockAdapter;
use openfang_channels::guilded::GuildedAdapter;
use openfang_channels::keybase::KeybaseAdapter;
use openfang_channels::nextcloud::NextcloudAdapter;
use openfang_channels::nostr::NostrAdapter;
use openfang_channels::pumble::PumbleAdapter;
use openfang_channels::threema::ThreemaAdapter;
use openfang_channels::twist::TwistAdapter;
use openfang_channels::webex::WebexAdapter;
// Wave 5
use async_trait::async_trait;
use openfang_channels::dingtalk::DingTalkAdapter;
use openfang_channels::discourse::DiscourseAdapter;
use openfang_channels::gitter::GitterAdapter;
use openfang_channels::gotify::GotifyAdapter;
use openfang_channels::linkedin::LinkedInAdapter;
use openfang_channels::mumble::MumbleAdapter;
use openfang_channels::ntfy::NtfyAdapter;
use openfang_channels::webhook::WebhookAdapter;
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentId;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Wraps `OpenFangKernel` to implement `ChannelBridgeHandle`.
pub struct KernelBridgeAdapter {
    kernel: Arc<OpenFangKernel>,
    started_at: Instant,
}

#[async_trait]
impl ChannelBridgeHandle for KernelBridgeAdapter {
    async fn send_message(&self, agent_id: AgentId, message: &str) -> Result<String, String> {
        let result = self
            .kernel
            .send_message(agent_id, message)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(result.response)
    }

    async fn find_agent_by_name(&self, name: &str) -> Result<Option<AgentId>, String> {
        Ok(self.kernel.registry.find_by_name(name).map(|e| e.id))
    }

    async fn list_agents(&self) -> Result<Vec<(AgentId, String)>, String> {
        Ok(self
            .kernel
            .registry
            .list()
            .iter()
            .map(|e| (e.id, e.name.clone()))
            .collect())
    }

    async fn spawn_agent_by_name(&self, manifest_name: &str) -> Result<AgentId, String> {
        // Look for manifest at ~/.openfang/agents/{name}/agent.toml
        let manifest_path = self
            .kernel
            .config
            .home_dir
            .join("agents")
            .join(manifest_name)
            .join("agent.toml");

        if !manifest_path.exists() {
            return Err(format!("Manifest not found: {}", manifest_path.display()));
        }

        let contents = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {e}"))?;

        let manifest: openfang_types::agent::AgentManifest =
            toml::from_str(&contents).map_err(|e| format!("Invalid manifest TOML: {e}"))?;

        let agent_id = self
            .kernel
            .spawn_agent(manifest)
            .map_err(|e| format!("Failed to spawn agent: {e}"))?;

        Ok(agent_id)
    }

    async fn uptime_info(&self) -> String {
        let uptime = self.started_at.elapsed();
        let agents = self.list_agents().await.unwrap_or_default();
        let secs = uptime.as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if hours > 0 {
            format!(
                "OpenFang status: {}h {}m uptime, {} agent(s)",
                hours,
                mins,
                agents.len()
            )
        } else {
            format!(
                "OpenFang status: {}m uptime, {} agent(s)",
                mins,
                agents.len()
            )
        }
    }

    async fn list_models_text(&self) -> String {
        let catalog = self
            .kernel
            .model_catalog
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let available = catalog.available_models();
        if available.is_empty() {
            return "No models available. Configure API keys to enable providers.".to_string();
        }
        let mut msg = format!("Available models ({}):\n", available.len());
        // Group by provider
        let mut by_provider: std::collections::HashMap<
            &str,
            Vec<&openfang_types::model_catalog::ModelCatalogEntry>,
        > = std::collections::HashMap::new();
        for m in &available {
            by_provider.entry(m.provider.as_str()).or_default().push(m);
        }
        let mut providers: Vec<&&str> = by_provider.keys().collect();
        providers.sort();
        for provider in providers {
            let provider_name = catalog
                .get_provider(provider)
                .map(|p| p.display_name.as_str())
                .unwrap_or(provider);
            msg.push_str(&format!("\n{}:\n", provider_name));
            for m in &by_provider[provider] {
                let cost = if m.input_cost_per_m > 0.0 {
                    format!(
                        " (${:.2}/${:.2} per M)",
                        m.input_cost_per_m, m.output_cost_per_m
                    )
                } else {
                    " (free/local)".to_string()
                };
                msg.push_str(&format!("  {} — {}{}\n", m.id, m.display_name, cost));
            }
        }
        msg
    }

    async fn list_providers_text(&self) -> String {
        let catalog = self
            .kernel
            .model_catalog
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut msg = "Providers:\n".to_string();
        for p in catalog.list_providers() {
            let status = match p.auth_status {
                openfang_types::model_catalog::AuthStatus::Configured => "configured",
                openfang_types::model_catalog::AuthStatus::Missing => "not configured",
                openfang_types::model_catalog::AuthStatus::NotRequired => "local (no key needed)",
            };
            msg.push_str(&format!(
                "  {} — {} [{}, {} model(s)]\n",
                p.id, p.display_name, status, p.model_count
            ));
        }
        msg
    }

    async fn list_skills_text(&self) -> String {
        let skills = self
            .kernel
            .skill_registry
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let skills = skills.list();
        if skills.is_empty() {
            return "No skills installed. Place skills in ~/.openfang/skills/ or install from the marketplace.".to_string();
        }
        let mut msg = format!("Installed skills ({}):\n", skills.len());
        for skill in &skills {
            let runtime = format!("{:?}", skill.manifest.runtime.runtime_type);
            let tools_count = skill.manifest.tools.provided.len();
            let enabled = if skill.enabled { "" } else { " [disabled]" };
            msg.push_str(&format!(
                "  {} — {} ({}, {} tool(s)){}\n",
                skill.manifest.skill.name,
                skill.manifest.skill.description,
                runtime,
                tools_count,
                enabled,
            ));
        }
        msg
    }

    async fn list_hands_text(&self) -> String {
        let defs = self.kernel.hand_registry.list_definitions();
        if defs.is_empty() {
            return "No hands available.".to_string();
        }
        let instances = self.kernel.hand_registry.list_instances();
        let mut msg = format!("Available hands ({}):\n", defs.len());
        for d in &defs {
            let reqs_met = self
                .kernel
                .hand_registry
                .check_requirements(&d.id)
                .map(|r| r.iter().all(|(_, ok)| *ok))
                .unwrap_or(false);
            let badge = if reqs_met { "Ready" } else { "Setup needed" };
            msg.push_str(&format!(
                "  {} {} — {} [{}]\n",
                d.icon, d.name, d.description, badge
            ));
        }
        if !instances.is_empty() {
            msg.push_str(&format!("\nActive ({}):\n", instances.len()));
            for i in &instances {
                msg.push_str(&format!(
                    "  {} — {} ({})\n",
                    i.agent_name, i.hand_id, i.status
                ));
            }
        }
        msg
    }

    // ── Automation: workflows, triggers, schedules, approvals ──

    async fn list_workflows_text(&self) -> String {
        let workflows = self.kernel.workflows.list_workflows().await;
        if workflows.is_empty() {
            return "No workflows defined.".to_string();
        }
        let mut msg = format!("Workflows ({}):\n", workflows.len());
        for wf in &workflows {
            let steps = wf.steps.len();
            let desc = if wf.description.is_empty() {
                String::new()
            } else {
                format!(" — {}", wf.description)
            };
            msg.push_str(&format!("  {} ({} step(s)){}\n", wf.name, steps, desc));
        }
        msg
    }

    async fn run_workflow_text(&self, name: &str, input: &str) -> String {
        let workflows = self.kernel.workflows.list_workflows().await;
        let wf = match workflows.iter().find(|w| w.name.eq_ignore_ascii_case(name)) {
            Some(w) => w.clone(),
            None => return format!("Workflow '{name}' not found. Use /workflows to list."),
        };

        let run_id = match self
            .kernel
            .workflows
            .create_run(wf.id, input.to_string())
            .await
        {
            Some(id) => id,
            None => return "Failed to create workflow run.".to_string(),
        };

        let kernel = self.kernel.clone();
        let registry_ref = &self.kernel.registry;
        let result = self
            .kernel
            .workflows
            .execute_run(
                run_id,
                |step_agent| match step_agent {
                    openfang_kernel::workflow::StepAgent::ById { id } => {
                        let aid: AgentId = id.parse().ok()?;
                        let entry = registry_ref.get(aid)?;
                        Some((aid, entry.name.clone()))
                    }
                    openfang_kernel::workflow::StepAgent::ByName { name } => {
                        let entry = registry_ref.find_by_name(name)?;
                        Some((entry.id, entry.name.clone()))
                    }
                },
                |agent_id, message| {
                    let k = kernel.clone();
                    async move {
                        let result = k
                            .send_message(agent_id, &message)
                            .await
                            .map_err(|e| format!("{e}"))?;
                        Ok((
                            result.response,
                            result.total_usage.input_tokens,
                            result.total_usage.output_tokens,
                        ))
                    }
                },
            )
            .await;

        match result {
            Ok(output) => format!("Workflow '{}' completed:\n{}", wf.name, output),
            Err(e) => format!("Workflow '{}' failed: {}", wf.name, e),
        }
    }

    async fn list_triggers_text(&self) -> String {
        let triggers = self.kernel.triggers.list_all();
        if triggers.is_empty() {
            return "No triggers configured.".to_string();
        }
        let mut msg = format!("Triggers ({}):\n", triggers.len());
        for t in &triggers {
            let agent_name = self
                .kernel
                .registry
                .get(t.agent_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| t.agent_id.to_string());
            let status = if t.enabled { "on" } else { "off" };
            let id_short = &t.id.0.to_string()[..8];
            msg.push_str(&format!(
                "  [{}] {} -> {} ({:?}) fires:{} [{}]\n",
                id_short,
                agent_name,
                t.prompt_template.chars().take(40).collect::<String>(),
                t.pattern,
                t.fire_count,
                status,
            ));
        }
        msg
    }

    async fn create_trigger_text(
        &self,
        agent_name: &str,
        pattern_str: &str,
        prompt: &str,
    ) -> String {
        let agent = match self.kernel.registry.find_by_name(agent_name) {
            Some(e) => e,
            None => return format!("Agent '{agent_name}' not found."),
        };

        let pattern = match parse_trigger_pattern(pattern_str) {
            Some(p) => p,
            None => {
                return format!(
                "Unknown pattern '{pattern_str}'. Valid: lifecycle, spawned:<name>, terminated, \
                 system, system:<keyword>, memory, memory:<key>, match:<text>, all"
            )
            }
        };

        let trigger_id = self
            .kernel
            .triggers
            .register(agent.id, pattern, prompt.to_string(), 0);
        let id_short = &trigger_id.0.to_string()[..8];
        format!("Trigger created [{id_short}] for agent '{agent_name}'.")
    }

    async fn delete_trigger_text(&self, id_prefix: &str) -> String {
        let triggers = self.kernel.triggers.list_all();
        let matched: Vec<_> = triggers
            .iter()
            .filter(|t| t.id.0.to_string().starts_with(id_prefix))
            .collect();
        match matched.len() {
            0 => format!("No trigger found matching '{id_prefix}'."),
            1 => {
                let t = matched[0];
                if self.kernel.triggers.remove(t.id) {
                    format!("Trigger [{}] removed.", &t.id.0.to_string()[..8])
                } else {
                    "Failed to remove trigger.".to_string()
                }
            }
            n => format!("{n} triggers match '{id_prefix}'. Be more specific."),
        }
    }

    async fn list_schedules_text(&self) -> String {
        let jobs = self.kernel.cron_scheduler.list_all_jobs();
        if jobs.is_empty() {
            return "No scheduled jobs.".to_string();
        }
        let mut msg = format!("Cron jobs ({}):\n", jobs.len());
        for job in &jobs {
            let agent_name = self
                .kernel
                .registry
                .get(job.agent_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| job.agent_id.to_string());
            let status = if job.enabled { "on" } else { "off" };
            let id_short = &job.id.0.to_string()[..8];
            let sched = match &job.schedule {
                openfang_types::scheduler::CronSchedule::Cron { expr, .. } => expr.clone(),
                openfang_types::scheduler::CronSchedule::Every { every_secs } => {
                    format!("every {every_secs}s")
                }
                openfang_types::scheduler::CronSchedule::At { at } => {
                    format!("at {}", at.format("%Y-%m-%d %H:%M"))
                }
            };
            let last = job
                .last_run
                .map(|t| t.format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "never".to_string());
            msg.push_str(&format!(
                "  [{}] {} — {} ({}) last:{} [{}]\n",
                id_short, job.name, sched, agent_name, last, status,
            ));
        }
        msg
    }

    async fn manage_schedule_text(&self, action: &str, args: &[String]) -> String {
        match action {
            "add" => {
                // Expected: <agent> <f1> <f2> <f3> <f4> <f5> <message...>
                // 5 cron fields: min hour dom month dow
                if args.len() < 7 {
                    return "Usage: /schedule add <agent> <min> <hour> <dom> <month> <dow> <message>".to_string();
                }
                let agent_name = &args[0];
                let agent = match self.kernel.registry.find_by_name(agent_name) {
                    Some(e) => e,
                    None => return format!("Agent '{agent_name}' not found."),
                };
                let cron_expr = args[1..6].join(" ");
                let message = args[6..].join(" ");

                let job = openfang_types::scheduler::CronJob {
                    id: openfang_types::scheduler::CronJobId::new(),
                    agent_id: agent.id,
                    name: format!("chat-{}", &agent.name),
                    enabled: true,
                    schedule: openfang_types::scheduler::CronSchedule::Cron {
                        expr: cron_expr.clone(),
                        tz: None,
                    },
                    action: openfang_types::scheduler::CronAction::AgentTurn {
                        message: message.clone(),
                        model_override: None,
                        timeout_secs: None,
                    },
                    delivery: openfang_types::scheduler::CronDelivery::None,
                    created_at: chrono::Utc::now(),
                    last_run: None,
                    next_run: None,
                };

                match self.kernel.cron_scheduler.add_job(job, false) {
                    Ok(id) => {
                        let id_short = &id.0.to_string()[..8];
                        format!("Job [{id_short}] created: '{cron_expr}' -> {agent_name}: \"{message}\"")
                    }
                    Err(e) => format!("Failed to create job: {e}"),
                }
            }
            "del" => {
                if args.is_empty() {
                    return "Usage: /schedule del <id-prefix>".to_string();
                }
                let prefix = &args[0];
                let jobs = self.kernel.cron_scheduler.list_all_jobs();
                let matched: Vec<_> = jobs
                    .iter()
                    .filter(|j| j.id.0.to_string().starts_with(prefix.as_str()))
                    .collect();
                match matched.len() {
                    0 => format!("No job found matching '{prefix}'."),
                    1 => {
                        let j = matched[0];
                        match self.kernel.cron_scheduler.remove_job(j.id) {
                            Ok(_) => {
                                format!("Job [{}] '{}' removed.", &j.id.0.to_string()[..8], j.name)
                            }
                            Err(e) => format!("Failed to remove job: {e}"),
                        }
                    }
                    n => format!("{n} jobs match '{prefix}'. Be more specific."),
                }
            }
            "run" => {
                if args.is_empty() {
                    return "Usage: /schedule run <id-prefix>".to_string();
                }
                let prefix = &args[0];
                let jobs = self.kernel.cron_scheduler.list_all_jobs();
                let matched: Vec<_> = jobs
                    .iter()
                    .filter(|j| j.id.0.to_string().starts_with(prefix.as_str()))
                    .collect();
                match matched.len() {
                    0 => format!("No job found matching '{prefix}'."),
                    1 => {
                        let j = matched[0];
                        let message = match &j.action {
                            openfang_types::scheduler::CronAction::AgentTurn {
                                message, ..
                            } => message.clone(),
                            openfang_types::scheduler::CronAction::SystemEvent { text } => {
                                text.clone()
                            }
                        };
                        match self.kernel.send_message(j.agent_id, &message).await {
                            Ok(result) => {
                                let id_short = &j.id.0.to_string()[..8];
                                format!("Job [{id_short}] ran:\n{}", result.response)
                            }
                            Err(e) => format!("Failed to run job: {e}"),
                        }
                    }
                    n => format!("{n} jobs match '{prefix}'. Be more specific."),
                }
            }
            _ => "Unknown schedule action. Use: add, del, run".to_string(),
        }
    }

    async fn list_approvals_text(&self) -> String {
        let pending = self.kernel.approval_manager.list_pending();
        if pending.is_empty() {
            return "No pending approvals.".to_string();
        }
        let mut msg = format!("Pending approvals ({}):\n", pending.len());
        for req in &pending {
            let id_short = &req.id.to_string()[..8];
            let age_secs = (chrono::Utc::now() - req.requested_at).num_seconds();
            let age = if age_secs >= 60 {
                format!("{}m", age_secs / 60)
            } else {
                format!("{age_secs}s")
            };
            msg.push_str(&format!(
                "  [{}] {} — {} ({:?}) age:{}\n",
                id_short, req.agent_id, req.tool_name, req.risk_level, age,
            ));
            if !req.action_summary.is_empty() {
                msg.push_str(&format!("    {}\n", req.action_summary));
            }
        }
        msg.push_str("\nUse /approve <id> or /reject <id>");
        msg
    }

    async fn resolve_approval_text(&self, id_prefix: &str, approve: bool) -> String {
        let pending = self.kernel.approval_manager.list_pending();
        let matched: Vec<_> = pending
            .iter()
            .filter(|r| r.id.to_string().starts_with(id_prefix))
            .collect();
        match matched.len() {
            0 => format!("No pending approval matching '{id_prefix}'."),
            1 => {
                let req = matched[0];
                let decision = if approve {
                    openfang_types::approval::ApprovalDecision::Approved
                } else {
                    openfang_types::approval::ApprovalDecision::Denied
                };
                match self.kernel.approval_manager.resolve(
                    req.id,
                    decision,
                    Some("channel".to_string()),
                ) {
                    Ok(_) => {
                        let verb = if approve { "Approved" } else { "Rejected" };
                        format!(
                            "{} [{}] {} — {}",
                            verb,
                            &req.id.to_string()[..8],
                            req.tool_name,
                            req.agent_id
                        )
                    }
                    Err(e) => format!("Failed to resolve approval: {e}"),
                }
            }
            n => format!("{n} approvals match '{id_prefix}'. Be more specific."),
        }
    }

    async fn reset_session(&self, agent_id: AgentId) -> Result<String, String> {
        self.kernel
            .reset_session(agent_id)
            .map_err(|e| format!("{e}"))?;
        Ok("Session reset. Chat history cleared.".to_string())
    }

    async fn compact_session(&self, agent_id: AgentId) -> Result<String, String> {
        self.kernel
            .compact_agent_session(agent_id)
            .await
            .map_err(|e| format!("{e}"))
    }

    async fn set_model(&self, agent_id: AgentId, model: &str) -> Result<String, String> {
        if model.is_empty() {
            // Show current model
            let entry = self
                .kernel
                .registry
                .get(agent_id)
                .ok_or_else(|| "Agent not found".to_string())?;
            return Ok(format!(
                "Current model: {} (provider: {})",
                entry.manifest.model.model, entry.manifest.model.provider
            ));
        }
        self.kernel
            .set_agent_model(agent_id, model)
            .map_err(|e| format!("{e}"))?;
        Ok(format!("Model switched to: {model}"))
    }

    async fn stop_run(&self, agent_id: AgentId) -> Result<String, String> {
        let cancelled = self
            .kernel
            .stop_agent_run(agent_id)
            .map_err(|e| format!("{e}"))?;
        if cancelled {
            Ok("Run cancelled.".to_string())
        } else {
            Ok("No active run to cancel.".to_string())
        }
    }

    async fn session_usage(&self, agent_id: AgentId) -> Result<String, String> {
        let (input, output, cost) = self
            .kernel
            .session_usage_cost(agent_id)
            .map_err(|e| format!("{e}"))?;
        let total = input + output;
        let mut msg = format!("Session usage:\n  Input: ~{input} tokens\n  Output: ~{output} tokens\n  Total: ~{total} tokens");
        if cost > 0.0 {
            msg.push_str(&format!("\n  Estimated cost: ${cost:.4}"));
        }
        Ok(msg)
    }

    async fn set_thinking(&self, _agent_id: AgentId, on: bool) -> Result<String, String> {
        // Future-ready: stores preference but doesn't affect model behavior yet
        let state = if on { "enabled" } else { "disabled" };
        Ok(format!(
            "Extended thinking {state}. (This will take effect when supported by the model.)"
        ))
    }

    async fn channel_overrides(
        &self,
        channel_type: &str,
    ) -> Option<openfang_types::config::ChannelOverrides> {
        let channels = &self.kernel.config.channels;
        match channel_type {
            "telegram" => channels.telegram.as_ref().map(|c| c.overrides.clone()),
            "discord" => channels.discord.as_ref().map(|c| c.overrides.clone()),
            "slack" => channels.slack.as_ref().map(|c| c.overrides.clone()),
            "whatsapp" => channels.whatsapp.as_ref().map(|c| c.overrides.clone()),
            "signal" => channels.signal.as_ref().map(|c| c.overrides.clone()),
            "matrix" => channels.matrix.as_ref().map(|c| c.overrides.clone()),
            "email" => channels.email.as_ref().map(|c| c.overrides.clone()),
            "teams" => channels.teams.as_ref().map(|c| c.overrides.clone()),
            "mattermost" => channels.mattermost.as_ref().map(|c| c.overrides.clone()),
            "irc" => channels.irc.as_ref().map(|c| c.overrides.clone()),
            "google_chat" => channels.google_chat.as_ref().map(|c| c.overrides.clone()),
            "twitch" => channels.twitch.as_ref().map(|c| c.overrides.clone()),
            "rocketchat" => channels.rocketchat.as_ref().map(|c| c.overrides.clone()),
            "zulip" => channels.zulip.as_ref().map(|c| c.overrides.clone()),
            "xmpp" => channels.xmpp.as_ref().map(|c| c.overrides.clone()),
            // Wave 3
            "line" => channels.line.as_ref().map(|c| c.overrides.clone()),
            "viber" => channels.viber.as_ref().map(|c| c.overrides.clone()),
            "messenger" => channels.messenger.as_ref().map(|c| c.overrides.clone()),
            "reddit" => channels.reddit.as_ref().map(|c| c.overrides.clone()),
            "mastodon" => channels.mastodon.as_ref().map(|c| c.overrides.clone()),
            "bluesky" => channels.bluesky.as_ref().map(|c| c.overrides.clone()),
            "feishu" => channels.feishu.as_ref().map(|c| c.overrides.clone()),
            "revolt" => channels.revolt.as_ref().map(|c| c.overrides.clone()),
            // Wave 4
            "nextcloud" => channels.nextcloud.as_ref().map(|c| c.overrides.clone()),
            "guilded" => channels.guilded.as_ref().map(|c| c.overrides.clone()),
            "keybase" => channels.keybase.as_ref().map(|c| c.overrides.clone()),
            "threema" => channels.threema.as_ref().map(|c| c.overrides.clone()),
            "nostr" => channels.nostr.as_ref().map(|c| c.overrides.clone()),
            "webex" => channels.webex.as_ref().map(|c| c.overrides.clone()),
            "pumble" => channels.pumble.as_ref().map(|c| c.overrides.clone()),
            "flock" => channels.flock.as_ref().map(|c| c.overrides.clone()),
            "twist" => channels.twist.as_ref().map(|c| c.overrides.clone()),
            // Wave 5
            "mumble" => channels.mumble.as_ref().map(|c| c.overrides.clone()),
            "dingtalk" => channels.dingtalk.as_ref().map(|c| c.overrides.clone()),
            "discourse" => channels.discourse.as_ref().map(|c| c.overrides.clone()),
            "gitter" => channels.gitter.as_ref().map(|c| c.overrides.clone()),
            "ntfy" => channels.ntfy.as_ref().map(|c| c.overrides.clone()),
            "gotify" => channels.gotify.as_ref().map(|c| c.overrides.clone()),
            "webhook" => channels.webhook.as_ref().map(|c| c.overrides.clone()),
            "linkedin" => channels.linkedin.as_ref().map(|c| c.overrides.clone()),
            _ => None,
        }
    }

    async fn authorize_channel_user(
        &self,
        channel_type: &str,
        platform_id: &str,
        action: &str,
    ) -> Result<(), String> {
        if !self.kernel.auth.is_enabled() {
            return Ok(()); // RBAC not configured — allow all
        }

        let user_id = self
            .kernel
            .auth
            .identify(channel_type, platform_id)
            .ok_or_else(|| "Unrecognized user. Contact an admin to get access.".to_string())?;

        let auth_action = match action {
            "chat" => openfang_kernel::auth::Action::ChatWithAgent,
            "spawn" => openfang_kernel::auth::Action::SpawnAgent,
            "kill" => openfang_kernel::auth::Action::KillAgent,
            "install_skill" => openfang_kernel::auth::Action::InstallSkill,
            _ => openfang_kernel::auth::Action::ChatWithAgent,
        };

        self.kernel
            .auth
            .authorize(user_id, &auth_action)
            .map_err(|e| e.to_string())
    }

    async fn record_delivery(
        &self,
        agent_id: AgentId,
        channel: &str,
        recipient: &str,
        success: bool,
        error: Option<&str>,
    ) {
        let receipt = if success {
            openfang_kernel::DeliveryTracker::sent_receipt(channel, recipient)
        } else {
            openfang_kernel::DeliveryTracker::failed_receipt(
                channel,
                recipient,
                error.unwrap_or("Unknown error"),
            )
        };
        self.kernel.delivery_tracker.record(agent_id, receipt);

        // Persist last channel for cron CronDelivery::LastChannel
        if success {
            let kv_val = serde_json::json!({"channel": channel, "recipient": recipient});
            let _ = self
                .kernel
                .memory
                .structured_set(agent_id, "delivery.last_channel", kv_val);
        }
    }

    async fn check_auto_reply(&self, agent_id: AgentId, message: &str) -> Option<String> {
        // Check if auto-reply should fire for this message
        let channel_type = "bridge"; // Generic; the bridge layer handles specifics
        self.kernel
            .auto_reply_engine
            .should_reply(message, channel_type, agent_id)?;
        // Fire auto-reply synchronously (bridge already runs in background task)
        match self.kernel.send_message(agent_id, message).await {
            Ok(result) => Some(result.response),
            Err(e) => {
                tracing::warn!(error = %e, "Auto-reply failed");
                None
            }
        }
    }

    // ── Budget, Network, A2A ──

    async fn budget_text(&self) -> String {
        let budget = &self.kernel.config.budget;
        let status = self.kernel.metering.budget_status(budget);

        let fmt_limit = |v: f64| -> String {
            if v > 0.0 {
                format!("${v:.2}")
            } else {
                "unlimited".to_string()
            }
        };
        let fmt_pct = |pct: f64, limit: f64| -> String {
            if limit > 0.0 {
                format!(" ({:.1}%)", pct * 100.0)
            } else {
                String::new()
            }
        };

        format!(
            "Budget Status:\n\
             \n\
             Hourly:  ${:.4} / {}{}\n\
             Daily:   ${:.4} / {}{}\n\
             Monthly: ${:.4} / {}{}\n\
             \n\
             Alert threshold: {}%",
            status.hourly_spend,
            fmt_limit(status.hourly_limit),
            fmt_pct(status.hourly_pct, status.hourly_limit),
            status.daily_spend,
            fmt_limit(status.daily_limit),
            fmt_pct(status.daily_pct, status.daily_limit),
            status.monthly_spend,
            fmt_limit(status.monthly_limit),
            fmt_pct(status.monthly_pct, status.monthly_limit),
            (status.alert_threshold * 100.0) as u32,
        )
    }

    async fn peers_text(&self) -> String {
        if !self.kernel.config.network_enabled {
            return "OFP peer network is disabled. Set network_enabled = true in config.toml."
                .to_string();
        }
        match &self.kernel.peer_registry {
            Some(registry) => {
                let peers = registry.all_peers();
                if peers.is_empty() {
                    "OFP network enabled but no peers connected.".to_string()
                } else {
                    let mut msg = format!("OFP Peers ({} connected):\n", peers.len());
                    for p in &peers {
                        msg.push_str(&format!(
                            "  {} — {} ({:?})\n",
                            p.node_id, p.address, p.state
                        ));
                    }
                    msg
                }
            }
            None => "OFP peer node not started.".to_string(),
        }
    }

    async fn a2a_agents_text(&self) -> String {
        let agents = self
            .kernel
            .a2a_external_agents
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if agents.is_empty() {
            return "No external A2A agents discovered.\nUse the dashboard or API to discover agents.".to_string();
        }
        let mut msg = format!("External A2A Agents ({}):\n", agents.len());
        for (url, card) in agents.iter() {
            msg.push_str(&format!("  {} — {}\n", card.name, url));
            let desc = &card.description;
            if !desc.is_empty() {
                let short = if desc.len() > 60 {
                    &desc[..60]
                } else {
                    desc.as_str()
                };
                msg.push_str(&format!("    {short}\n"));
            }
        }
        msg
    }
}

/// Parse a trigger pattern string from chat into a `TriggerPattern`.
fn parse_trigger_pattern(s: &str) -> Option<openfang_kernel::triggers::TriggerPattern> {
    use openfang_kernel::triggers::TriggerPattern;
    if let Some(rest) = s.strip_prefix("spawned:") {
        return Some(TriggerPattern::AgentSpawned {
            name_pattern: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("system:") {
        return Some(TriggerPattern::SystemKeyword {
            keyword: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("memory:") {
        return Some(TriggerPattern::MemoryKeyPattern {
            key_pattern: rest.to_string(),
        });
    }
    if let Some(rest) = s.strip_prefix("match:") {
        return Some(TriggerPattern::ContentMatch {
            substring: rest.to_string(),
        });
    }
    match s {
        "lifecycle" => Some(TriggerPattern::Lifecycle),
        "terminated" => Some(TriggerPattern::AgentTerminated),
        "system" => Some(TriggerPattern::System),
        "memory" => Some(TriggerPattern::MemoryUpdate),
        "all" => Some(TriggerPattern::All),
        _ => None,
    }
}

/// Resolve a default agent by name — find running or spawn from manifest.
async fn resolve_default_agent(
    handle: &KernelBridgeAdapter,
    name: &str,
    router: &mut AgentRouter,
    adapter_name: &str,
) {
    match handle.find_agent_by_name(name).await {
        Ok(Some(agent_id)) => {
            router.set_default(agent_id);
            info!("{adapter_name} default agent: {name} ({agent_id})");
        }
        _ => match handle.spawn_agent_by_name(name).await {
            Ok(agent_id) => {
                router.set_default(agent_id);
                info!("{adapter_name}: spawned default agent {name} ({agent_id})");
            }
            Err(e) => {
                warn!("{adapter_name}: could not find or spawn default agent '{name}': {e}");
            }
        },
    }
}

/// Read a token from an env var, returning None with a warning if missing/empty.
fn read_token(env_var: &str, adapter_name: &str) -> Option<String> {
    match std::env::var(env_var) {
        Ok(t) if !t.is_empty() => Some(t),
        Ok(_) => {
            warn!("{adapter_name} bot token env var '{env_var}' is empty, skipping");
            None
        }
        Err(_) => {
            warn!("{adapter_name} bot token env var '{env_var}' not set, skipping");
            None
        }
    }
}

/// Start the channel bridge for all configured channels based on kernel config.
///
/// Returns `Some(BridgeManager)` if any channels were configured and started,
/// or `None` if no channels are configured.
pub async fn start_channel_bridge(kernel: Arc<OpenFangKernel>) -> Option<BridgeManager> {
    let channels = kernel.config.channels.clone();
    let (bridge, _names) = start_channel_bridge_with_config(kernel, &channels).await;
    bridge
}

/// Start channels from an explicit `ChannelsConfig` (used by hot-reload).
///
/// Returns `(Option<BridgeManager>, Vec<started_channel_names>)`.
pub async fn start_channel_bridge_with_config(
    kernel: Arc<OpenFangKernel>,
    config: &openfang_types::config::ChannelsConfig,
) -> (Option<BridgeManager>, Vec<String>) {
    let has_any = config.telegram.is_some()
        || config.discord.is_some()
        || config.slack.is_some()
        || config.whatsapp.is_some()
        || config.signal.is_some()
        || config.matrix.is_some()
        || config.email.is_some()
        || config.teams.is_some()
        || config.mattermost.is_some()
        || config.irc.is_some()
        || config.google_chat.is_some()
        || config.twitch.is_some()
        || config.rocketchat.is_some()
        || config.zulip.is_some()
        || config.xmpp.is_some()
        // Wave 3
        || config.line.is_some()
        || config.viber.is_some()
        || config.messenger.is_some()
        || config.reddit.is_some()
        || config.mastodon.is_some()
        || config.bluesky.is_some()
        || config.feishu.is_some()
        || config.revolt.is_some()
        // Wave 4
        || config.nextcloud.is_some()
        || config.guilded.is_some()
        || config.keybase.is_some()
        || config.threema.is_some()
        || config.nostr.is_some()
        || config.webex.is_some()
        || config.pumble.is_some()
        || config.flock.is_some()
        || config.twist.is_some()
        // Wave 5
        || config.mumble.is_some()
        || config.dingtalk.is_some()
        || config.discourse.is_some()
        || config.gitter.is_some()
        || config.ntfy.is_some()
        || config.gotify.is_some()
        || config.webhook.is_some()
        || config.linkedin.is_some();

    if !has_any {
        return (None, Vec::new());
    }

    let handle = KernelBridgeAdapter {
        kernel: kernel.clone(),
        started_at: Instant::now(),
    };

    // Collect all adapters to start
    let mut adapters: Vec<(Arc<dyn ChannelAdapter>, Option<String>)> = Vec::new();

    // Telegram
    if let Some(ref tg_config) = config.telegram {
        if let Some(token) = read_token(&tg_config.bot_token_env, "Telegram") {
            let poll_interval = Duration::from_secs(tg_config.poll_interval_secs);
            let adapter = Arc::new(TelegramAdapter::new(
                token,
                tg_config.allowed_users.clone(),
                poll_interval,
            ));
            adapters.push((adapter, tg_config.default_agent.clone()));
        }
    }

    // Discord
    if let Some(ref dc_config) = config.discord {
        if let Some(token) = read_token(&dc_config.bot_token_env, "Discord") {
            let adapter = Arc::new(DiscordAdapter::new(
                token,
                dc_config.allowed_guilds.clone(),
                dc_config.intents,
            ));
            adapters.push((adapter, dc_config.default_agent.clone()));
        }
    }

    // Slack
    if let Some(ref sl_config) = config.slack {
        if let Some(app_token) = read_token(&sl_config.app_token_env, "Slack (app)") {
            if let Some(bot_token) = read_token(&sl_config.bot_token_env, "Slack (bot)") {
                let adapter = Arc::new(SlackAdapter::new(
                    app_token,
                    bot_token,
                    sl_config.allowed_channels.clone(),
                ));
                adapters.push((adapter, sl_config.default_agent.clone()));
            }
        }
    }

    // WhatsApp — supports Cloud API mode (access token) or Web/QR mode (gateway URL)
    if let Some(ref wa_config) = config.whatsapp {
        let cloud_token = read_token(&wa_config.access_token_env, "WhatsApp");
        let gateway_url = std::env::var(&wa_config.gateway_url_env)
            .ok()
            .filter(|u| !u.is_empty());

        if cloud_token.is_some() || gateway_url.is_some() {
            let token = cloud_token.unwrap_or_default();
            let verify_token =
                read_token(&wa_config.verify_token_env, "WhatsApp (verify)").unwrap_or_default();
            let adapter = Arc::new(
                WhatsAppAdapter::new(
                    wa_config.phone_number_id.clone(),
                    token,
                    verify_token,
                    wa_config.webhook_port,
                    wa_config.allowed_users.clone(),
                )
                .with_gateway(gateway_url),
            );
            adapters.push((adapter, wa_config.default_agent.clone()));
        }
    }

    // Signal
    if let Some(ref sig_config) = config.signal {
        if !sig_config.phone_number.is_empty() {
            let adapter = Arc::new(SignalAdapter::new(
                sig_config.api_url.clone(),
                sig_config.phone_number.clone(),
                sig_config.allowed_users.clone(),
            ));
            adapters.push((adapter, sig_config.default_agent.clone()));
        } else {
            warn!("Signal configured but phone_number is empty, skipping");
        }
    }

    // Matrix
    if let Some(ref mx_config) = config.matrix {
        if let Some(token) = read_token(&mx_config.access_token_env, "Matrix") {
            let adapter = Arc::new(MatrixAdapter::new(
                mx_config.homeserver_url.clone(),
                mx_config.user_id.clone(),
                token,
                mx_config.allowed_rooms.clone(),
            ));
            adapters.push((adapter, mx_config.default_agent.clone()));
        }
    }

    // Email
    if let Some(ref em_config) = config.email {
        if let Some(password) = read_token(&em_config.password_env, "Email") {
            let adapter = Arc::new(EmailAdapter::new(
                em_config.imap_host.clone(),
                em_config.imap_port,
                em_config.smtp_host.clone(),
                em_config.smtp_port,
                em_config.username.clone(),
                password,
                em_config.poll_interval_secs,
                em_config.folders.clone(),
                em_config.allowed_senders.clone(),
            ));
            adapters.push((adapter, em_config.default_agent.clone()));
        }
    }

    // Teams
    if let Some(ref tm_config) = config.teams {
        if let Some(password) = read_token(&tm_config.app_password_env, "Teams") {
            let adapter = Arc::new(TeamsAdapter::new(
                tm_config.app_id.clone(),
                password,
                tm_config.webhook_port,
                tm_config.allowed_tenants.clone(),
            ));
            adapters.push((adapter, tm_config.default_agent.clone()));
        }
    }

    // Mattermost
    if let Some(ref mm_config) = config.mattermost {
        if let Some(token) = read_token(&mm_config.token_env, "Mattermost") {
            let adapter = Arc::new(MattermostAdapter::new(
                mm_config.server_url.clone(),
                token,
                mm_config.allowed_channels.clone(),
            ));
            adapters.push((adapter, mm_config.default_agent.clone()));
        }
    }

    // IRC
    if let Some(ref irc_config) = config.irc {
        if !irc_config.server.is_empty() {
            let password = irc_config
                .password_env
                .as_ref()
                .and_then(|env| read_token(env, "IRC"));
            let adapter = Arc::new(IrcAdapter::new(
                irc_config.server.clone(),
                irc_config.port,
                irc_config.nick.clone(),
                password,
                irc_config.channels.clone(),
                irc_config.use_tls,
            ));
            adapters.push((adapter, irc_config.default_agent.clone()));
        } else {
            warn!("IRC configured but server is empty, skipping");
        }
    }

    // Google Chat
    if let Some(ref gc_config) = config.google_chat {
        if let Some(key) = read_token(&gc_config.service_account_env, "Google Chat") {
            let adapter = Arc::new(GoogleChatAdapter::new(
                key,
                gc_config.space_ids.clone(),
                gc_config.webhook_port,
            ));
            adapters.push((adapter, gc_config.default_agent.clone()));
        }
    }

    // Twitch
    if let Some(ref tw_config) = config.twitch {
        if let Some(token) = read_token(&tw_config.oauth_token_env, "Twitch") {
            let adapter = Arc::new(TwitchAdapter::new(
                token,
                tw_config.channels.clone(),
                tw_config.nick.clone(),
            ));
            adapters.push((adapter, tw_config.default_agent.clone()));
        }
    }

    // Rocket.Chat
    if let Some(ref rc_config) = config.rocketchat {
        if let Some(token) = read_token(&rc_config.token_env, "Rocket.Chat") {
            let adapter = Arc::new(RocketChatAdapter::new(
                rc_config.server_url.clone(),
                token,
                rc_config.user_id.clone(),
                rc_config.allowed_channels.clone(),
            ));
            adapters.push((adapter, rc_config.default_agent.clone()));
        }
    }

    // Zulip
    if let Some(ref z_config) = config.zulip {
        if let Some(api_key) = read_token(&z_config.api_key_env, "Zulip") {
            let adapter = Arc::new(ZulipAdapter::new(
                z_config.server_url.clone(),
                z_config.bot_email.clone(),
                api_key,
                z_config.streams.clone(),
            ));
            adapters.push((adapter, z_config.default_agent.clone()));
        }
    }

    // XMPP
    if let Some(ref x_config) = config.xmpp {
        if let Some(password) = read_token(&x_config.password_env, "XMPP") {
            let adapter = Arc::new(XmppAdapter::new(
                x_config.jid.clone(),
                password,
                x_config.server.clone(),
                x_config.port,
                x_config.rooms.clone(),
            ));
            adapters.push((adapter, x_config.default_agent.clone()));
        }
    }

    // ── Wave 3 ──────────────────────────────────────────────────

    // LINE
    if let Some(ref ln_config) = config.line {
        if let Some(secret) = read_token(&ln_config.channel_secret_env, "LINE (secret)") {
            if let Some(token) = read_token(&ln_config.access_token_env, "LINE (token)") {
                let adapter = Arc::new(LineAdapter::new(secret, token, ln_config.webhook_port));
                adapters.push((adapter, ln_config.default_agent.clone()));
            }
        }
    }

    // Viber
    if let Some(ref vb_config) = config.viber {
        if let Some(token) = read_token(&vb_config.auth_token_env, "Viber") {
            let adapter = Arc::new(ViberAdapter::new(
                token,
                vb_config.webhook_url.clone(),
                vb_config.webhook_port,
            ));
            adapters.push((adapter, vb_config.default_agent.clone()));
        }
    }

    // Facebook Messenger
    if let Some(ref ms_config) = config.messenger {
        if let Some(page_token) = read_token(&ms_config.page_token_env, "Messenger (page)") {
            let verify_token =
                read_token(&ms_config.verify_token_env, "Messenger (verify)").unwrap_or_default();
            let adapter = Arc::new(MessengerAdapter::new(
                page_token,
                verify_token,
                ms_config.webhook_port,
            ));
            adapters.push((adapter, ms_config.default_agent.clone()));
        }
    }

    // Reddit
    if let Some(ref rd_config) = config.reddit {
        if let Some(secret) = read_token(&rd_config.client_secret_env, "Reddit (secret)") {
            if let Some(password) = read_token(&rd_config.password_env, "Reddit (password)") {
                let adapter = Arc::new(RedditAdapter::new(
                    rd_config.client_id.clone(),
                    secret,
                    rd_config.username.clone(),
                    password,
                    rd_config.subreddits.clone(),
                ));
                adapters.push((adapter, rd_config.default_agent.clone()));
            }
        }
    }

    // Mastodon
    if let Some(ref md_config) = config.mastodon {
        if let Some(token) = read_token(&md_config.access_token_env, "Mastodon") {
            let adapter = Arc::new(MastodonAdapter::new(md_config.instance_url.clone(), token));
            adapters.push((adapter, md_config.default_agent.clone()));
        }
    }

    // Bluesky
    if let Some(ref bs_config) = config.bluesky {
        if let Some(password) = read_token(&bs_config.app_password_env, "Bluesky") {
            let adapter = Arc::new(BlueskyAdapter::new(bs_config.identifier.clone(), password));
            adapters.push((adapter, bs_config.default_agent.clone()));
        }
    }

    // Feishu/Lark
    if let Some(ref fs_config) = config.feishu {
        if let Some(secret) = read_token(&fs_config.app_secret_env, "Feishu") {
            let adapter = Arc::new(FeishuAdapter::new(
                fs_config.app_id.clone(),
                secret,
                fs_config.webhook_port,
            ));
            adapters.push((adapter, fs_config.default_agent.clone()));
        }
    }

    // Revolt
    if let Some(ref rv_config) = config.revolt {
        if let Some(token) = read_token(&rv_config.bot_token_env, "Revolt") {
            let adapter = Arc::new(RevoltAdapter::new(token));
            adapters.push((adapter, rv_config.default_agent.clone()));
        }
    }

    // ── Wave 4 ──────────────────────────────────────────────────

    // Nextcloud Talk
    if let Some(ref nc_config) = config.nextcloud {
        if let Some(token) = read_token(&nc_config.token_env, "Nextcloud") {
            let adapter = Arc::new(NextcloudAdapter::new(
                nc_config.server_url.clone(),
                token,
                nc_config.allowed_rooms.clone(),
            ));
            adapters.push((adapter, nc_config.default_agent.clone()));
        }
    }

    // Guilded
    if let Some(ref gd_config) = config.guilded {
        if let Some(token) = read_token(&gd_config.bot_token_env, "Guilded") {
            let adapter = Arc::new(GuildedAdapter::new(token, gd_config.server_ids.clone()));
            adapters.push((adapter, gd_config.default_agent.clone()));
        }
    }

    // Keybase
    if let Some(ref kb_config) = config.keybase {
        if let Some(paperkey) = read_token(&kb_config.paperkey_env, "Keybase") {
            let adapter = Arc::new(KeybaseAdapter::new(
                kb_config.username.clone(),
                paperkey,
                kb_config.allowed_teams.clone(),
            ));
            adapters.push((adapter, kb_config.default_agent.clone()));
        }
    }

    // Threema
    if let Some(ref tm_config) = config.threema {
        if let Some(secret) = read_token(&tm_config.secret_env, "Threema") {
            let adapter = Arc::new(ThreemaAdapter::new(
                tm_config.threema_id.clone(),
                secret,
                tm_config.webhook_port,
            ));
            adapters.push((adapter, tm_config.default_agent.clone()));
        }
    }

    // Nostr
    if let Some(ref ns_config) = config.nostr {
        if let Some(key) = read_token(&ns_config.private_key_env, "Nostr") {
            let adapter = Arc::new(NostrAdapter::new(key, ns_config.relays.clone()));
            adapters.push((adapter, ns_config.default_agent.clone()));
        }
    }

    // Webex
    if let Some(ref wx_config) = config.webex {
        if let Some(token) = read_token(&wx_config.bot_token_env, "Webex") {
            let adapter = Arc::new(WebexAdapter::new(token, wx_config.allowed_rooms.clone()));
            adapters.push((adapter, wx_config.default_agent.clone()));
        }
    }

    // Pumble
    if let Some(ref pb_config) = config.pumble {
        if let Some(token) = read_token(&pb_config.bot_token_env, "Pumble") {
            let adapter = Arc::new(PumbleAdapter::new(token, pb_config.webhook_port));
            adapters.push((adapter, pb_config.default_agent.clone()));
        }
    }

    // Flock
    if let Some(ref fl_config) = config.flock {
        if let Some(token) = read_token(&fl_config.bot_token_env, "Flock") {
            let adapter = Arc::new(FlockAdapter::new(token, fl_config.webhook_port));
            adapters.push((adapter, fl_config.default_agent.clone()));
        }
    }

    // Twist
    if let Some(ref tw_config) = config.twist {
        if let Some(token) = read_token(&tw_config.token_env, "Twist") {
            let adapter = Arc::new(TwistAdapter::new(
                token,
                tw_config.workspace_id.clone(),
                tw_config.allowed_channels.clone(),
            ));
            adapters.push((adapter, tw_config.default_agent.clone()));
        }
    }

    // ── Wave 5 ──────────────────────────────────────────────────

    // Mumble
    if let Some(ref mb_config) = config.mumble {
        if let Some(password) = read_token(&mb_config.password_env, "Mumble") {
            let adapter = Arc::new(MumbleAdapter::new(
                mb_config.host.clone(),
                mb_config.port,
                password,
                mb_config.username.clone(),
                mb_config.channel.clone(),
            ));
            adapters.push((adapter, mb_config.default_agent.clone()));
        }
    }

    // DingTalk
    if let Some(ref dt_config) = config.dingtalk {
        if let Some(token) = read_token(&dt_config.access_token_env, "DingTalk") {
            let secret = read_token(&dt_config.secret_env, "DingTalk (secret)").unwrap_or_default();
            let adapter = Arc::new(DingTalkAdapter::new(token, secret, dt_config.webhook_port));
            adapters.push((adapter, dt_config.default_agent.clone()));
        }
    }

    // Discourse
    if let Some(ref dc_config) = config.discourse {
        if let Some(api_key) = read_token(&dc_config.api_key_env, "Discourse") {
            let adapter = Arc::new(DiscourseAdapter::new(
                dc_config.base_url.clone(),
                api_key,
                dc_config.api_username.clone(),
                dc_config.categories.clone(),
            ));
            adapters.push((adapter, dc_config.default_agent.clone()));
        }
    }

    // Gitter
    if let Some(ref gt_config) = config.gitter {
        if let Some(token) = read_token(&gt_config.token_env, "Gitter") {
            let adapter = Arc::new(GitterAdapter::new(token, gt_config.room_id.clone()));
            adapters.push((adapter, gt_config.default_agent.clone()));
        }
    }

    // ntfy
    if let Some(ref nf_config) = config.ntfy {
        let token = if nf_config.token_env.is_empty() {
            String::new()
        } else {
            read_token(&nf_config.token_env, "ntfy").unwrap_or_default()
        };
        let adapter = Arc::new(NtfyAdapter::new(
            nf_config.server_url.clone(),
            nf_config.topic.clone(),
            token,
        ));
        adapters.push((adapter, nf_config.default_agent.clone()));
    }

    // Gotify
    if let Some(ref gf_config) = config.gotify {
        if let Some(app_token) = read_token(&gf_config.app_token_env, "Gotify (app)") {
            let client_token =
                read_token(&gf_config.client_token_env, "Gotify (client)").unwrap_or_default();
            let adapter = Arc::new(GotifyAdapter::new(
                gf_config.server_url.clone(),
                app_token,
                client_token,
            ));
            adapters.push((adapter, gf_config.default_agent.clone()));
        }
    }

    // Webhook
    if let Some(ref wh_config) = config.webhook {
        if let Some(secret) = read_token(&wh_config.secret_env, "Webhook") {
            let adapter = Arc::new(WebhookAdapter::new(
                secret,
                wh_config.listen_port,
                wh_config.callback_url.clone(),
            ));
            adapters.push((adapter, wh_config.default_agent.clone()));
        }
    }

    // LinkedIn
    if let Some(ref li_config) = config.linkedin {
        if let Some(token) = read_token(&li_config.access_token_env, "LinkedIn") {
            let adapter = Arc::new(LinkedInAdapter::new(
                token,
                li_config.organization_id.clone(),
            ));
            adapters.push((adapter, li_config.default_agent.clone()));
        }
    }

    if adapters.is_empty() {
        return (None, Vec::new());
    }

    // Resolve default agent from first adapter that has one configured
    let mut router = AgentRouter::new();
    for (_, default_agent) in &adapters {
        if let Some(ref name) = default_agent {
            resolve_default_agent(&handle, name, &mut router, "Channel bridge").await;
            break; // Only need one default
        }
    }

    // Load bindings and broadcast config from kernel
    let bindings = kernel.list_bindings();
    if !bindings.is_empty() {
        // Register all known agents in the router's name cache for binding resolution
        for entry in kernel.registry.list() {
            router.register_agent(entry.name.clone(), entry.id);
        }
        router.load_bindings(&bindings);
        info!(count = bindings.len(), "Loaded agent bindings into router");
    }
    router.load_broadcast(kernel.broadcast.clone());

    let bridge_handle: Arc<dyn ChannelBridgeHandle> = Arc::new(KernelBridgeAdapter {
        kernel: kernel.clone(),
        started_at: Instant::now(),
    });
    let router = Arc::new(router);
    let mut manager = BridgeManager::new(bridge_handle, router);

    let mut started_names = Vec::new();
    for (adapter, _) in adapters {
        let name = adapter.name().to_string();
        match manager.start_adapter(adapter).await {
            Ok(()) => {
                info!("{name} channel bridge started");
                started_names.push(name);
            }
            Err(e) => {
                error!("Failed to start {name} bridge: {e}");
            }
        }
    }

    if started_names.is_empty() {
        (None, Vec::new())
    } else {
        (Some(manager), started_names)
    }
}

/// Reload channels from disk config — stops old bridge, starts new one.
///
/// Reads `config.toml` fresh, rebuilds the channel bridge, and stores it
/// in `AppState.bridge_manager`. Returns the list of started channel names.
pub async fn reload_channels_from_disk(
    state: &crate::routes::AppState,
) -> Result<Vec<String>, String> {
    // Stop existing bridge
    {
        let mut guard = state.bridge_manager.lock().await;
        if let Some(ref mut bridge) = *guard {
            bridge.stop().await;
        }
        *guard = None;
    }

    // Re-read config from disk
    let config_path = state.kernel.config.home_dir.join("config.toml");
    let fresh_config = openfang_kernel::config::load_config(Some(&config_path));

    // Update the live channels config so list_channels() reflects reality
    *state.channels_config.write().await = fresh_config.channels.clone();

    // Start new bridge with fresh channel config
    let (new_bridge, started) =
        start_channel_bridge_with_config(state.kernel.clone(), &fresh_config.channels).await;

    // Store the new bridge
    *state.bridge_manager.lock().await = new_bridge;

    info!(
        started = started.len(),
        channels = ?started,
        "Channel hot-reload complete"
    );

    Ok(started)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_bridge_skips_when_no_config() {
        let config = openfang_types::config::KernelConfig::default();
        assert!(config.channels.telegram.is_none());
        assert!(config.channels.discord.is_none());
        assert!(config.channels.slack.is_none());
        assert!(config.channels.whatsapp.is_none());
        assert!(config.channels.signal.is_none());
        assert!(config.channels.matrix.is_none());
        assert!(config.channels.email.is_none());
        assert!(config.channels.teams.is_none());
        assert!(config.channels.mattermost.is_none());
        assert!(config.channels.irc.is_none());
        assert!(config.channels.google_chat.is_none());
        assert!(config.channels.twitch.is_none());
        assert!(config.channels.rocketchat.is_none());
        assert!(config.channels.zulip.is_none());
        assert!(config.channels.xmpp.is_none());
        // Wave 3
        assert!(config.channels.line.is_none());
        assert!(config.channels.viber.is_none());
        assert!(config.channels.messenger.is_none());
        assert!(config.channels.reddit.is_none());
        assert!(config.channels.mastodon.is_none());
        assert!(config.channels.bluesky.is_none());
        assert!(config.channels.feishu.is_none());
        assert!(config.channels.revolt.is_none());
        // Wave 4
        assert!(config.channels.nextcloud.is_none());
        assert!(config.channels.guilded.is_none());
        assert!(config.channels.keybase.is_none());
        assert!(config.channels.threema.is_none());
        assert!(config.channels.nostr.is_none());
        assert!(config.channels.webex.is_none());
        assert!(config.channels.pumble.is_none());
        assert!(config.channels.flock.is_none());
        assert!(config.channels.twist.is_none());
        // Wave 5
        assert!(config.channels.mumble.is_none());
        assert!(config.channels.dingtalk.is_none());
        assert!(config.channels.discourse.is_none());
        assert!(config.channels.gitter.is_none());
        assert!(config.channels.ntfy.is_none());
        assert!(config.channels.gotify.is_none());
        assert!(config.channels.webhook.is_none());
        assert!(config.channels.linkedin.is_none());
    }
}
