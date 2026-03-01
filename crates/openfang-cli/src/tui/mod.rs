//! Ratatui TUI for OpenFang interactive mode.
//!
//! Two-level navigation: Phase::Boot (Welcome/Wizard) → Phase::Main with 16 tabs.

pub mod chat_runner;
pub mod event;
pub mod screens;
pub mod theme;

use event::{AppEvent, BackendRef};
use openfang_kernel::OpenFangKernel;
use openfang_runtime::llm_driver::StreamEvent;
use openfang_types::agent::AgentId;
use screens::{
    agents, audit, channels, chat, comms, dashboard, extensions, hands, logs, memory, peers,
    security, sessions, settings, skills, templates, triggers, usage, welcome, wizard, workflows,
};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

// ─── Core types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Boot(BootScreen),
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BootScreen {
    Welcome,
    Wizard,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Agents,
    Chat,
    Sessions,
    Workflows,
    Triggers,
    Memory,
    Channels,
    Skills,
    Hands,
    Extensions,
    Templates,
    Peers,
    Comms,
    Security,
    Audit,
    Usage,
    Settings,
    Logs,
}

const TABS: &[Tab] = &[
    Tab::Dashboard,
    Tab::Agents,
    Tab::Chat,
    Tab::Sessions,
    Tab::Workflows,
    Tab::Triggers,
    Tab::Memory,
    Tab::Channels,
    Tab::Skills,
    Tab::Hands,
    Tab::Extensions,
    Tab::Templates,
    Tab::Peers,
    Tab::Comms,
    Tab::Security,
    Tab::Audit,
    Tab::Usage,
    Tab::Settings,
    Tab::Logs,
];

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Agents => "Agents",
            Tab::Chat => "Chat",
            Tab::Sessions => "Sessions",
            Tab::Workflows => "Workflows",
            Tab::Triggers => "Triggers",
            Tab::Memory => "Memory",
            Tab::Channels => "Channels",
            Tab::Skills => "Skills",
            Tab::Hands => "Hands",
            Tab::Extensions => "Extensions",
            Tab::Templates => "Templates",
            Tab::Peers => "Peers",
            Tab::Comms => "Comms",
            Tab::Security => "Security",
            Tab::Audit => "Audit",
            Tab::Usage => "Usage",
            Tab::Settings => "Settings",
            Tab::Logs => "Logs",
        }
    }

    fn index(self) -> usize {
        TABS.iter().position(|&t| t == self).unwrap_or(0)
    }
}

enum Backend {
    Daemon { base_url: String },
    InProcess { kernel: Arc<OpenFangKernel> },
    None,
}

impl Backend {
    fn to_ref(&self) -> Option<BackendRef> {
        match self {
            Backend::Daemon { base_url } => Some(BackendRef::Daemon(base_url.clone())),
            Backend::InProcess { kernel } => Some(BackendRef::InProcess(kernel.clone())),
            Backend::None => None,
        }
    }
}

struct ChatTarget {
    agent_id_daemon: Option<String>,
    agent_id_inprocess: Option<AgentId>,
    agent_name: String,
}

struct App {
    phase: Phase,
    active_tab: Tab,
    tab_scroll_offset: usize,
    config_path: Option<PathBuf>,
    should_quit: bool,
    event_tx: mpsc::Sender<AppEvent>,
    /// Double Ctrl+C quit: true after first Ctrl+C press.
    ctrl_c_pending: bool,
    /// Tick counter when first Ctrl+C was pressed (auto-resets after ~2s).
    ctrl_c_tick: usize,
    /// Global tick counter for Ctrl+C timeout tracking.
    tick_count: usize,

    backend: Backend,
    chat_target: Option<ChatTarget>,

    // Screen states
    welcome: welcome::WelcomeState,
    wizard: wizard::WizardState,
    agents: agents::AgentSelectState,
    chat: chat::ChatState,
    dashboard: dashboard::DashboardState,
    channels: channels::ChannelState,
    workflows: workflows::WorkflowState,
    triggers: triggers::TriggerState,
    sessions: sessions::SessionsState,
    memory: memory::MemoryState,
    skills: skills::SkillsState,
    hands: hands::HandsState,
    extensions: extensions::ExtensionsState,
    templates: templates::TemplatesState,
    security: security::SecurityState,
    audit: audit::AuditState,
    usage: usage::UsageState,
    settings: settings::SettingsState,
    peers: peers::PeersState,
    comms: comms::CommsState,
    logs: logs::LogsState,

    kernel_booting: bool,
    kernel_boot_error: Option<String>,
}

// ─── App construction ────────────────────────────────────────────────────────

impl App {
    fn new(config_path: Option<PathBuf>, event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self {
            phase: Phase::Boot(BootScreen::Welcome),
            active_tab: Tab::Dashboard,
            tab_scroll_offset: 0,
            config_path,
            should_quit: false,
            event_tx,
            backend: Backend::None,
            chat_target: None,
            welcome: welcome::WelcomeState::new(),
            wizard: wizard::WizardState::new(),
            agents: agents::AgentSelectState::new(),
            chat: chat::ChatState::new(),
            dashboard: dashboard::DashboardState::new(),
            channels: channels::ChannelState::new(),
            workflows: workflows::WorkflowState::new(),
            triggers: triggers::TriggerState::new(),
            sessions: sessions::SessionsState::new(),
            memory: memory::MemoryState::new(),
            skills: skills::SkillsState::new(),
            hands: hands::HandsState::new(),
            extensions: extensions::ExtensionsState::new(),
            templates: templates::TemplatesState::new(),
            security: security::SecurityState::new(),
            audit: audit::AuditState::new(),
            usage: usage::UsageState::new(),
            settings: settings::SettingsState::new(),
            peers: peers::PeersState::new(),
            comms: comms::CommsState::new(),
            logs: logs::LogsState::new(),
            kernel_booting: false,
            kernel_boot_error: None,
            ctrl_c_pending: false,
            ctrl_c_tick: 0,
            tick_count: 0,
        }
    }

    // ─── Event dispatch ──────────────────────────────────────────────────────

    fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Tick => self.handle_tick(),
            AppEvent::Stream(stream_ev) => self.handle_stream(stream_ev),
            AppEvent::StreamDone(result) => self.handle_stream_done(result),
            AppEvent::KernelReady(kernel) => self.handle_kernel_ready(kernel),
            AppEvent::KernelError(err) => self.handle_kernel_error(err),
            AppEvent::AgentSpawned { id, name } => self.handle_agent_spawned(id, name),
            AppEvent::AgentSpawnError(err) => self.handle_agent_spawn_error(err),
            AppEvent::DaemonDetected { url, agent_count } => {
                self.welcome.on_daemon_detected(url, agent_count);
            }
            // ── New tab events ──
            AppEvent::DashboardData {
                agent_count,
                uptime_secs,
                version,
                provider,
                model,
            } => {
                self.dashboard.agent_count = agent_count;
                self.dashboard.uptime_secs = uptime_secs;
                self.dashboard.version = version;
                self.dashboard.provider = provider;
                self.dashboard.model = model;
                self.dashboard.loading = false;
            }
            AppEvent::AuditLoaded(rows) => {
                self.dashboard.recent_audit = rows;
                self.dashboard.loading = false;
            }
            AppEvent::ChannelListLoaded(list) => {
                if !list.is_empty() {
                    self.channels.channels = list;
                    self.channels.list_state.select(Some(0));
                }
                self.channels.loading = false;
            }
            AppEvent::ChannelTestResult { success, message } => {
                self.channels.test_result = Some((success, message));
            }
            AppEvent::WorkflowListLoaded(list) => {
                self.workflows.workflows = list;
                if !self.workflows.workflows.is_empty() {
                    self.workflows.list_state.select(Some(0));
                }
                self.workflows.loading = false;
            }
            AppEvent::WorkflowRunsLoaded(runs) => {
                self.workflows.runs = runs;
                if !self.workflows.runs.is_empty() {
                    self.workflows.runs_list_state.select(Some(0));
                }
                self.workflows.loading = false;
            }
            AppEvent::WorkflowRunResult(result) => {
                self.workflows.run_result = Some(result);
                self.workflows.loading = false;
            }
            AppEvent::WorkflowCreated(_id) => {
                self.workflows.status_msg = "Workflow created!".to_string();
                self.refresh_workflows();
            }
            AppEvent::TriggerListLoaded(list) => {
                self.triggers.triggers = list;
                if !self.triggers.triggers.is_empty() {
                    self.triggers.list_state.select(Some(0));
                }
                self.triggers.loading = false;
            }
            AppEvent::TriggerCreated(_id) => {
                self.triggers.status_msg = "Trigger created!".to_string();
                self.refresh_triggers();
            }
            AppEvent::TriggerDeleted(id) => {
                self.triggers.triggers.retain(|t| t.id != id);
                self.triggers.status_msg = format!("Trigger {id} deleted.");
            }
            AppEvent::AgentKilled { id } => {
                self.agents.status_msg = format!("Agent {id} killed.");
                self.agents.sub = agents::AgentSubScreen::AgentList;
                self.refresh_agents();
            }
            AppEvent::AgentKillError(err) => {
                self.agents.status_msg = format!("Kill failed: {err}");
            }
            AppEvent::AgentSkillsLoaded {
                assigned,
                available,
            } => {
                // Populate skill editor: mark assigned skills as checked
                self.agents.available_skills = available
                    .into_iter()
                    .map(|name| {
                        let checked = assigned.contains(&name);
                        (name, checked)
                    })
                    .collect();
                self.agents.skill_cursor = 0;
            }
            AppEvent::AgentMcpServersLoaded {
                assigned,
                available,
            } => {
                // Populate MCP editor: mark assigned servers as checked
                self.agents.available_mcp = available
                    .into_iter()
                    .map(|name| {
                        let checked = assigned.contains(&name);
                        (name, checked)
                    })
                    .collect();
                self.agents.mcp_cursor = 0;
            }
            AppEvent::AgentSkillsUpdated(id) => {
                self.agents.status_msg = format!("Skills updated for agent {id}.");
                self.agents.sub = agents::AgentSubScreen::AgentDetail;
            }
            AppEvent::AgentMcpServersUpdated(id) => {
                self.agents.status_msg = format!("MCP servers updated for agent {id}.");
                self.agents.sub = agents::AgentSubScreen::AgentDetail;
            }
            AppEvent::FetchError(err) => {
                // Route to the active tab's status message
                match self.active_tab {
                    Tab::Workflows => self.workflows.status_msg = err,
                    Tab::Triggers => self.triggers.status_msg = err,
                    Tab::Channels => self.channels.status_msg = err,
                    Tab::Sessions => self.sessions.status_msg = err,
                    Tab::Memory => self.memory.status_msg = err,
                    Tab::Skills => self.skills.status_msg = err,
                    Tab::Hands => self.hands.status_msg = err,
                    Tab::Extensions => self.extensions.status_msg = err,
                    Tab::Templates => self.templates.status_msg = err,
                    Tab::Settings => self.settings.status_msg = err,
                    _ => {}
                }
            }

            // ── New screen events ──
            AppEvent::SessionsLoaded(list) => {
                self.sessions.sessions = list;
                self.sessions.refilter();
                self.sessions.loading = false;
            }
            AppEvent::SessionDeleted(id) => {
                self.sessions.sessions.retain(|s| s.id != id);
                self.sessions.refilter();
                self.sessions.status_msg = format!("Session {id} deleted.");
            }
            AppEvent::MemoryAgentsLoaded(agents) => {
                self.memory.agents = agents;
                if !self.memory.agents.is_empty() {
                    self.memory.agent_list_state.select(Some(0));
                }
                self.memory.loading = false;
            }
            AppEvent::MemoryKvLoaded(pairs) => {
                self.memory.kv_pairs = pairs;
                if !self.memory.kv_pairs.is_empty() {
                    self.memory.kv_list_state.select(Some(0));
                }
                self.memory.loading = false;
            }
            AppEvent::MemoryKvSaved { key } => {
                self.memory.status_msg = format!("Saved key: {key}");
                // Refresh KV pairs
                if let Some(agent) = &self.memory.selected_agent {
                    if let Some(backend) = self.backend.to_ref() {
                        event::spawn_fetch_memory_kv(
                            backend,
                            agent.id.clone(),
                            self.event_tx.clone(),
                        );
                    }
                }
            }
            AppEvent::MemoryKvDeleted(key) => {
                self.memory.kv_pairs.retain(|kv| kv.key != key);
                self.memory.status_msg = format!("Deleted key: {key}");
            }
            AppEvent::SkillsLoaded(list) => {
                self.skills.installed = list;
                if !self.skills.installed.is_empty() {
                    self.skills.installed_list.select(Some(0));
                }
                self.skills.loading = false;
            }
            AppEvent::ClawHubLoaded(results) => {
                self.skills.clawhub_results = results;
                if !self.skills.clawhub_results.is_empty() {
                    self.skills.clawhub_list.select(Some(0));
                }
                self.skills.loading = false;
            }
            AppEvent::SkillInstalled(name) => {
                self.skills.status_msg = format!("Installed: {name}");
                self.refresh_skills();
            }
            AppEvent::SkillUninstalled(name) => {
                self.skills.installed.retain(|s| s.name != name);
                self.skills.status_msg = format!("Uninstalled: {name}");
            }
            AppEvent::McpServersLoaded(servers) => {
                self.skills.mcp_servers = servers;
                if !self.skills.mcp_servers.is_empty() {
                    self.skills.mcp_list.select(Some(0));
                }
                self.skills.loading = false;
            }
            AppEvent::TemplateProvidersLoaded(providers) => {
                self.templates.providers = providers;
            }
            AppEvent::SecurityLoaded(features) => {
                self.security.features = features;
                self.security.loading = false;
            }
            AppEvent::SecurityChainVerified { valid, message } => {
                self.security.chain_verified = Some(valid);
                self.security.verify_result = message;
                self.security.loading = false;
            }
            AppEvent::AuditEntriesLoaded(entries) => {
                self.audit.entries = entries;
                self.audit.refilter();
                self.audit.loading = false;
            }
            AppEvent::AuditChainVerified(valid) => {
                self.audit.chain_verified = Some(valid);
            }
            AppEvent::UsageSummaryLoaded(summary) => {
                self.usage.summary = summary;
                self.usage.loading = false;
            }
            AppEvent::UsageByModelLoaded(models) => {
                self.usage.by_model = models;
                if !self.usage.by_model.is_empty() {
                    self.usage.model_list.select(Some(0));
                }
            }
            AppEvent::UsageByAgentLoaded(agents) => {
                self.usage.by_agent = agents;
                if !self.usage.by_agent.is_empty() {
                    self.usage.agent_list.select(Some(0));
                }
            }
            AppEvent::SettingsProvidersLoaded(providers) => {
                self.settings.providers = providers;
                if !self.settings.providers.is_empty() {
                    self.settings.provider_list.select(Some(0));
                }
                self.settings.loading = false;
            }
            AppEvent::SettingsModelsLoaded(models) => {
                self.settings.models = models;
                if !self.settings.models.is_empty() {
                    self.settings.model_list.select(Some(0));
                }
                self.settings.loading = false;
            }
            AppEvent::SettingsToolsLoaded(tools) => {
                self.settings.tools = tools;
                if !self.settings.tools.is_empty() {
                    self.settings.tool_list.select(Some(0));
                }
                self.settings.loading = false;
            }
            AppEvent::ProviderKeySaved(name) => {
                self.settings.status_msg = format!("Key saved for {name}");
                self.refresh_settings_providers();
            }
            AppEvent::ProviderKeyDeleted(name) => {
                self.settings.status_msg = format!("Key deleted for {name}");
                self.refresh_settings_providers();
            }
            AppEvent::ProviderTestResult(result) => {
                self.settings.test_result = Some(result);
            }
            AppEvent::PeersLoaded(list) => {
                self.peers.peers = list;
                if !self.peers.peers.is_empty() && self.peers.list_state.selected().is_none() {
                    self.peers.list_state.select(Some(0));
                }
                self.peers.loading = false;
            }
            AppEvent::CommsTopologyLoaded { nodes, edges } => {
                self.comms.nodes = nodes;
                self.comms.edges = edges;
                self.comms.loading = false;
            }
            AppEvent::CommsEventsLoaded(events) => {
                self.comms.events = events;
                if !self.comms.events.is_empty()
                    && self.comms.event_list_state.selected().is_none()
                {
                    self.comms.event_list_state.select(Some(0));
                }
            }
            AppEvent::CommsSendResult(msg) => {
                self.comms.status_msg = msg;
                self.refresh_comms();
            }
            AppEvent::CommsTaskResult(msg) => {
                self.comms.status_msg = msg;
            }
            AppEvent::LogsLoaded(entries) => {
                self.logs.entries = entries;
                self.logs.refilter();
                self.logs.loading = false;
            }
            AppEvent::HandsLoaded(list) => {
                self.hands.definitions = list;
                if !self.hands.definitions.is_empty() {
                    self.hands.marketplace_list.select(Some(0));
                }
                self.hands.loading = false;
            }
            AppEvent::ActiveHandsLoaded(list) => {
                self.hands.instances = list;
                if !self.hands.instances.is_empty() && self.hands.active_list.selected().is_none() {
                    self.hands.active_list.select(Some(0));
                }
                self.hands.loading = false;
            }
            AppEvent::HandActivated(name) => {
                self.hands.status_msg = format!("Activated: {name}");
                self.refresh_hands();
            }
            AppEvent::HandDeactivated(id) => {
                self.hands.instances.retain(|i| i.instance_id != id);
                self.hands.status_msg = format!("Deactivated: {id}");
            }
            AppEvent::HandPaused(id) => {
                if let Some(inst) = self
                    .hands
                    .instances
                    .iter_mut()
                    .find(|i| i.instance_id == id)
                {
                    inst.status = "Paused".to_string();
                }
                self.hands.status_msg = "Hand paused".to_string();
            }
            AppEvent::HandResumed(id) => {
                if let Some(inst) = self
                    .hands
                    .instances
                    .iter_mut()
                    .find(|i| i.instance_id == id)
                {
                    inst.status = "Active".to_string();
                }
                self.hands.status_msg = "Hand resumed".to_string();
            }
            AppEvent::ExtensionsLoaded(list) => {
                self.extensions.all_extensions = list;
                if !self.extensions.all_extensions.is_empty()
                    && self.extensions.browse_list.selected().is_none()
                {
                    self.extensions.browse_list.select(Some(0));
                }
                self.extensions.loading = false;
            }
            AppEvent::ExtensionHealthLoaded(entries) => {
                self.extensions.health_entries = entries;
                if !self.extensions.health_entries.is_empty()
                    && self.extensions.health_list.selected().is_none()
                {
                    self.extensions.health_list.select(Some(0));
                }
            }
            AppEvent::ExtensionInstalled(id) => {
                self.extensions.status_msg = format!("Installed: {id}");
                self.refresh_extensions();
            }
            AppEvent::ExtensionRemoved(id) => {
                self.extensions.status_msg = format!("Removed: {id}");
                self.refresh_extensions();
            }
            AppEvent::ExtensionReconnected(id, tools) => {
                self.extensions.status_msg = format!("Reconnected {id}: {tools} tools");
                self.refresh_extension_health();
            }
        }
    }

    fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        // ── Global: Double Ctrl+C to quit (all phases) ──────────────────────
        let is_ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if is_ctrl_c {
            if self.ctrl_c_pending {
                self.should_quit = true;
                return;
            }
            self.ctrl_c_pending = true;
            self.ctrl_c_tick = self.tick_count;
            // In Main phase, don't pass the first Ctrl+C to screen handlers —
            // just show the "press again to quit" hint (rendered in status bar).
            if matches!(self.phase, Phase::Main) {
                return;
            }
            // In Boot phase, let it fall through to the welcome/wizard handler
            // which has its own double-Ctrl+C logic.
        } else {
            // Any other key clears the pending Ctrl+C state
            self.ctrl_c_pending = false;
        }

        // ── Global: Ctrl+Q quit from Main phase ─────────────────────────────
        if matches!(self.phase, Phase::Main) {
            if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.should_quit = true;
                return;
            }
            // Tab switching: F1-F12 for direct jump (reliable on all terminals)
            match key.code {
                KeyCode::F(1) => {
                    self.switch_tab(Tab::Dashboard);
                    return;
                }
                KeyCode::F(2) => {
                    self.switch_tab(Tab::Agents);
                    return;
                }
                KeyCode::F(3) => {
                    self.switch_tab(Tab::Chat);
                    return;
                }
                KeyCode::F(4) => {
                    self.switch_tab(Tab::Sessions);
                    return;
                }
                KeyCode::F(5) => {
                    self.switch_tab(Tab::Workflows);
                    return;
                }
                KeyCode::F(6) => {
                    self.switch_tab(Tab::Triggers);
                    return;
                }
                KeyCode::F(7) => {
                    self.switch_tab(Tab::Memory);
                    return;
                }
                KeyCode::F(8) => {
                    self.switch_tab(Tab::Channels);
                    return;
                }
                KeyCode::F(9) => {
                    self.switch_tab(Tab::Skills);
                    return;
                }
                KeyCode::F(10) => {
                    self.switch_tab(Tab::Templates);
                    return;
                }
                KeyCode::F(11) => {
                    self.switch_tab(Tab::Peers);
                    return;
                }
                KeyCode::F(12) => {
                    self.switch_tab(Tab::Security);
                    return;
                }
                _ => {}
            }
            // Tab cycling: Tab / Shift+Tab
            if key.code == KeyCode::Tab && key.modifiers.is_empty() {
                self.next_tab();
                return;
            }
            if key.code == KeyCode::BackTab {
                self.prev_tab();
                return;
            }
            // Tab cycling: Ctrl+Left/Right
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Left => {
                        self.prev_tab();
                        return;
                    }
                    KeyCode::Right => {
                        self.next_tab();
                        return;
                    }
                    _ => {}
                }
            }
            // Tab cycling: Ctrl+[ / Ctrl+] (reliable on MINGW/Windows terminals)
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('[') => {
                        self.prev_tab();
                        return;
                    }
                    KeyCode::Char(']') => {
                        self.next_tab();
                        return;
                    }
                    _ => {}
                }
            }
            // Fallback: Alt+1-9,0
            if key.modifiers.contains(KeyModifiers::ALT) {
                match key.code {
                    KeyCode::Char('1') => {
                        self.switch_tab(Tab::Dashboard);
                        return;
                    }
                    KeyCode::Char('2') => {
                        self.switch_tab(Tab::Agents);
                        return;
                    }
                    KeyCode::Char('3') => {
                        self.switch_tab(Tab::Chat);
                        return;
                    }
                    KeyCode::Char('4') => {
                        self.switch_tab(Tab::Sessions);
                        return;
                    }
                    KeyCode::Char('5') => {
                        self.switch_tab(Tab::Workflows);
                        return;
                    }
                    KeyCode::Char('6') => {
                        self.switch_tab(Tab::Triggers);
                        return;
                    }
                    KeyCode::Char('7') => {
                        self.switch_tab(Tab::Memory);
                        return;
                    }
                    KeyCode::Char('8') => {
                        self.switch_tab(Tab::Channels);
                        return;
                    }
                    KeyCode::Char('9') => {
                        self.switch_tab(Tab::Skills);
                        return;
                    }
                    KeyCode::Char('0') => {
                        self.switch_tab(Tab::Templates);
                        return;
                    }
                    _ => {}
                }
            }
        }

        // ── Route to screen handler ─────────────────────────────────────────
        match self.phase {
            Phase::Boot(BootScreen::Welcome) => {
                if let Some(action) = self.welcome.handle_key(key) {
                    self.handle_welcome_action(action);
                }
            }
            Phase::Boot(BootScreen::Wizard) => match self.wizard.handle_key(key) {
                wizard::WizardResult::Cancelled => {
                    self.phase = Phase::Boot(BootScreen::Welcome);
                    self.start_daemon_detect();
                }
                wizard::WizardResult::Continue => {
                    if self.wizard.step == wizard::WizardStep::Done
                        && self.wizard.created_config.is_some()
                    {
                        self.config_path = self.wizard.created_config.clone();
                        self.welcome.setup_just_completed = true;
                        self.phase = Phase::Boot(BootScreen::Welcome);
                        self.start_daemon_detect();
                    }
                }
            },
            Phase::Main => match self.active_tab {
                Tab::Dashboard => {
                    let action = self.dashboard.handle_key(key);
                    self.handle_dashboard_action(action);
                }
                Tab::Agents => {
                    let action = self.agents.handle_key(key);
                    self.handle_agent_action(action);
                }
                Tab::Chat => {
                    let action = self.chat.handle_key(key);
                    self.handle_chat_action(action);
                }
                Tab::Channels => {
                    let action = self.channels.handle_key(key);
                    self.handle_channel_action(action);
                }
                Tab::Workflows => {
                    let action = self.workflows.handle_key(key);
                    self.handle_workflow_action(action);
                }
                Tab::Triggers => {
                    let action = self.triggers.handle_key(key);
                    self.handle_trigger_action(action);
                }
                Tab::Sessions => {
                    let action = self.sessions.handle_key(key);
                    self.handle_sessions_action(action);
                }
                Tab::Memory => {
                    let action = self.memory.handle_key(key);
                    self.handle_memory_action(action);
                }
                Tab::Skills => {
                    let action = self.skills.handle_key(key);
                    self.handle_skills_action(action);
                }
                Tab::Extensions => {
                    let action = self.extensions.handle_key(key);
                    self.handle_extensions_action(action);
                }
                Tab::Hands => {
                    let action = self.hands.handle_key(key);
                    self.handle_hands_action(action);
                }
                Tab::Templates => {
                    let action = self.templates.handle_key(key);
                    self.handle_templates_action(action);
                }
                Tab::Security => {
                    let action = self.security.handle_key(key);
                    self.handle_security_action(action);
                }
                Tab::Audit => {
                    let action = self.audit.handle_key(key);
                    self.handle_audit_action(action);
                }
                Tab::Usage => {
                    let action = self.usage.handle_key(key);
                    self.handle_usage_action(action);
                }
                Tab::Settings => {
                    let action = self.settings.handle_key(key);
                    self.handle_settings_action(action);
                }
                Tab::Peers => {
                    let action = self.peers.handle_key(key);
                    self.handle_peers_action(action);
                }
                Tab::Comms => {
                    let action = self.comms.handle_key(key);
                    self.handle_comms_action(action);
                }
                Tab::Logs => {
                    let action = self.logs.handle_key(key);
                    self.handle_logs_action(action);
                }
            },
        }
    }

    fn handle_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        // Auto-reset Ctrl+C pending after ~2s (40 ticks at 50ms)
        if self.ctrl_c_pending && self.tick_count.wrapping_sub(self.ctrl_c_tick) > 40 {
            self.ctrl_c_pending = false;
        }
        self.welcome.tick();
        self.chat.tick();
        self.dashboard.tick();
        self.channels.tick();
        self.workflows.tick();
        self.triggers.tick();
        self.sessions.tick();
        self.memory.tick();
        self.skills.tick();
        self.hands.tick();
        self.extensions.tick();
        self.templates.tick();
        self.security.tick();
        self.audit.tick();
        self.usage.tick();
        self.settings.tick();
        self.peers.tick();
        self.comms.tick();
        self.logs.tick();

        // Auto-poll for active tabs
        if self.phase == Phase::Main {
            match self.active_tab {
                Tab::Logs if self.logs.should_poll() => self.refresh_logs(),
                Tab::Peers if self.peers.should_poll() => self.refresh_peers(),
                Tab::Comms if self.comms.should_poll() => self.refresh_comms(),
                _ => {}
            }
        }
    }

    // ─── Tab navigation ──────────────────────────────────────────────────────

    fn next_tab(&mut self) {
        let idx = self.active_tab.index();
        let next = (idx + 1) % TABS.len();
        self.switch_tab(TABS[next]);
    }

    fn prev_tab(&mut self) {
        let idx = self.active_tab.index();
        let prev = if idx == 0 { TABS.len() - 1 } else { idx - 1 };
        self.switch_tab(TABS[prev]);
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
        // Keep active tab visible in the scrollable tab bar
        let idx = tab.index();
        if idx < self.tab_scroll_offset {
            self.tab_scroll_offset = idx;
        }
        // Will be further adjusted during draw based on actual width
        self.on_tab_enter(tab);
    }

    /// Called when a tab becomes active — load data if needed.
    fn on_tab_enter(&mut self, tab: Tab) {
        match tab {
            Tab::Dashboard => self.refresh_dashboard(),
            Tab::Agents => self.refresh_agents(),
            Tab::Channels => self.refresh_channels(),
            Tab::Workflows => self.refresh_workflows(),
            Tab::Triggers => self.refresh_triggers(),
            Tab::Sessions => self.refresh_sessions(),
            Tab::Memory => self.refresh_memory(),
            Tab::Skills => self.refresh_skills(),
            Tab::Hands => self.refresh_hands(),
            Tab::Extensions => self.refresh_extensions(),
            Tab::Templates => self.refresh_templates(),
            Tab::Security => self.refresh_security(),
            Tab::Audit => self.refresh_audit(),
            Tab::Usage => self.refresh_usage(),
            Tab::Settings => self.refresh_settings_providers(),
            Tab::Peers => self.refresh_peers(),
            Tab::Comms => self.refresh_comms(),
            Tab::Logs => self.refresh_logs(),
            Tab::Chat => {} // Chat doesn't need refresh on enter
        }
    }

    /// Transition from Boot to Main phase.
    fn enter_main_phase(&mut self) {
        self.phase = Phase::Main;
        self.active_tab = Tab::Agents;
        // Load initial data for visible tabs
        self.refresh_agents();
        self.refresh_dashboard();
        self.refresh_channels();
    }

    // ─── Data refresh helpers ────────────────────────────────────────────────

    fn refresh_dashboard(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.dashboard.loading = true;
            event::spawn_fetch_dashboard(backend, self.event_tx.clone());
        }
    }

    fn refresh_agents(&mut self) {
        match &self.backend {
            Backend::Daemon { base_url } => {
                self.agents.load_daemon_agents(base_url);
            }
            Backend::InProcess { kernel } => {
                self.agents.load_inprocess_agents(kernel);
            }
            Backend::None => {}
        }
    }

    fn refresh_channels(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.channels.loading = true;
            event::spawn_fetch_channels(backend, self.event_tx.clone());
        }
        // Also build defaults from env detection
        if self.channels.channels.is_empty() {
            self.channels.build_default_channels();
        }
    }

    fn refresh_workflows(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.workflows.loading = true;
            event::spawn_fetch_workflows(backend, self.event_tx.clone());
        }
    }

    fn refresh_triggers(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.triggers.loading = true;
            event::spawn_fetch_triggers(backend, self.event_tx.clone());
        }
    }

    fn refresh_sessions(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.sessions.loading = true;
            event::spawn_fetch_sessions(backend, self.event_tx.clone());
        }
    }

    fn refresh_memory(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.memory.loading = true;
            event::spawn_fetch_memory_agents(backend, self.event_tx.clone());
        }
    }

    fn refresh_skills(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.skills.loading = true;
            event::spawn_fetch_skills(backend, self.event_tx.clone());
        }
    }

    fn refresh_hands(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.hands.loading = true;
            event::spawn_fetch_hands(backend.clone(), self.event_tx.clone());
            event::spawn_fetch_active_hands(backend, self.event_tx.clone());
        }
    }

    fn refresh_extensions(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.extensions.loading = true;
            event::spawn_fetch_extensions(backend, self.event_tx.clone());
        }
    }

    fn refresh_extension_health(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            event::spawn_fetch_extension_health(backend, self.event_tx.clone());
        }
    }

    fn refresh_templates(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            event::spawn_fetch_template_providers(backend, self.event_tx.clone());
        }
    }

    fn refresh_security(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.security.loading = true;
            event::spawn_fetch_security(backend, self.event_tx.clone());
        }
    }

    fn refresh_audit(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.audit.loading = true;
            event::spawn_fetch_audit(backend, self.event_tx.clone());
        }
    }

    fn refresh_usage(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.usage.loading = true;
            event::spawn_fetch_usage(backend, self.event_tx.clone());
        }
    }

    fn refresh_settings_providers(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.settings.loading = true;
            event::spawn_fetch_providers(backend, self.event_tx.clone());
        }
    }

    fn refresh_settings_models(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.settings.loading = true;
            event::spawn_fetch_models(backend, self.event_tx.clone());
        }
    }

    fn refresh_settings_tools(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            event::spawn_fetch_tools(backend, self.event_tx.clone());
        }
    }

    fn refresh_peers(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.peers.loading = true;
            event::spawn_fetch_peers(backend, self.event_tx.clone());
        }
    }

    fn refresh_comms(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.comms.loading = true;
            event::spawn_fetch_comms(backend, self.event_tx.clone());
        }
    }

    fn refresh_logs(&mut self) {
        if let Some(backend) = self.backend.to_ref() {
            self.logs.loading = true;
            event::spawn_fetch_logs(backend, self.event_tx.clone());
        }
    }

    // ─── Streaming ───────────────────────────────────────────────────────────

    fn handle_stream(&mut self, ev: StreamEvent) {
        match ev {
            StreamEvent::TextDelta { text } => {
                self.chat.thinking = false;
                if self.chat.active_tool.is_some() {
                    self.chat.active_tool = None;
                }
                self.chat.append_stream(&text);
            }
            StreamEvent::ToolUseStart { name, .. } => {
                if !self.chat.streaming_text.is_empty() {
                    let text = std::mem::take(&mut self.chat.streaming_text);
                    self.chat.push_message(chat::Role::Agent, text);
                }
                self.chat.tool_start(&name);
            }
            StreamEvent::ToolInputDelta { text } => {
                self.chat.tool_input_buf.push_str(&text);
            }
            StreamEvent::ToolUseEnd { name, input, .. } => {
                let input_str = if !self.chat.tool_input_buf.is_empty() {
                    std::mem::take(&mut self.chat.tool_input_buf)
                } else {
                    serde_json::to_string(&input).unwrap_or_default()
                };
                self.chat.tool_use_end(&name, &input_str);
            }
            StreamEvent::ContentComplete { usage, .. } => {
                self.chat.last_tokens = Some((usage.input_tokens, usage.output_tokens));
            }
            StreamEvent::PhaseChange { phase, detail } => {
                if phase == "tool_use" {
                    if let Some(tool_name) = detail {
                        self.chat.tool_start(&tool_name);
                    }
                } else if phase == "thinking" {
                    self.chat.thinking = true;
                }
            }
            StreamEvent::ThinkingDelta { text } => {
                self.chat.thinking = true;
                self.chat.append_stream(&text);
            }
            StreamEvent::ToolExecutionResult {
                name,
                result_preview,
                is_error,
            } => {
                self.chat.tool_result(&name, &result_preview, is_error);
            }
        }
    }

    fn handle_stream_done(
        &mut self,
        result: Result<openfang_runtime::agent_loop::AgentLoopResult, String>,
    ) {
        self.chat.finalize_stream();
        match result {
            Ok(r) => {
                // Only add if the response wasn't already streamed
                if !r.response.is_empty()
                    && self.chat.messages.last().map(|m| m.text.as_str()) != Some(&r.response)
                {
                    self.chat.push_message(chat::Role::Agent, r.response);
                }
                if r.total_usage.input_tokens > 0 || r.total_usage.output_tokens > 0 {
                    self.chat.last_tokens =
                        Some((r.total_usage.input_tokens, r.total_usage.output_tokens));
                }
            }
            Err(e) => {
                self.chat.status_msg = Some(format!("Error: {e}"));
            }
        }
        // Auto-send the next staged message if any
        if let Some(msg) = self.chat.take_staged() {
            self.send_message(msg);
        }
    }

    // ─── Kernel lifecycle ────────────────────────────────────────────────────

    fn handle_kernel_ready(&mut self, kernel: Arc<OpenFangKernel>) {
        self.kernel_booting = false;
        self.backend = Backend::InProcess { kernel };
        self.agents.reset();
        self.enter_main_phase();
    }

    fn handle_kernel_error(&mut self, err: String) {
        self.kernel_booting = false;
        self.kernel_boot_error = Some(err.clone());
        if err.contains("Missing API key") || err.contains("api_key") {
            self.wizard.reset();
            self.phase = Phase::Boot(BootScreen::Wizard);
        } else {
            self.phase = Phase::Boot(BootScreen::Welcome);
            self.start_daemon_detect();
        }
    }

    fn handle_agent_spawned(&mut self, id: String, name: String) {
        self.agents.sub = agents::AgentSubScreen::AgentList;
        self.enter_chat_daemon(id, name);
    }

    fn handle_agent_spawn_error(&mut self, err: String) {
        self.agents.status_msg = err;
        self.agents.sub = agents::AgentSubScreen::AgentList;
    }

    // ─── Screen transitions ──────────────────────────────────────────────────

    fn start_daemon_detect(&mut self) {
        self.welcome.detecting = true;
        event::spawn_daemon_detect(self.event_tx.clone());
    }

    fn handle_welcome_action(&mut self, action: welcome::WelcomeAction) {
        match action {
            welcome::WelcomeAction::Exit => self.should_quit = true,
            welcome::WelcomeAction::ConnectDaemon => {
                if let Some(ref url) = self.welcome.daemon_url {
                    self.backend = Backend::Daemon {
                        base_url: url.clone(),
                    };
                    self.agents.reset();
                    self.enter_main_phase();
                }
            }
            welcome::WelcomeAction::InProcess => {
                if self.kernel_booting {
                    return;
                }
                self.kernel_booting = true;
                self.kernel_boot_error = None;
                event::spawn_kernel_boot(self.config_path.clone(), self.event_tx.clone());
            }
            welcome::WelcomeAction::Wizard => {
                self.wizard.reset();
                self.phase = Phase::Boot(BootScreen::Wizard);
            }
        }
    }

    // ─── Tab action handlers ─────────────────────────────────────────────────

    fn handle_dashboard_action(&mut self, action: dashboard::DashboardAction) {
        match action {
            dashboard::DashboardAction::Continue => {}
            dashboard::DashboardAction::Refresh => self.refresh_dashboard(),
            dashboard::DashboardAction::GoToAgents => {
                self.switch_tab(Tab::Agents);
            }
        }
    }

    fn handle_agent_action(&mut self, action: agents::AgentAction) {
        match action {
            agents::AgentAction::Continue => {}
            agents::AgentAction::Back => {
                // In Main phase, Esc from agents just stays on the tab
            }
            agents::AgentAction::CreatedManifest(toml_content) => {
                self.spawn_agent(toml_content);
            }
            agents::AgentAction::ChatWithAgent { id, name } => {
                // From detail view — enter chat with this agent
                if let Some(agent) = self.agents.daemon_agents.iter().find(|a| a.id == id) {
                    self.enter_chat_daemon(agent.id.clone(), agent.name.clone());
                } else if let Some(agent) = self
                    .agents
                    .inprocess_agents
                    .iter()
                    .find(|a| format!("{}", a.id) == id)
                {
                    self.enter_chat_inprocess(agent.id, agent.name.clone());
                } else {
                    // Fallback: treat as daemon
                    self.enter_chat_daemon(id, name);
                }
            }
            agents::AgentAction::KillAgent(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_kill_agent(backend, id, self.event_tx.clone());
                }
            }
            agents::AgentAction::UpdateSkills { id, skills } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_update_agent_skills(backend, id, skills, self.event_tx.clone());
                }
            }
            agents::AgentAction::UpdateMcpServers { id, servers } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_update_agent_mcp_servers(
                        backend,
                        id,
                        servers,
                        self.event_tx.clone(),
                    );
                }
            }
            agents::AgentAction::FetchAgentSkills(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_fetch_agent_skills(backend, id, self.event_tx.clone());
                }
            }
            agents::AgentAction::FetchAgentMcpServers(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_fetch_agent_mcp_servers(backend, id, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_chat_action(&mut self, action: chat::ChatAction) {
        match action {
            chat::ChatAction::Continue => {}
            chat::ChatAction::Back => {
                // In Main phase, go back to Agents tab
                self.chat.reset();
                self.chat_target = None;
                self.switch_tab(Tab::Agents);
            }
            chat::ChatAction::SendMessage(msg) => self.send_message(msg),
            chat::ChatAction::SlashCommand(cmd) => self.handle_slash_command(&cmd),
        }
    }

    fn handle_channel_action(&mut self, action: channels::ChannelAction) {
        match action {
            channels::ChannelAction::Continue => {}
            channels::ChannelAction::Refresh => self.refresh_channels(),
            channels::ChannelAction::TestChannel(name) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_test_channel(backend, name, self.event_tx.clone());
                }
            }
            channels::ChannelAction::ToggleChannel(_name, _enabled) => {
                // Toggle is handled locally in the state; daemon toggle
                // could be spawned here if the API supports it.
            }
            channels::ChannelAction::SaveChannel(name, values) => {
                // Save channel credentials via daemon API
                if let Some(backend) = self.backend.to_ref() {
                    let tx = self.event_tx.clone();
                    std::thread::spawn(move || {
                        if let event::BackendRef::Daemon(base_url) = backend {
                            let client = reqwest::blocking::Client::builder()
                                .timeout(std::time::Duration::from_secs(10))
                                .build()
                                .ok();
                            if let Some(client) = client {
                                let mut fields = serde_json::Map::new();
                                for (k, v) in &values {
                                    fields.insert(k.clone(), serde_json::Value::String(v.clone()));
                                }
                                let body = serde_json::json!({ "fields": fields });
                                let _ = client
                                    .post(format!("{base_url}/api/channels/{name}/configure"))
                                    .json(&body)
                                    .send();
                            }
                        }
                        // Signal tick so the UI refreshes next cycle
                        let _ = tx.send(event::AppEvent::Tick);
                    });
                }
                // Immediately trigger a refresh of the channel list
                self.refresh_channels();
            }
        }
    }

    fn handle_workflow_action(&mut self, action: workflows::WorkflowAction) {
        match action {
            workflows::WorkflowAction::Continue => {}
            workflows::WorkflowAction::Refresh => self.refresh_workflows(),
            workflows::WorkflowAction::LoadRuns(wf_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    self.workflows.loading = true;
                    event::spawn_fetch_workflow_runs(backend, wf_id, self.event_tx.clone());
                }
            }
            workflows::WorkflowAction::CreateWorkflow {
                name,
                description,
                steps_json,
            } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_create_workflow(
                        backend,
                        name,
                        description,
                        steps_json,
                        self.event_tx.clone(),
                    );
                }
            }
            workflows::WorkflowAction::RunWorkflow { id, input } => {
                if let Some(backend) = self.backend.to_ref() {
                    self.workflows.loading = true;
                    event::spawn_run_workflow(backend, id, input, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_trigger_action(&mut self, action: triggers::TriggerAction) {
        match action {
            triggers::TriggerAction::Continue => {}
            triggers::TriggerAction::Refresh => self.refresh_triggers(),
            triggers::TriggerAction::CreateTrigger {
                agent_id,
                pattern_type,
                pattern_param,
                prompt,
                max_fires,
            } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_create_trigger(
                        backend,
                        agent_id,
                        pattern_type,
                        pattern_param,
                        prompt,
                        max_fires,
                        self.event_tx.clone(),
                    );
                }
            }
            triggers::TriggerAction::DeleteTrigger(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_delete_trigger(backend, id, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_sessions_action(&mut self, action: sessions::SessionsAction) {
        match action {
            sessions::SessionsAction::Continue => {}
            sessions::SessionsAction::Refresh => self.refresh_sessions(),
            sessions::SessionsAction::OpenInChat {
                agent_id,
                agent_name,
            } => {
                self.enter_chat_daemon(agent_id, agent_name);
            }
            sessions::SessionsAction::DeleteSession(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_delete_session(backend, id, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_memory_action(&mut self, action: memory::MemoryAction) {
        match action {
            memory::MemoryAction::Continue => {}
            memory::MemoryAction::LoadAgents => self.refresh_memory(),
            memory::MemoryAction::LoadKv(agent_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    self.memory.loading = true;
                    event::spawn_fetch_memory_kv(backend, agent_id, self.event_tx.clone());
                }
            }
            memory::MemoryAction::SaveKv {
                agent_id,
                key,
                value,
            } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_save_memory_kv(
                        backend,
                        agent_id,
                        key,
                        value,
                        self.event_tx.clone(),
                    );
                }
            }
            memory::MemoryAction::DeleteKv { agent_id, key } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_delete_memory_kv(backend, agent_id, key, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_skills_action(&mut self, action: skills::SkillsAction) {
        match action {
            skills::SkillsAction::Continue => {}
            skills::SkillsAction::RefreshInstalled => self.refresh_skills(),
            skills::SkillsAction::SearchClawHub(query) => {
                if let Some(backend) = self.backend.to_ref() {
                    self.skills.loading = true;
                    event::spawn_search_clawhub(backend, query, self.event_tx.clone());
                }
            }
            skills::SkillsAction::BrowseClawHub(sort) => {
                if let Some(backend) = self.backend.to_ref() {
                    self.skills.loading = true;
                    event::spawn_browse_clawhub(backend, sort, self.event_tx.clone());
                }
            }
            skills::SkillsAction::InstallSkill(slug) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_install_skill(backend, slug, self.event_tx.clone());
                }
            }
            skills::SkillsAction::UninstallSkill(name) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_uninstall_skill(backend, name, self.event_tx.clone());
                }
            }
            skills::SkillsAction::RefreshMcp => {
                if let Some(backend) = self.backend.to_ref() {
                    self.skills.loading = true;
                    event::spawn_fetch_mcp_servers(backend, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_extensions_action(&mut self, action: extensions::ExtensionsAction) {
        match action {
            extensions::ExtensionsAction::Continue => {}
            extensions::ExtensionsAction::RefreshAll => self.refresh_extensions(),
            extensions::ExtensionsAction::RefreshHealth => self.refresh_extension_health(),
            extensions::ExtensionsAction::Install(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_install_extension(backend, id, self.event_tx.clone());
                }
            }
            extensions::ExtensionsAction::Remove(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_remove_extension(backend, id, self.event_tx.clone());
                }
            }
            extensions::ExtensionsAction::Reconnect(id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_reconnect_extension(backend, id, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_hands_action(&mut self, action: hands::HandsAction) {
        match action {
            hands::HandsAction::Continue => {}
            hands::HandsAction::RefreshDefinitions => self.refresh_hands(),
            hands::HandsAction::RefreshActive => {
                if let Some(backend) = self.backend.to_ref() {
                    self.hands.loading = true;
                    event::spawn_fetch_active_hands(backend, self.event_tx.clone());
                }
            }
            hands::HandsAction::ActivateHand(hand_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_activate_hand(backend, hand_id, self.event_tx.clone());
                }
            }
            hands::HandsAction::DeactivateHand(instance_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_deactivate_hand(backend, instance_id, self.event_tx.clone());
                }
            }
            hands::HandsAction::PauseHand(instance_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_pause_hand(backend, instance_id, self.event_tx.clone());
                }
            }
            hands::HandsAction::ResumeHand(instance_id) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_resume_hand(backend, instance_id, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_templates_action(&mut self, action: templates::TemplatesAction) {
        match action {
            templates::TemplatesAction::Continue => {}
            templates::TemplatesAction::Refresh => self.refresh_templates(),
            templates::TemplatesAction::SpawnTemplate(name) => {
                // Find template and generate TOML manifest
                if let Some(t) = self.templates.templates.iter().find(|t| t.name == name) {
                    let toml_content = format!(
                        "name = \"{}\"\ndescription = \"{}\"\n\n[model]\nprovider = \"{}\"\nmodel = \"{}\"\n\n[capabilities]\ntools = [\"shell\", \"file_read\", \"file_write\", \"web_fetch\", \"web_search\"]\n",
                        t.name, t.description, t.provider, t.model,
                    );
                    self.spawn_agent(toml_content);
                }
            }
        }
    }

    fn handle_security_action(&mut self, action: security::SecurityAction) {
        match action {
            security::SecurityAction::Continue => {}
            security::SecurityAction::Refresh => self.refresh_security(),
            security::SecurityAction::VerifyChain => {
                if let Some(backend) = self.backend.to_ref() {
                    self.security.loading = true;
                    event::spawn_verify_chain(backend, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_audit_action(&mut self, action: audit::AuditAction) {
        match action {
            audit::AuditAction::Continue => {}
            audit::AuditAction::Refresh => self.refresh_audit(),
            audit::AuditAction::VerifyChain => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_verify_chain(backend, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_usage_action(&mut self, action: usage::UsageAction) {
        match action {
            usage::UsageAction::Continue => {}
            usage::UsageAction::Refresh => self.refresh_usage(),
        }
    }

    fn handle_settings_action(&mut self, action: settings::SettingsAction) {
        match action {
            settings::SettingsAction::Continue => {}
            settings::SettingsAction::RefreshProviders => self.refresh_settings_providers(),
            settings::SettingsAction::RefreshModels => self.refresh_settings_models(),
            settings::SettingsAction::RefreshTools => self.refresh_settings_tools(),
            settings::SettingsAction::SaveProviderKey { name, key } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_save_provider_key(backend, name, key, self.event_tx.clone());
                }
            }
            settings::SettingsAction::DeleteProviderKey(name) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_delete_provider_key(backend, name, self.event_tx.clone());
                }
            }
            settings::SettingsAction::TestProvider(name) => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_test_provider(backend, name, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_peers_action(&mut self, action: peers::PeersAction) {
        match action {
            peers::PeersAction::Continue => {}
            peers::PeersAction::Refresh => self.refresh_peers(),
        }
    }

    fn handle_comms_action(&mut self, action: comms::CommsAction) {
        match action {
            comms::CommsAction::Continue => {}
            comms::CommsAction::Refresh => self.refresh_comms(),
            comms::CommsAction::SendMessage { from, to, msg } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_comms_send(backend, from, to, msg, self.event_tx.clone());
                }
            }
            comms::CommsAction::PostTask {
                title,
                desc,
                assign,
            } => {
                if let Some(backend) = self.backend.to_ref() {
                    event::spawn_comms_task(backend, title, desc, assign, self.event_tx.clone());
                }
            }
        }
    }

    fn handle_logs_action(&mut self, action: logs::LogsAction) {
        match action {
            logs::LogsAction::Continue => {}
            logs::LogsAction::Refresh => self.refresh_logs(),
        }
    }

    // ─── Chat helpers ────────────────────────────────────────────────────────

    fn enter_chat_daemon(&mut self, id: String, name: String) {
        self.chat.reset();
        self.chat.agent_name = name.clone();
        self.chat.mode_label = "daemon".to_string();

        if let Backend::Daemon { ref base_url } = self.backend {
            let client = crate::daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/agents/{id}")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let provider = body["model_provider"].as_str().unwrap_or("?");
                    let model = body["model_name"].as_str().unwrap_or("?");
                    self.chat.model_label = format!("{provider}/{model}");
                }
            }
        }

        self.chat_target = Some(ChatTarget {
            agent_id_daemon: Some(id),
            agent_id_inprocess: None,
            agent_name: name,
        });
        self.chat.push_message(
            chat::Role::System,
            "/help for commands \u{2022} /exit to quit".to_string(),
        );
        self.active_tab = Tab::Chat;
    }

    fn enter_chat_inprocess(&mut self, id: AgentId, name: String) {
        self.chat.reset();
        self.chat.agent_name = name.clone();
        self.chat.mode_label = "in-process".to_string();

        if let Backend::InProcess { ref kernel } = self.backend {
            if let Some(entry) = kernel.registry.get(id) {
                self.chat.model_label = format!(
                    "{}/{}",
                    entry.manifest.model.provider, entry.manifest.model.model
                );
            }
        }

        self.chat_target = Some(ChatTarget {
            agent_id_daemon: None,
            agent_id_inprocess: Some(id),
            agent_name: name,
        });
        self.chat.push_message(
            chat::Role::System,
            "/help for commands \u{2022} /exit to quit".to_string(),
        );
        self.active_tab = Tab::Chat;
    }

    fn send_message(&mut self, message: String) {
        self.chat.is_streaming = true;
        self.chat.thinking = true;
        self.chat.streaming_chars = 0;
        self.chat.last_tokens = None;
        self.chat.status_msg = None;

        match (&self.backend, &self.chat_target) {
            (Backend::Daemon { base_url }, Some(target)) if target.agent_id_daemon.is_some() => {
                event::spawn_daemon_stream(
                    base_url.clone(),
                    target.agent_id_daemon.as_ref().unwrap().clone(),
                    message,
                    self.event_tx.clone(),
                );
            }
            (Backend::InProcess { kernel }, Some(target))
                if target.agent_id_inprocess.is_some() =>
            {
                event::spawn_inprocess_stream(
                    kernel.clone(),
                    target.agent_id_inprocess.unwrap(),
                    message,
                    self.event_tx.clone(),
                );
            }
            _ => {
                self.chat.is_streaming = false;
                self.chat.status_msg = Some("No active connection".to_string());
            }
        }
    }

    fn spawn_agent(&mut self, toml_content: String) {
        match &self.backend {
            Backend::Daemon { base_url } => {
                self.agents.sub = agents::AgentSubScreen::Spawning;
                event::spawn_daemon_agent(base_url.clone(), toml_content, self.event_tx.clone());
            }
            Backend::InProcess { kernel } => {
                let manifest: openfang_types::agent::AgentManifest =
                    match toml::from_str(&toml_content) {
                        Ok(m) => m,
                        Err(e) => {
                            self.agents.status_msg = format!("Invalid manifest: {e}");
                            self.agents.sub = agents::AgentSubScreen::AgentList;
                            return;
                        }
                    };
                let name = manifest.name.clone();
                match kernel.spawn_agent(manifest) {
                    Ok(id) => self.enter_chat_inprocess(id, name),
                    Err(e) => {
                        self.agents.status_msg = format!("Spawn failed: {e}");
                        self.agents.sub = agents::AgentSubScreen::AgentList;
                    }
                }
            }
            Backend::None => {
                self.agents.status_msg = "No backend connected".to_string();
                self.agents.sub = agents::AgentSubScreen::AgentList;
            }
        }
    }

    // ─── Slash commands ──────────────────────────────────────────────────────

    fn handle_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/exit" | "/quit" => self.handle_chat_action(chat::ChatAction::Back),
            "/help" => {
                self.chat.push_message(
                    chat::Role::System,
                    [
                        "/help    \u{2014} show this help",
                        "/status  \u{2014} connection & agent info",
                        "/agents  \u{2014} list running agents",
                        "/model   \u{2014} show current model",
                        "/clear   \u{2014} clear chat history",
                        "/kill    \u{2014} kill the current agent",
                        "/exit    \u{2014} end chat session",
                    ]
                    .join("\n"),
                );
            }
            "/status" => {
                let mut s = Vec::new();
                match &self.backend {
                    Backend::Daemon { base_url } => {
                        s.push(format!("Mode: daemon ({base_url})"));
                        if let Some(ref t) = self.chat_target {
                            s.push(format!("Agent: {}", t.agent_name));
                        }
                    }
                    Backend::InProcess { kernel } => {
                        s.push("Mode: in-process".to_string());
                        s.push(format!("Agents: {}", kernel.registry.count()));
                        if let Some(ref t) = self.chat_target {
                            s.push(format!("Agent: {}", t.agent_name));
                        }
                    }
                    Backend::None => s.push("Mode: disconnected".to_string()),
                }
                self.chat.push_message(chat::Role::System, s.join("\n"));
            }
            "/agents" => {
                let mut lines = Vec::new();
                match &self.backend {
                    Backend::Daemon { base_url } => {
                        let client = crate::daemon_client();
                        if let Ok(resp) = client.get(format!("{base_url}/api/agents")).send() {
                            if let Ok(body) = resp.json::<serde_json::Value>() {
                                if let Some(arr) = body.as_array() {
                                    for a in arr {
                                        lines.push(format!(
                                            "{} [{}] {}",
                                            a["name"].as_str().unwrap_or("?"),
                                            a["state"].as_str().unwrap_or("?"),
                                            a["model_name"].as_str().unwrap_or("?"),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Backend::InProcess { kernel } => {
                        for e in kernel.registry.list() {
                            lines.push(format!(
                                "{} [{:?}] {}/{}",
                                e.name, e.state, e.manifest.model.provider, e.manifest.model.model,
                            ));
                        }
                    }
                    Backend::None => {}
                }
                let msg = if lines.is_empty() {
                    "No agents running.".to_string()
                } else {
                    lines.join("\n")
                };
                self.chat.push_message(chat::Role::System, msg);
            }
            "/clear" => {
                let name = self.chat.agent_name.clone();
                let model = self.chat.model_label.clone();
                let mode = self.chat.mode_label.clone();
                self.chat.reset();
                self.chat.agent_name = name;
                self.chat.model_label = model;
                self.chat.mode_label = mode;
                self.chat
                    .push_message(chat::Role::System, "Chat history cleared.".to_string());
            }
            "/kill" => {
                if let Some(ref target) = self.chat_target {
                    let name = target.agent_name.clone();
                    match &self.backend {
                        Backend::Daemon { base_url } => {
                            if let Some(ref id) = target.agent_id_daemon {
                                let client = crate::daemon_client();
                                let url = format!("{base_url}/api/agents/{id}");
                                match client.delete(&url).send() {
                                    Ok(r) if r.status().is_success() => {
                                        self.chat.push_message(
                                            chat::Role::System,
                                            format!("Agent \"{name}\" killed."),
                                        );
                                    }
                                    _ => {
                                        self.chat.push_message(
                                            chat::Role::System,
                                            format!("Failed to kill agent \"{name}\"."),
                                        );
                                    }
                                }
                            }
                        }
                        Backend::InProcess { kernel } => {
                            if let Some(id) = target.agent_id_inprocess {
                                match kernel.kill_agent(id) {
                                    Ok(()) => {
                                        self.chat.push_message(
                                            chat::Role::System,
                                            format!("Agent \"{name}\" killed."),
                                        );
                                    }
                                    Err(e) => {
                                        self.chat.push_message(
                                            chat::Role::System,
                                            format!("Kill failed: {e}"),
                                        );
                                    }
                                }
                            }
                        }
                        Backend::None => {
                            self.chat.push_message(
                                chat::Role::System,
                                "No backend connected.".to_string(),
                            );
                        }
                    }
                }
            }
            "/model" => {
                self.chat.push_message(
                    chat::Role::System,
                    format!("Model: {}", self.chat.model_label),
                );
            }
            "/hands" => match &self.backend {
                Backend::InProcess { kernel } => {
                    let defs = kernel.hand_registry.list_definitions();
                    let instances = kernel.hand_registry.list_instances();
                    let mut msg = format!("Available hands ({}):\n", defs.len());
                    for d in &defs {
                        let reqs_met = kernel
                            .hand_registry
                            .check_requirements(&d.id)
                            .map(|r| r.iter().all(|(_, ok)| *ok))
                            .unwrap_or(false);
                        let badge = if reqs_met { "Ready" } else { "Setup" };
                        msg.push_str(&format!(
                            "  {} {} — {} [{}]\n",
                            d.icon, d.name, d.description, badge
                        ));
                    }
                    if !instances.is_empty() {
                        msg.push_str(&format!("\nActive hands ({}):\n", instances.len()));
                        for i in &instances {
                            msg.push_str(&format!(
                                "  {} — {} ({})\n",
                                i.agent_name, i.hand_id, i.status
                            ));
                        }
                    }
                    self.chat.push_message(chat::Role::System, msg);
                }
                _ => {
                    self.chat.push_message(
                        chat::Role::System,
                        "Hands info requires in-process mode. Use the Hands tab instead."
                            .to_string(),
                    );
                }
            },
            _ => {
                self.chat.push_message(
                    chat::Role::System,
                    format!("Unknown command: {}. Type /help", parts[0]),
                );
            }
        }
    }

    // ─── Drawing ─────────────────────────────────────────────────────────────

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        match self.phase {
            Phase::Boot(BootScreen::Welcome) => {
                welcome::draw(frame, area, &mut self.welcome);

                // Overlay boot status on top of the welcome card
                if self.kernel_booting {
                    let spinner =
                        theme::SPINNER_FRAMES[self.welcome.tick % theme::SPINNER_FRAMES.len()];
                    let msg = format!(" {spinner} Booting kernel\u{2026}");
                    render_toast(frame, area, &msg, theme::YELLOW);
                }
                if let Some(ref err) = self.kernel_boot_error {
                    let msg = format!(" \u{2718} {err}");
                    render_toast(frame, area, &msg, theme::RED);
                }
            }
            Phase::Boot(BootScreen::Wizard) => wizard::draw(frame, area, &mut self.wizard),
            Phase::Main => {
                // Split: tab bar (1 line) + content
                let chunks = Layout::vertical([
                    Constraint::Length(1), // tab bar
                    Constraint::Min(1),    // content
                ])
                .split(area);

                self.draw_tab_bar(frame, chunks[0]);

                match self.active_tab {
                    Tab::Dashboard => dashboard::draw(frame, chunks[1], &mut self.dashboard),
                    Tab::Agents => agents::draw(frame, chunks[1], &mut self.agents),
                    Tab::Chat => chat::draw(frame, chunks[1], &mut self.chat),
                    Tab::Channels => channels::draw(frame, chunks[1], &mut self.channels),
                    Tab::Workflows => workflows::draw(frame, chunks[1], &mut self.workflows),
                    Tab::Triggers => triggers::draw(frame, chunks[1], &mut self.triggers),
                    Tab::Sessions => sessions::draw(frame, chunks[1], &mut self.sessions),
                    Tab::Memory => memory::draw(frame, chunks[1], &mut self.memory),
                    Tab::Skills => skills::draw(frame, chunks[1], &mut self.skills),
                    Tab::Hands => hands::draw(frame, chunks[1], &mut self.hands),
                    Tab::Extensions => extensions::draw(frame, chunks[1], &mut self.extensions),
                    Tab::Templates => templates::draw(frame, chunks[1], &mut self.templates),
                    Tab::Security => security::draw(frame, chunks[1], &mut self.security),
                    Tab::Audit => audit::draw(frame, chunks[1], &mut self.audit),
                    Tab::Usage => usage::draw(frame, chunks[1], &mut self.usage),
                    Tab::Settings => settings::draw(frame, chunks[1], &mut self.settings),
                    Tab::Peers => peers::draw(frame, chunks[1], &mut self.peers),
                    Tab::Comms => comms::draw(frame, chunks[1], &mut self.comms),
                    Tab::Logs => logs::draw(frame, chunks[1], &mut self.logs),
                }
            }
        }
    }

    fn draw_tab_bar(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let width = area.width as usize;

        // Compute all tab labels with their widths
        let tab_labels: Vec<(usize, String)> = TABS
            .iter()
            .map(|tab| {
                let label = format!(" {} ", tab.label());
                let w = label.len() + 1; // +1 for spacing
                (w, label)
            })
            .collect();

        // Reserve space for overflow indicators (2 chars each) and hint
        let indicator_width = 2; // "< " or " >"
        let hint = if self.ctrl_c_pending {
            "Press Ctrl+C again to quit"
        } else {
            "Ctrl+C×2 quit  Tab/Ctrl+\u{2190}\u{2192} switch"
        };
        let hint_width = hint.len() + 2;
        let available = width.saturating_sub(hint_width + 2);

        // Ensure active tab is visible by adjusting scroll offset
        let active_idx = self.active_tab.index();

        // Scroll so active tab fits in the visible window
        if active_idx < self.tab_scroll_offset {
            self.tab_scroll_offset = active_idx;
        }

        // Find how many tabs fit starting from scroll offset
        loop {
            let mut used = if self.tab_scroll_offset > 0 {
                indicator_width
            } else {
                1
            }; // leading space or left indicator
            let mut last_visible = self.tab_scroll_offset;
            for (i, (tab_w, _)) in tab_labels.iter().enumerate().skip(self.tab_scroll_offset) {
                if used + tab_w > available {
                    break;
                }
                used += tab_w;
                last_visible = i;
            }
            if active_idx <= last_visible || self.tab_scroll_offset >= TABS.len() - 1 {
                break;
            }
            self.tab_scroll_offset += 1;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Left overflow indicator
        if self.tab_scroll_offset > 0 {
            spans.push(Span::styled(
                "\u{25c0} ",
                Style::default().fg(theme::TEXT_TERTIARY),
            ));
        } else {
            spans.push(Span::raw(" "));
        }

        // Render visible tabs
        let mut used = if self.tab_scroll_offset > 0 {
            indicator_width
        } else {
            1
        };
        let mut last_rendered = self.tab_scroll_offset;
        for (i, ((tab_w, label), &tab)) in tab_labels
            .iter()
            .zip(TABS.iter())
            .enumerate()
            .skip(self.tab_scroll_offset)
        {
            if used + tab_w > available {
                break;
            }
            if tab == self.active_tab {
                spans.push(Span::styled(label.clone(), theme::tab_active()));
            } else {
                spans.push(Span::styled(label.clone(), theme::tab_inactive()));
            }
            spans.push(Span::raw(" "));
            used += tab_w;
            last_rendered = i;
        }

        // Right overflow indicator
        if last_rendered < TABS.len() - 1 {
            spans.push(Span::styled(
                " \u{25b6}",
                Style::default().fg(theme::TEXT_TERTIARY),
            ));
        }

        // Right-aligned hint (yellow warning when Ctrl+C pending)
        let hint_style = if self.ctrl_c_pending {
            Style::default()
                .fg(theme::YELLOW)
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            theme::hint_style()
        };
        let spans_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let padding = width.saturating_sub(spans_width + hint.len());
        if padding > 0 {
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(Span::styled(hint, hint_style));
        }

        let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_CARD));
        frame.render_widget(bar, area);
    }
}

/// Draw a one-line toast at the bottom of the screen.
fn render_toast(frame: &mut ratatui::Frame, area: Rect, msg: &str, color: ratatui::style::Color) {
    let w = (msg.len() as u16 + 4).min(area.width);
    let x = area.width.saturating_sub(w) / 2;
    let y = area.height.saturating_sub(2);
    let toast_area = Rect::new(x, y, w, 1);
    let para = Paragraph::new(Line::from(vec![Span::styled(
        msg,
        Style::default().fg(color),
    )]));
    frame.render_widget(para, toast_area);
}

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Entry point for the TUI interactive mode.
pub fn run(config: Option<PathBuf>) {
    // Panic hook: always restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        original_hook(info);
    }));

    let mut terminal = ratatui::init();

    // 50ms tick → 20fps spinner animation, snappy key response
    let (tx, rx) = event::spawn_event_thread(Duration::from_millis(50));
    let mut app = App::new(config, tx);

    // Initial screen
    if wizard::needs_setup() {
        app.wizard.reset();
        app.phase = Phase::Boot(BootScreen::Wizard);
    } else {
        app.phase = Phase::Boot(BootScreen::Welcome);
        // Non-blocking daemon detection
        app.start_daemon_detect();
    }

    // ── Main loop ────────────────────────────────────────────────────────────
    // Draw first, then block on events. This ensures the first frame appears
    // immediately, before any event processing.
    while !app.should_quit {
        terminal
            .draw(|frame| app.draw(frame))
            .expect("Failed to draw");

        // Block until at least one event arrives (or 33ms timeout for ~30fps)
        match rx.recv_timeout(Duration::from_millis(33)) {
            Ok(ev) => app.handle_event(ev),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Drain all queued events immediately (batch processing)
        while let Ok(ev) = rx.try_recv() {
            app.handle_event(ev);
        }
    }

    ratatui::restore();
}
