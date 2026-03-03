//! Settings screen: provider key management, model catalog, tools list.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct ProviderInfo {
    pub name: String,
    pub configured: bool,
    pub env_var: String,
    /// Whether this is a local provider (ollama, vllm, lmstudio).
    pub is_local: bool,
    /// Whether the local provider is reachable (only set for local providers).
    pub reachable: Option<bool>,
    /// Probe latency in milliseconds (only set for local providers).
    pub latency_ms: Option<u64>,
}

#[derive(Clone, Default)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub tier: String,
    pub context_window: u64,
    pub cost_input: f64,
    pub cost_output: f64,
}

#[derive(Clone, Default)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Clone)]
pub struct TestResult {
    pub provider: String,
    pub success: bool,
    pub latency_ms: u64,
    pub message: String,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsSub {
    Providers,
    Models,
    Tools,
}

pub struct SettingsState {
    pub sub: SettingsSub,
    pub providers: Vec<ProviderInfo>,
    pub models: Vec<ModelInfo>,
    pub tools: Vec<ToolInfo>,
    pub provider_list: ListState,
    pub model_list: ListState,
    pub tool_list: ListState,
    pub input_buf: String,
    pub input_mode: bool,
    pub editing_provider: Option<String>,
    pub test_result: Option<TestResult>,
    pub loading: bool,
    pub tick: usize,
    pub status_msg: String,
}

pub enum SettingsAction {
    Continue,
    RefreshProviders,
    RefreshModels,
    RefreshTools,
    SaveProviderKey { name: String, key: String },
    DeleteProviderKey(String),
    TestProvider(String),
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            sub: SettingsSub::Providers,
            providers: Vec::new(),
            models: Vec::new(),
            tools: Vec::new(),
            provider_list: ListState::default(),
            model_list: ListState::default(),
            tool_list: ListState::default(),
            input_buf: String::new(),
            input_mode: false,
            editing_provider: None,
            test_result: None,
            loading: false,
            tick: 0,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SettingsAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return SettingsAction::Continue;
        }

        if self.input_mode {
            return self.handle_input(key);
        }

        // Sub-tab switching
        if !self.input_mode {
            match key.code {
                KeyCode::Char('1') => {
                    self.sub = SettingsSub::Providers;
                    return SettingsAction::RefreshProviders;
                }
                KeyCode::Char('2') => {
                    self.sub = SettingsSub::Models;
                    return SettingsAction::RefreshModels;
                }
                KeyCode::Char('3') => {
                    self.sub = SettingsSub::Tools;
                    return SettingsAction::RefreshTools;
                }
                _ => {}
            }
        }

        match self.sub {
            SettingsSub::Providers => self.handle_providers(key),
            SettingsSub::Models => self.handle_models(key),
            SettingsSub::Tools => self.handle_tools(key),
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> SettingsAction {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = false;
                self.editing_provider = None;
                self.input_buf.clear();
            }
            KeyCode::Enter => {
                self.input_mode = false;
                if let Some(name) = self.editing_provider.take() {
                    if !self.input_buf.is_empty() {
                        let api_key = self.input_buf.clone();
                        self.input_buf.clear();
                        return SettingsAction::SaveProviderKey { name, key: api_key };
                    }
                }
                self.input_buf.clear();
            }
            KeyCode::Backspace => {
                self.input_buf.pop();
            }
            KeyCode::Char(c) => {
                self.input_buf.push(c);
            }
            _ => {}
        }
        SettingsAction::Continue
    }

    fn handle_providers(&mut self, key: KeyEvent) -> SettingsAction {
        let total = self.providers.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.provider_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.provider_list.select(Some(next));
                    self.test_result = None;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.provider_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.provider_list.select(Some(next));
                    self.test_result = None;
                }
            }
            KeyCode::Char('e') => {
                if let Some(sel) = self.provider_list.selected() {
                    if sel < self.providers.len() {
                        self.editing_provider = Some(self.providers[sel].name.clone());
                        self.input_mode = true;
                        self.input_buf.clear();
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(sel) = self.provider_list.selected() {
                    if sel < self.providers.len() {
                        return SettingsAction::DeleteProviderKey(self.providers[sel].name.clone());
                    }
                }
            }
            KeyCode::Char('t') => {
                if let Some(sel) = self.provider_list.selected() {
                    if sel < self.providers.len() {
                        self.test_result = None;
                        return SettingsAction::TestProvider(self.providers[sel].name.clone());
                    }
                }
            }
            KeyCode::Char('r') => return SettingsAction::RefreshProviders,
            _ => {}
        }
        SettingsAction::Continue
    }

    fn handle_models(&mut self, key: KeyEvent) -> SettingsAction {
        let total = self.models.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.model_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.model_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.model_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.model_list.select(Some(next));
                }
            }
            KeyCode::Char('r') => return SettingsAction::RefreshModels,
            _ => {}
        }
        SettingsAction::Continue
    }

    fn handle_tools(&mut self, key: KeyEvent) -> SettingsAction {
        let total = self.tools.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.tool_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.tool_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.tool_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.tool_list.select(Some(next));
                }
            }
            KeyCode::Char('r') => return SettingsAction::RefreshTools,
            _ => {}
        }
        SettingsAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut SettingsState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Settings ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // sub-tab bar
        Constraint::Length(1), // separator
        Constraint::Min(3),    // content
        Constraint::Length(1), // hints
    ])
    .split(inner);

    draw_sub_tabs(f, chunks[0], state.sub);

    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, theme::dim_style())),
        chunks[1],
    );

    match state.sub {
        SettingsSub::Providers => draw_providers(f, chunks[2], state),
        SettingsSub::Models => draw_models(f, chunks[2], state),
        SettingsSub::Tools => draw_tools(f, chunks[2], state),
    }

    // Hints
    let hint_text = match state.sub {
        SettingsSub::Providers if state.input_mode => "  [Enter] Save  [Esc] Cancel",
        SettingsSub::Providers => {
            "  [\u{2191}\u{2193}] Navigate  [e] Set Key  [d] Delete Key  [t] Test  [r] Refresh"
        }
        SettingsSub::Models => "  [\u{2191}\u{2193}] Navigate  [r] Refresh",
        SettingsSub::Tools => "  [\u{2191}\u{2193}] Navigate  [r] Refresh",
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            hint_text,
            theme::hint_style(),
        )])),
        chunks[3],
    );
}

fn draw_sub_tabs(f: &mut Frame, area: Rect, active: SettingsSub) {
    let tabs = [
        (SettingsSub::Providers, "1 Providers"),
        (SettingsSub::Models, "2 Models"),
        (SettingsSub::Tools, "3 Tools"),
    ];
    let mut spans = vec![Span::raw("  ")];
    for (sub, label) in &tabs {
        let style = if *sub == active {
            theme::tab_active()
        } else {
            theme::tab_inactive()
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_providers(f: &mut Frame, area: Rect, state: &mut SettingsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(2), // input / test result
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {:<20} {:<20} {}", "Provider", "Status", "Env Variable"),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading && state.providers.is_empty() {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading providers\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.providers.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No providers available.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .providers
            .iter()
            .map(|p| {
                let (badge, badge_style) = if p.is_local {
                    match p.reachable {
                        Some(true) => {
                            let ms = p.latency_ms.unwrap_or(0);
                            (
                                format!("\u{2714} Online ({ms}ms)"),
                                Style::default().fg(theme::GREEN),
                            )
                        }
                        Some(false) => (
                            "\u{2718} Offline".to_string(),
                            Style::default().fg(theme::RED),
                        ),
                        None => ("\u{25cb} Local".to_string(), theme::dim_style()),
                    }
                } else if p.configured {
                    (
                        "\u{2714} Configured".to_string(),
                        Style::default().fg(theme::GREEN),
                    )
                } else {
                    ("\u{25cb} Not set".to_string(), theme::dim_style())
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<20}", &p.name),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!(" {:<20}", badge), badge_style),
                    Span::styled(format!(" {}", &p.env_var), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.provider_list);
    }

    // Input mode or test result
    if state.input_mode {
        let provider_name = state.editing_provider.as_deref().unwrap_or("?");
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![Span::styled(
                    format!("  Enter API key for {provider_name}: "),
                    Style::default().fg(theme::YELLOW),
                )]),
                Line::from(vec![
                    Span::raw("  > "),
                    Span::styled(
                        "\u{2022}".repeat(state.input_buf.len().min(40)),
                        theme::input_style(),
                    ),
                    Span::styled(
                        "\u{2588}",
                        Style::default()
                            .fg(theme::GREEN)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ),
                ]),
            ]),
            chunks[2],
        );
    } else if let Some(result) = &state.test_result {
        let (icon, style) = if result.success {
            ("\u{2714}", Style::default().fg(theme::GREEN))
        } else {
            ("\u{2718}", Style::default().fg(theme::RED))
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(format!("  {icon} "), style),
                    Span::styled(format!("{}: {}", result.provider, result.message), style),
                ]),
                Line::from(vec![Span::styled(
                    if result.success {
                        format!("  Latency: {}ms", result.latency_ms)
                    } else {
                        String::new()
                    },
                    theme::dim_style(),
                )]),
            ]),
            chunks[2],
        );
    } else if !state.status_msg.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!("  {}", state.status_msg),
                Style::default().fg(theme::GREEN),
            )])),
            chunks[2],
        );
    }
}

fn draw_models(f: &mut Frame, area: Rect, state: &mut SettingsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<28} {:<14} {:<10} {:<10} {}",
                "Model ID", "Provider", "Tier", "Context", "Cost (in/out per 1M)"
            ),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading && state.models.is_empty() {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading models\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.models.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No models available.", theme::dim_style())),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .models
            .iter()
            .map(|m| {
                let tier_style = match m.tier.as_str() {
                    "Frontier" => Style::default()
                        .fg(theme::PURPLE)
                        .add_modifier(Modifier::BOLD),
                    "Smart" => Style::default()
                        .fg(theme::BLUE)
                        .add_modifier(Modifier::BOLD),
                    "Balanced" => Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                    "Fast" => Style::default()
                        .fg(theme::YELLOW)
                        .add_modifier(Modifier::BOLD),
                    _ => theme::dim_style(),
                };
                let ctx = format_context(m.context_window);
                let cost = format!("${:.2}/${:.2}", m.cost_input, m.cost_output);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<28}", truncate(&m.id, 27)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<14}", truncate(&m.provider, 13)),
                        theme::dim_style(),
                    ),
                    Span::styled(format!(" {:<10}", &m.tier), tier_style),
                    Span::styled(format!(" {:<10}", ctx), Style::default().fg(theme::YELLOW)),
                    Span::styled(format!(" {cost}"), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.model_list);
    }
}

fn draw_tools(f: &mut Frame, area: Rect, state: &mut SettingsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {:<24} {}", "Tool Name", "Description"),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.tools.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No tools available.", theme::dim_style())),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .tools
            .iter()
            .map(|t| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<24}", truncate(&t.name, 23)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {}", truncate(&t.description, 50)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.tool_list);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}

fn format_context(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        format!("{n}")
    }
}
