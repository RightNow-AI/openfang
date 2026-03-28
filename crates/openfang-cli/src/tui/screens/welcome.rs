//! Welcome screen: branded logo, daemon/provider status, mode selection menu.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::theme;

// ── ASCII Logo ───────────────────────────────────────────────────────────────

const LOGO: &str = "\
 \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}
\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}
\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2554}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2588}\u{2551}
\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{255d} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{255d}  \u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{255d}  \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}
\u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2551}     \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}     \u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}
 \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{255d}     \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}\u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{255d}\u{255a}\u{2550}\u{255d}     \u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{255d}\u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}";

const LOGO_HEIGHT: u16 = 6;
/// Minimum terminal width to show the full ASCII logo.
const LOGO_MIN_WIDTH: u16 = 75;

const COMPACT_LOGO: &str = "O P E N F A N G";

// ── Provider detection ───────────────────────────────────────────────────────

/// Known provider env vars, checked in priority order.
const PROVIDER_ENV_VARS: &[(&str, &str)] = &[
    ("ANTHROPIC_API_KEY", "Anthropic"),
    ("OPENAI_API_KEY", "OpenAI"),
    ("DEEPSEEK_API_KEY", "DeepSeek"),
    ("VOLCENGINE_API_KEY", "Volcano Engine"),
    ("GEMINI_API_KEY", "Gemini"),
    ("GOOGLE_API_KEY", "Gemini"),
    ("GROQ_API_KEY", "Groq"),
    ("OPENROUTER_API_KEY", "OpenRouter"),
    ("TOGETHER_API_KEY", "Together"),
    ("MISTRAL_API_KEY", "Mistral"),
    ("FIREWORKS_API_KEY", "Fireworks"),
    ("BRAVE_API_KEY", "Brave Search"),
    ("TAVILY_API_KEY", "Tavily"),
    ("PERPLEXITY_API_KEY", "Perplexity"),
];

/// Returns (provider_name, env_var_name) for the first detected key, or None.
fn detect_provider() -> Option<(&'static str, &'static str)> {
    for &(var, name) in PROVIDER_ENV_VARS {
        if std::env::var(var).ok().filter(|v| !v.is_empty()).is_some() {
            return Some((name, var));
        }
    }
    None
}

// ── State ────────────────────────────────────────────────────────────────────

/// State for the welcome screen.
pub struct WelcomeState {
    pub menu: ListState,
    pub daemon_url: Option<String>,
    pub daemon_agents: u64,
    pub menu_items: Vec<MenuItem>,
    /// True while we're probing the daemon in the background.
    pub detecting: bool,
    /// Spinner tick counter for the detecting animation.
    pub tick: usize,
    /// True after first Ctrl+C — requires a second press to exit.
    pub ctrl_c_pending: bool,
    /// Tick at which Ctrl+C was first pressed (auto-resets after timeout).
    ctrl_c_tick: usize,
    /// True when the setup wizard just completed — shows guidance banner.
    pub setup_just_completed: bool,
}

pub struct MenuItem {
    pub label: &'static str,
    pub hint: &'static str,
    pub action: WelcomeAction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WelcomeAction {
    ConnectDaemon,
    InProcess,
    Wizard,
    Exit,
}

impl WelcomeState {
    /// Ticks before the Ctrl+C pending state auto-resets (~2s at 50ms tick).
    const CTRL_C_TIMEOUT: usize = 40;

    pub fn new() -> Self {
        Self {
            menu: ListState::default(),
            daemon_url: None,
            daemon_agents: 0,
            menu_items: Vec::new(),
            detecting: true,
            tick: 0,
            ctrl_c_pending: false,
            ctrl_c_tick: 0,
            setup_just_completed: false,
        }
    }

    /// Called when daemon detection finishes (from background thread).
    pub fn on_daemon_detected(&mut self, url: Option<String>, agent_count: u64) {
        self.detecting = false;
        self.daemon_url = url;
        self.daemon_agents = agent_count;
        self.rebuild_menu();
    }

    fn rebuild_menu(&mut self) {
        self.menu_items.clear();
        if self.daemon_url.is_some() {
            self.menu_items.push(MenuItem {
                label: "Connect to daemon",
                hint: "talk to running agents via API",
                action: WelcomeAction::ConnectDaemon,
            });
        }
        self.menu_items.push(MenuItem {
            label: "Quick in-process chat",
            hint: "boot kernel locally, no daemon needed",
            action: WelcomeAction::InProcess,
        });
        self.menu_items.push(MenuItem {
            label: "Setup wizard",
            hint: "configure providers & channels",
            action: WelcomeAction::Wizard,
        });
        self.menu_items.push(MenuItem {
            label: "Exit",
            hint: "quit OpenFang",
            action: WelcomeAction::Exit,
        });
        self.menu.select(Some(0));
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        // Auto-reset Ctrl+C pending after timeout
        if self.ctrl_c_pending && self.tick.wrapping_sub(self.ctrl_c_tick) > Self::CTRL_C_TIMEOUT {
            self.ctrl_c_pending = false;
        }
    }

    /// Handle a key event. Returns Some(action) if one was selected.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<WelcomeAction> {
        let is_ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);

        if self.detecting {
            // Block input while detecting — only Ctrl+C (double) or q exits
            if is_ctrl_c {
                if self.ctrl_c_pending {
                    return Some(WelcomeAction::Exit);
                }
                self.ctrl_c_pending = true;
                self.ctrl_c_tick = self.tick;
                return None;
            }
            if key.code == KeyCode::Char('q') {
                return Some(WelcomeAction::Exit);
            }
            self.ctrl_c_pending = false;
            return None;
        }

        // Double Ctrl+C to exit
        if is_ctrl_c {
            if self.ctrl_c_pending {
                return Some(WelcomeAction::Exit);
            }
            self.ctrl_c_pending = true;
            self.ctrl_c_tick = self.tick;
            return None;
        }

        // Any other key clears the Ctrl+C pending state
        self.ctrl_c_pending = false;

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Some(WelcomeAction::Exit),
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.menu.selected().unwrap_or(0);
                let next = if i == 0 {
                    self.menu_items.len() - 1
                } else {
                    i - 1
                };
                self.menu.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.menu.selected().unwrap_or(0);
                let next = (i + 1) % self.menu_items.len();
                self.menu.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(i) = self.menu.selected() {
                    return Some(self.menu_items[i].action);
                }
            }
            _ => {}
        }
        None
    }
}

// ── Drawing ──────────────────────────────────────────────────────────────────

/// Render the welcome screen.
pub fn draw(f: &mut Frame, area: Rect, state: &mut WelcomeState) {
    // Fill background
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_PRIMARY)),
        area,
    );

    let version = env!("CARGO_PKG_VERSION");
    let compact = area.width < LOGO_MIN_WIDTH;

    // Logo height: full (6 lines) or compact (1 line)
    let logo_h: u16 = if compact { 1 } else { LOGO_HEIGHT };

    // Status block height
    let has_provider = detect_provider().is_some();
    let setup_extra: u16 = if state.setup_just_completed { 1 } else { 0 };
    let status_h: u16 = if state.detecting {
        1
    } else if has_provider {
        2 + setup_extra
    } else {
        3 + setup_extra
    };

    // Left-aligned content area
    let content = if area.width < 10 || area.height < 5 {
        area
    } else {
        let margin = 3u16.min(area.width.saturating_sub(10));
        let w = 80u16.min(area.width.saturating_sub(margin));
        Rect {
            x: area.x.saturating_add(margin),
            y: area.y,
            width: w,
            height: area.height,
        }
    };

    // Vertical layout with upper-third positioning
    let total_needed = 1 + logo_h + 1 + 1 + status_h + 1 + 4 + 1;
    let top_pad = if area.height > total_needed + 2 {
        ((area.height - total_needed) / 3).max(1)
    } else {
        1
    };

    let chunks = Layout::vertical([
        Constraint::Length(top_pad),  // top space
        Constraint::Length(logo_h),   // logo
        Constraint::Length(1),        // tagline + version
        Constraint::Length(1),        // separator
        Constraint::Length(status_h), // status block
        Constraint::Length(1),        // separator
        Constraint::Min(1),           // menu
        Constraint::Length(1),        // key hints
        Constraint::Min(0),           // remaining
    ])
    .split(content);

    // ── Logo ─────────────────────────────────────────────────────────────────
    if compact {
        let line = Line::from(vec![Span::styled(
            COMPACT_LOGO,
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )]);
        f.render_widget(Paragraph::new(line), chunks[1]);
    } else {
        let logo_lines: Vec<Line> = LOGO
            .lines()
            .map(|l| Line::from(vec![Span::styled(l, Style::default().fg(theme::ACCENT))]))
            .collect();
        f.render_widget(Paragraph::new(logo_lines), chunks[1]);
    }

    // ── Tagline + version ────────────────────────────────────────────────────
    let tagline = Line::from(vec![
        Span::styled(
            "Agent Operating System",
            Style::default()
                .fg(theme::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  v{version}"), theme::dim_style()),
    ]);
    f.render_widget(Paragraph::new(tagline), chunks[2]);

    // ── Separator ────────────────────────────────────────────────────────────
    let sep_w = content.width.min(60) as usize;
    let sep_line = Line::from(vec![Span::styled(
        "\u{2500}".repeat(sep_w),
        Style::default().fg(theme::BORDER),
    )]);
    f.render_widget(Paragraph::new(sep_line.clone()), chunks[3]);

    // ── Status block ─────────────────────────────────────────────────────────
    if state.detecting {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        let line = Line::from(vec![
            Span::styled(format!("{spinner} "), Style::default().fg(theme::YELLOW)),
            Span::styled("Checking for daemon\u{2026}", theme::dim_style()),
        ]);
        f.render_widget(Paragraph::new(line), chunks[4]);
    } else {
        let mut status_lines: Vec<Line> = Vec::new();

        // Daemon status
        if let Some(ref url) = state.daemon_url {
            let agent_suffix = if state.daemon_agents > 0 {
                format!(
                    " ({} agent{})",
                    state.daemon_agents,
                    if state.daemon_agents == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            };
            status_lines.push(Line::from(vec![
                Span::styled(
                    "\u{25cf} ",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("Daemon at {url}"),
                    Style::default().fg(theme::TEXT_PRIMARY),
                ),
                Span::styled(agent_suffix, Style::default().fg(theme::GREEN)),
            ]));
        } else {
            status_lines.push(Line::from(vec![
                Span::styled("\u{25cb} ", theme::dim_style()),
                Span::styled("No daemon running", theme::dim_style()),
            ]));
        }

        // Provider detection
        if let Some((provider, env_var)) = detect_provider() {
            status_lines.push(Line::from(vec![
                Span::styled(
                    "\u{2714} ",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("Provider: {provider}"),
                    Style::default().fg(theme::TEXT_PRIMARY),
                ),
                Span::styled(format!(" ({env_var})"), theme::dim_style()),
            ]));
        } else {
            status_lines.push(Line::from(vec![
                Span::styled("\u{25cb} ", Style::default().fg(theme::YELLOW)),
                Span::styled("No API keys detected", Style::default().fg(theme::YELLOW)),
            ]));
            status_lines.push(Line::from(vec![Span::styled(
                "  Run 'openfang init' to get started",
                theme::hint_style(),
            )]));
        }

        // Post-wizard guidance
        if state.setup_just_completed {
            status_lines.push(Line::from(vec![Span::styled(
                "\u{2714} Setup complete! Select 'Quick in-process chat' to try it out.",
                Style::default().fg(theme::GREEN),
            )]));
        }

        f.render_widget(Paragraph::new(status_lines), chunks[4]);
    }

    // ── Separator 2 ──────────────────────────────────────────────────────────
    f.render_widget(Paragraph::new(sep_line), chunks[5]);

    // ── Menu ─────────────────────────────────────────────────────────────────
    if !state.detecting {
        let items: Vec<ListItem> = state
            .menu_items
            .iter()
            .map(|item| {
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{:<26}", item.label)),
                    Span::styled(item.hint, theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(theme::ACCENT)
                    .bg(theme::BG_HOVER)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("\u{25b8} ");

        f.render_stateful_widget(list, chunks[6], &mut state.menu);
    }

    // ── Hints ────────────────────────────────────────────────────────────────
    let hints = if state.ctrl_c_pending {
        Line::from(vec![Span::styled(
            "Press Ctrl+C again to exit",
            Style::default().fg(theme::YELLOW),
        )])
    } else {
        Line::from(vec![Span::styled(
            "\u{2191}\u{2193} navigate  enter select  q quit",
            theme::hint_style(),
        )])
    };
    f.render_widget(Paragraph::new(hints), chunks[7]);
}
