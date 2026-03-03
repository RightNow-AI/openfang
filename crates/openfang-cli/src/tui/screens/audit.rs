//! Audit screen: audit log viewer with action filter and chain verification.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct AuditEntry {
    pub timestamp: String,
    pub action: String,
    pub agent: String,
    pub detail: String,
    pub tip_hash: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuditFilter {
    All,
    AgentSpawn,
    AgentKill,
    ToolInvoke,
    NetworkAccess,
    ShellExec,
}

impl AuditFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::AgentSpawn => "Agent Created",
            Self::AgentKill => "Agent Killed",
            Self::ToolInvoke => "Tool Used",
            Self::NetworkAccess => "Network",
            Self::ShellExec => "Shell Exec",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::All => Self::AgentSpawn,
            Self::AgentSpawn => Self::AgentKill,
            Self::AgentKill => Self::ToolInvoke,
            Self::ToolInvoke => Self::NetworkAccess,
            Self::NetworkAccess => Self::ShellExec,
            Self::ShellExec => Self::All,
        }
    }
    fn matches(self, action: &str) -> bool {
        match self {
            Self::All => true,
            Self::AgentSpawn => {
                action.contains("Spawn")
                    || action.contains("spawn")
                    || action.contains("Create")
                    || action.contains("create")
            }
            Self::AgentKill => {
                action.contains("Kill")
                    || action.contains("kill")
                    || action.contains("Stop")
                    || action.contains("stop")
            }
            Self::ToolInvoke => {
                action.contains("Tool")
                    || action.contains("tool")
                    || action.contains("Invoke")
                    || action.contains("invoke")
            }
            Self::NetworkAccess => {
                action.contains("Net")
                    || action.contains("net")
                    || action.contains("Fetch")
                    || action.contains("fetch")
                    || action.contains("Http")
                    || action.contains("http")
            }
            Self::ShellExec => {
                action.contains("Shell")
                    || action.contains("shell")
                    || action.contains("Exec")
                    || action.contains("exec")
                    || action.contains("Process")
                    || action.contains("process")
            }
        }
    }
}

/// Map raw action names to friendly display names.
fn friendly_action(action: &str) -> &str {
    match action {
        "AgentSpawn" | "AgentSpawned" => "Agent Created",
        "AgentKill" | "AgentKilled" => "Agent Killed",
        "ToolInvoke" | "ToolInvocation" => "Tool Used",
        "NetworkAccess" | "NetFetch" => "Network Access",
        "ShellExec" | "ShellCommand" => "Shell Exec",
        "CapabilityDenied" => "Access Denied",
        "ConfigChange" => "Config Changed",
        other => other,
    }
}

// ── State ───────────────────────────────────────────────────────────────────

pub struct AuditState {
    pub entries: Vec<AuditEntry>,
    pub filtered: Vec<usize>,
    pub action_filter: AuditFilter,
    pub list_state: ListState,
    pub chain_verified: Option<bool>,
    pub loading: bool,
    pub tick: usize,
    pub status_msg: String,
}

pub enum AuditAction {
    Continue,
    Refresh,
    VerifyChain,
}

impl AuditState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filtered: Vec::new(),
            action_filter: AuditFilter::All,
            list_state: ListState::default(),
            chain_verified: None,
            loading: false,
            tick: 0,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn refilter(&mut self) {
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.action_filter.matches(&e.action))
            .map(|(i, _)| i)
            .collect();
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AuditAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return AuditAction::Continue;
        }

        let total = self.filtered.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.list_state.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.list_state.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.list_state.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.list_state.select(Some(next));
                }
            }
            KeyCode::Char('f') => {
                self.action_filter = self.action_filter.next();
                self.refilter();
            }
            KeyCode::Char('v') => return AuditAction::VerifyChain,
            KeyCode::Char('r') => return AuditAction::Refresh,
            _ => {}
        }
        AuditAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut AuditState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Audit Trail ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(2), // header + filter
        Constraint::Min(3),    // list
        Constraint::Length(2), // chain status + hints
    ])
    .split(inner);

    // ── Header + filter ──
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Filter: ", theme::dim_style()),
                Span::styled(
                    format!("[{}]", state.action_filter.label()),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({} entries)", state.filtered.len()),
                    theme::dim_style(),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!(
                    "  {:<20} {:<16} {:<14} {:<10} {}",
                    "Timestamp", "Action", "Agent", "Hash", "Detail"
                ),
                theme::table_header(),
            )]),
        ]),
        chunks[0],
    );

    // ── List ──
    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading audit trail\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.filtered.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No audit entries match the current filter.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .filtered
            .iter()
            .map(|&idx| {
                let e = &state.entries[idx];
                let action_display = friendly_action(&e.action);
                let action_style = if e.action.contains("Kill") || e.action.contains("Denied") {
                    Style::default().fg(theme::RED)
                } else if e.action.contains("Spawn") || e.action.contains("Create") {
                    Style::default().fg(theme::GREEN)
                } else if e.action.contains("Tool") {
                    Style::default().fg(theme::BLUE)
                } else {
                    Style::default().fg(theme::YELLOW)
                };
                let hash_short = if e.tip_hash.len() > 8 {
                    &e.tip_hash[..8]
                } else {
                    &e.tip_hash
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<20}", truncate(&e.timestamp, 19)),
                        theme::dim_style(),
                    ),
                    Span::styled(
                        format!(" {:<16}", truncate(action_display, 15)),
                        action_style,
                    ),
                    Span::styled(
                        format!(" {:<14}", truncate(&e.agent, 13)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<10}", hash_short),
                        Style::default().fg(theme::PURPLE),
                    ),
                    Span::styled(format!(" {}", truncate(&e.detail, 24)), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.list_state);
    }

    // ── Chain status + hints ──
    let chain_line = match state.chain_verified {
        None => Line::from(vec![Span::styled(
            "  Chain: not verified",
            theme::dim_style(),
        )]),
        Some(true) => Line::from(vec![Span::styled(
            "  Chain: \u{2714} Verified",
            Style::default().fg(theme::GREEN),
        )]),
        Some(false) => Line::from(vec![Span::styled(
            "  Chain: \u{2718} Verification failed",
            Style::default().fg(theme::RED),
        )]),
    };

    let hints = if !state.status_msg.is_empty() {
        Line::from(vec![Span::styled(
            format!("  {}", state.status_msg),
            Style::default().fg(theme::GREEN),
        )])
    } else {
        Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}] Navigate  [f] Filter  [v] Verify Chain  [r] Refresh",
            theme::hint_style(),
        )])
    };

    f.render_widget(Paragraph::new(vec![chain_line, hints]), chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}
