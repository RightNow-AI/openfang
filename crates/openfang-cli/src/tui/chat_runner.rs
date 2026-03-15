//! Standalone chat TUI for `openfang chat`.
//!
//! Launches a focused ratatui chat screen — same beautiful rendering as the
//! full TUI's Chat tab, but without the 17-tab chrome. Reuses 100% of
//! `ChatState`, `chat::draw()`, event spawning, and the theme system.

use super::event::{self, AppEvent};
use super::screens::chat::{self, ChatAction, ChatState, Role};
use super::theme;
use openfang_kernel::OpenFangKernel;
use openfang_runtime::llm_driver::StreamEvent;
use openfang_types::agent::AgentId;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

// ── Internal state ───────────────────────────────────────────────────────────

enum Backend {
    Daemon { base_url: String },
    InProcess { kernel: Arc<OpenFangKernel> },
    None,
}

struct StandaloneChat {
    chat: ChatState,
    event_tx: mpsc::Sender<AppEvent>,
    backend: Backend,
    agent_id_daemon: Option<String>,
    agent_id_inprocess: Option<AgentId>,
    agent_name: String,
    should_quit: bool,
    booting: bool,
    boot_error: Option<String>,
    spinner_frame: usize,
}

impl StandaloneChat {
    fn new(event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self {
            chat: ChatState::new(),
            event_tx,
            backend: Backend::None,
            agent_id_daemon: None,
            agent_id_inprocess: None,
            agent_name: String::new(),
            should_quit: false,
            booting: false,
            boot_error: None,
            spinner_frame: 0,
        }
    }

    // ── Event dispatch ───────────────────────────────────────────────────────

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
            // All other events (tab-specific data loads) are irrelevant in
            // standalone chat mode — silently ignore.
            _ => {}
        }
    }

    fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        // Ctrl+Q / Ctrl+C always quit
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    self.should_quit = true;
                    return;
                }
                _ => {}
            }
        }

        // If still booting, only allow quit keys
        if self.booting || self.backend_is_none() {
            if key.code == KeyCode::Esc {
                self.should_quit = true;
            }
            return;
        }

        let action = self.chat.handle_key(key);
        self.handle_chat_action(action);
    }

    fn handle_tick(&mut self) {
        self.chat.tick();
        if self.booting {
            self.spinner_frame = (self.spinner_frame + 1) % theme::SPINNER_FRAMES.len();
        }
    }

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
                    self.chat.push_message(Role::Agent, text);
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
                if !r.response.is_empty()
                    && self.chat.messages.last().map(|m| m.text.as_str()) != Some(&r.response)
                {
                    self.chat.push_message(Role::Agent, r.response);
                }
                if r.total_usage.input_tokens > 0 || r.total_usage.output_tokens > 0 {
                    self.chat.last_tokens =
                        Some((r.total_usage.input_tokens, r.total_usage.output_tokens));
                }
                self.chat.last_cost_usd = r.cost_usd;
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

    // ── Kernel lifecycle ─────────────────────────────────────────────────────

    fn handle_kernel_ready(&mut self, kernel: Arc<OpenFangKernel>) {
        self.booting = false;
        self.boot_error = None;
        self.backend = Backend::InProcess { kernel };
        // Spawn or find the agent
        self.resolve_inprocess_agent();
    }

    fn handle_kernel_error(&mut self, err: String) {
        self.booting = false;
        self.boot_error = Some(err);
    }

    fn handle_agent_spawned(&mut self, id: String, name: String) {
        self.enter_chat_daemon(id, name);
    }

    fn handle_agent_spawn_error(&mut self, err: String) {
        self.chat.status_msg = Some(format!("Failed to spawn agent: {err}"));
    }

    // ── Chat action dispatch ─────────────────────────────────────────────────

    fn handle_chat_action(&mut self, action: ChatAction) {
        match action {
            ChatAction::Continue => {}
            ChatAction::Back => {
                self.should_quit = true;
            }
            ChatAction::SendMessage(msg) => self.send_message(msg),
            ChatAction::SlashCommand(cmd) => self.handle_slash_command(&cmd),
            ChatAction::OpenModelPicker => self.open_model_picker(),
            ChatAction::SwitchModel(model_id) => self.switch_model(&model_id),
        }
    }

    fn send_message(&mut self, message: String) {
        self.chat.is_streaming = true;
        self.chat.thinking = true;
        self.chat.streaming_chars = 0;
        self.chat.last_tokens = None;
        self.chat.last_cost_usd = None;
        self.chat.status_msg = None;

        match &self.backend {
            Backend::Daemon { base_url } if self.agent_id_daemon.is_some() => {
                event::spawn_daemon_stream(
                    base_url.clone(),
                    self.agent_id_daemon.as_ref().unwrap().clone(),
                    message,
                    self.event_tx.clone(),
                );
            }
            Backend::InProcess { kernel } if self.agent_id_inprocess.is_some() => {
                event::spawn_inprocess_stream(
                    kernel.clone(),
                    self.agent_id_inprocess.unwrap(),
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

    // ── Slash commands (subset — no tab navigation) ──────────────────────────

    fn handle_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/exit" | "/quit" => {
                self.should_quit = true;
            }
            "/help" => {
                self.chat.push_message(
                    Role::System,
                    [
                        "/help         \u{2014} show this help",
                        "/model        \u{2014} open model picker (Ctrl+M)",
                        "/model <name> \u{2014} switch to model directly",
                        "/status       \u{2014} connection & agent info",
                        "/clear        \u{2014} clear chat history",
                        "/kill         \u{2014} kill the current agent & quit",
                        "/exit         \u{2014} end chat session",
                    ]
                    .join("\n"),
                );
            }
            "/status" => {
                let mut s = Vec::new();
                match &self.backend {
                    Backend::Daemon { base_url } => {
                        s.push(format!("Mode: daemon ({base_url})"));
                        s.push(format!("Agent: {}", self.agent_name));
                    }
                    Backend::InProcess { kernel } => {
                        s.push("Mode: in-process".to_string());
                        s.push(format!("Agents: {}", kernel.registry.count()));
                        s.push(format!("Agent: {}", self.agent_name));
                    }
                    Backend::None => s.push("Mode: disconnected".to_string()),
                }
                self.chat.push_message(Role::System, s.join("\n"));
            }
            "/model" => {
                let args = parts.get(1).map(|s| s.trim()).unwrap_or("");
                if args.is_empty() {
                    // No argument: open the model picker
                    self.open_model_picker();
                } else {
                    // With argument: switch directly
                    self.switch_model(args);
                }
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
                    .push_message(Role::System, "Chat history cleared.".to_string());
            }
            "/kill" => {
                let name = self.agent_name.clone();
                match &self.backend {
                    Backend::Daemon { base_url } => {
                        if let Some(ref id) = self.agent_id_daemon {
                            let client = crate::daemon_client();
                            let url = format!("{base_url}/api/agents/{id}");
                            match client.delete(&url).send() {
                                Ok(r) if r.status().is_success() => {
                                    self.chat.push_message(
                                        Role::System,
                                        format!("Agent \"{name}\" killed."),
                                    );
                                    self.should_quit = true;
                                }
                                _ => {
                                    self.chat.push_message(
                                        Role::System,
                                        format!("Failed to kill agent \"{name}\"."),
                                    );
                                }
                            }
                        }
                    }
                    Backend::InProcess { kernel } => {
                        if let Some(id) = self.agent_id_inprocess {
                            match kernel.kill_agent(id) {
                                Ok(()) => {
                                    self.chat.push_message(
                                        Role::System,
                                        format!("Agent \"{name}\" killed."),
                                    );
                                    self.should_quit = true;
                                }
                                Err(e) => {
                                    self.chat
                                        .push_message(Role::System, format!("Kill failed: {e}"));
                                }
                            }
                        }
                    }
                    Backend::None => {
                        self.chat
                            .push_message(Role::System, "No backend connected.".to_string());
                    }
                }
            }
            _ => {
                self.chat.push_message(
                    Role::System,
                    format!("Unknown command: {}. Type /help", parts[0]),
                );
            }
        }
    }

    // ── Model picker helpers ──────────────────────────────────────────────────

    fn open_model_picker(&mut self) {
        use super::screens::chat::ModelEntry;

        let models = match &self.backend {
            Backend::Daemon { base_url } => {
                let client = crate::daemon_client();
                match client.get(format!("{base_url}/api/models")).send() {
                    Ok(resp) => match resp.json::<serde_json::Value>() {
                        Ok(body) => body["models"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter(|m| m["available"].as_bool().unwrap_or(false))
                                    .map(|m| ModelEntry {
                                        id: m["id"].as_str().unwrap_or("").to_string(),
                                        display_name: m["display_name"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        provider: m["provider"].as_str().unwrap_or("").to_string(),
                                        tier: m["tier"].as_str().unwrap_or("Balanced").to_string(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        Err(_) => Vec::new(),
                    },
                    Err(_) => Vec::new(),
                }
            }
            Backend::InProcess { kernel } => {
                let catalog = kernel.model_catalog.read().unwrap();
                catalog
                    .available_models()
                    .into_iter()
                    .map(|e| ModelEntry {
                        id: e.id.clone(),
                        display_name: e.display_name.clone(),
                        provider: e.provider.clone(),
                        tier: format!("{:?}", e.tier),
                    })
                    .collect()
            }
            Backend::None => Vec::new(),
        };

        if models.is_empty() {
            self.chat
                .push_message(Role::System, "No models available.".to_string());
            return;
        }

        self.chat.model_picker_models = models;
        self.chat.model_picker_filter.clear();
        self.chat.model_picker_idx = 0;
        self.chat.show_model_picker = true;
    }

    fn switch_model(&mut self, model_id: &str) {
        // Skip if already on this model
        if self.chat.model_label.ends_with(model_id) {
            return;
        }

        match &self.backend {
            Backend::Daemon { base_url } => {
                if let Some(ref agent_id) = self.agent_id_daemon {
                    let client = crate::daemon_client();
                    let url = format!("{base_url}/api/agents/{agent_id}/model");
                    match client
                        .put(&url)
                        .json(&serde_json::json!({"model": model_id}))
                        .send()
                    {
                        Ok(r) if r.status().is_success() => {
                            // Re-fetch agent to get updated provider/model
                            if let Ok(resp) = client
                                .get(format!("{base_url}/api/agents/{agent_id}"))
                                .send()
                            {
                                if let Ok(body) = resp.json::<serde_json::Value>() {
                                    let provider = body["model_provider"].as_str().unwrap_or("?");
                                    let model = body["model_name"].as_str().unwrap_or("?");
                                    self.chat.model_label = format!("{provider}/{model}");
                                }
                            }
                            self.chat
                                .push_message(Role::System, format!("Switched to {model_id}"));
                        }
                        _ => {
                            self.chat.push_message(
                                Role::System,
                                format!("Failed to switch to {model_id}"),
                            );
                        }
                    }
                }
            }
            Backend::InProcess { kernel } => {
                if let Some(id) = self.agent_id_inprocess {
                    let provider = kernel
                        .model_catalog
                        .read()
                        .unwrap()
                        .find_model(model_id)
                        .map(|e| e.provider.clone());
                    let result = if let Some(ref prov) = provider {
                        kernel.registry.update_model_and_provider(
                            id,
                            model_id.to_string(),
                            prov.clone(),
                        )
                    } else {
                        kernel.registry.update_model(id, model_id.to_string())
                    };
                    match result {
                        Ok(()) => {
                            let prov_label = provider.unwrap_or_else(|| {
                                kernel
                                    .registry
                                    .get(id)
                                    .map(|e| e.manifest.model.provider.clone())
                                    .unwrap_or_else(|| "?".to_string())
                            });
                            self.chat.model_label = format!("{prov_label}/{model_id}");
                            self.chat
                                .push_message(Role::System, format!("Switched to {model_id}"));
                        }
                        Err(e) => {
                            self.chat
                                .push_message(Role::System, format!("Switch failed: {e}"));
                        }
                    }
                }
            }
            Backend::None => {
                self.chat
                    .push_message(Role::System, "No backend connected.".to_string());
            }
        }
    }

    // ── Agent resolution helpers ─────────────────────────────────────────────

    fn enter_chat_daemon(&mut self, id: String, name: String) {
        self.agent_id_daemon = Some(id.clone());
        self.agent_name = name.clone();
        self.chat.agent_name = name;
        self.chat.mode_label = "daemon".to_string();

        // Fetch model info
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

        self.chat.push_message(
            Role::System,
            "/help for commands \u{2022} /exit to quit".to_string(),
        );
    }

    fn enter_chat_inprocess(&mut self, id: AgentId, name: String) {
        self.agent_id_inprocess = Some(id);
        self.agent_name = name.clone();
        self.chat.agent_name = name;
        self.chat.mode_label = "in-process".to_string();

        if let Backend::InProcess { ref kernel } = self.backend {
            if let Some(entry) = kernel.registry.get(id) {
                self.chat.model_label = format!(
                    "{}/{}",
                    entry.manifest.model.provider, entry.manifest.model.model
                );
            }
        }

        self.chat.push_message(
            Role::System,
            "/help for commands \u{2022} /exit to quit".to_string(),
        );
    }

    /// Resolve agent on daemon: find by name/id, or auto-spawn from template.
    fn resolve_daemon_agent(&mut self, base_url: &str, agent_name: Option<&str>) {
        let client = crate::daemon_client();
        let body = crate::daemon_json(client.get(format!("{base_url}/api/agents")).send());
        let agents = body.as_array();

        // Try to find by name/id
        let found = match agent_name {
            Some(name_or_id) => agents.and_then(|arr| {
                arr.iter().find(|a| {
                    a["name"].as_str() == Some(name_or_id) || a["id"].as_str() == Some(name_or_id)
                })
            }),
            None => agents.and_then(|arr| arr.first()),
        };

        if let Some(agent) = found {
            let id = agent["id"].as_str().unwrap_or("").to_string();
            let name = agent["name"].as_str().unwrap_or("agent").to_string();
            self.backend = Backend::Daemon {
                base_url: base_url.to_string(),
            };
            self.enter_chat_daemon(id, name);
            return;
        }

        // Auto-spawn from template
        let target_name = agent_name.unwrap_or("assistant");
        let all_templates = crate::templates::load_all_templates();
        let template = all_templates
            .iter()
            .find(|t| t.name == target_name)
            .or_else(|| all_templates.first());

        match template {
            Some(t) => {
                self.backend = Backend::Daemon {
                    base_url: base_url.to_string(),
                };
                event::spawn_daemon_agent(
                    base_url.to_string(),
                    t.content.clone(),
                    self.event_tx.clone(),
                );
                self.chat.status_msg = Some(format!("Spawning '{}' agent\u{2026}", t.name));
            }
            None => {
                self.boot_error =
                    Some("No agent templates found. Run `openfang init`.".to_string());
            }
        }
    }

    /// Resolve agent in-process: find existing or spawn from template.
    fn resolve_inprocess_agent(&mut self) {
        let kernel = match &self.backend {
            Backend::InProcess { kernel } => kernel.clone(),
            _ => return,
        };

        // Check for existing agents
        let existing = kernel.registry.list();
        if let Some(entry) = existing
            .iter()
            .find(|e| self.agent_name.is_empty() || e.name == self.agent_name)
        {
            self.enter_chat_inprocess(entry.id, entry.name.clone());
            return;
        }

        // Spawn from template
        let target_name = if self.agent_name.is_empty() {
            "assistant"
        } else {
            &self.agent_name
        };
        let all_templates = crate::templates::load_all_templates();
        let template = all_templates
            .iter()
            .find(|t| t.name == target_name)
            .or_else(|| all_templates.iter().find(|t| t.name == "assistant"))
            .or_else(|| all_templates.first());

        match template {
            Some(t) => {
                let manifest: openfang_types::agent::AgentManifest =
                    match toml::from_str(&t.content) {
                        Ok(m) => m,
                        Err(e) => {
                            self.chat.status_msg =
                                Some(format!("Invalid template '{}': {e}", t.name));
                            return;
                        }
                    };
                let name = manifest.name.clone();
                match kernel.spawn_agent(manifest) {
                    Ok(id) => {
                        self.enter_chat_inprocess(id, name);
                    }
                    Err(e) => {
                        self.chat.status_msg = Some(format!("Spawn failed: {e}"));
                    }
                }
            }
            None => {
                self.chat.status_msg =
                    Some("No agent templates found. Run `openfang init`.".to_string());
            }
        }
    }

    fn backend_is_none(&self) -> bool {
        matches!(self.backend, Backend::None)
    }

    // ── Drawing ──────────────────────────────────────────────────────────────

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        if self.booting {
            self.draw_booting(frame, area);
        } else if let Some(ref err) = self.boot_error {
            self.draw_error(frame, area, err);
        } else {
            chat::draw(frame, area, &mut self.chat);
        }
    }

    fn draw_booting(&self, frame: &mut ratatui::Frame, area: Rect) {
        let spinner = theme::SPINNER_FRAMES[self.spinner_frame];

        let chunks = Layout::vertical([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

        let lines = vec![
            Line::from(vec![
                Span::styled(format!(" {spinner} "), Style::default().fg(theme::ACCENT)),
                Span::styled(
                    "Booting kernel\u{2026}",
                    Style::default().fg(theme::TEXT_PRIMARY),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  This may take a moment while the kernel initializes.",
                theme::dim_style(),
            )]),
        ];

        let para = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(para, chunks[1]);
    }

    fn draw_error(&self, frame: &mut ratatui::Frame, area: Rect, err: &str) {
        let chunks = Layout::vertical([
            Constraint::Percentage(35),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

        let lines = vec![
            Line::from(vec![
                Span::styled(" \u{2718} ", Style::default().fg(theme::RED)),
                Span::styled("Failed to start", Style::default().fg(theme::RED)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                format!("  {err}"),
                Style::default().fg(theme::TEXT_SECONDARY),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Press Esc to exit.",
                theme::hint_style(),
            )]),
        ];

        let para = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(para, chunks[1]);
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Launch the standalone chat TUI.
///
/// - If a daemon is running, connects to it and resolves the agent.
/// - Otherwise, boots the kernel in-process.
pub fn run_chat_tui(config: Option<PathBuf>, agent_name: Option<String>) {
    // Panic hook: always restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        original_hook(info);
    }));

    let mut terminal = ratatui::init();

    let (tx, rx) = event::spawn_event_thread(Duration::from_millis(50));
    let mut state = StandaloneChat::new(tx.clone());

    // Store the requested agent name for later resolution
    if let Some(ref name) = agent_name {
        state.agent_name = name.clone();
    }

    // Boot sequence: check for daemon, or boot kernel in-process
    if let Some(base_url) = crate::find_daemon() {
        state.resolve_daemon_agent(&base_url, agent_name.as_deref());
    } else {
        state.booting = true;
        event::spawn_kernel_boot(config, tx);
    }

    // ── Main loop ────────────────────────────────────────────────────────────
    while !state.should_quit {
        terminal
            .draw(|frame| state.draw(frame))
            .expect("Failed to draw");

        match rx.recv_timeout(Duration::from_millis(33)) {
            Ok(ev) => state.handle_event(ev),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Drain queued events
        while let Ok(ev) = rx.try_recv() {
            state.handle_event(ev);
        }
    }

    ratatui::restore();
}
