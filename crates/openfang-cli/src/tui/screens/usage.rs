//! Usage screen: token/cost analytics with summary, by-model, by-agent views.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct UsageSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_calls: u64,
}

#[derive(Clone, Default)]
pub struct ModelUsage {
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub calls: u64,
}

#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct AgentUsage {
    pub agent_name: String,
    pub agent_id: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub tool_calls: u64,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UsageSub {
    Summary,
    ByModel,
    ByAgent,
}

pub struct UsageState {
    pub sub: UsageSub,
    pub summary: UsageSummary,
    pub by_model: Vec<ModelUsage>,
    pub by_agent: Vec<AgentUsage>,
    pub model_list: ListState,
    pub agent_list: ListState,
    pub loading: bool,
    pub tick: usize,
}

pub enum UsageAction {
    Continue,
    Refresh,
}

impl UsageState {
    pub fn new() -> Self {
        Self {
            sub: UsageSub::Summary,
            summary: UsageSummary::default(),
            by_model: Vec::new(),
            by_agent: Vec::new(),
            model_list: ListState::default(),
            agent_list: ListState::default(),
            loading: false,
            tick: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> UsageAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return UsageAction::Continue;
        }

        // Sub-tab switching
        match key.code {
            KeyCode::Char('1') => {
                self.sub = UsageSub::Summary;
                return UsageAction::Continue;
            }
            KeyCode::Char('2') => {
                self.sub = UsageSub::ByModel;
                return UsageAction::Continue;
            }
            KeyCode::Char('3') => {
                self.sub = UsageSub::ByAgent;
                return UsageAction::Continue;
            }
            _ => {}
        }

        match self.sub {
            UsageSub::Summary => {
                if key.code == KeyCode::Char('r') {
                    return UsageAction::Refresh;
                }
            }
            UsageSub::ByModel => {
                let total = self.by_model.len();
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
                    KeyCode::Char('r') => return UsageAction::Refresh,
                    _ => {}
                }
            }
            UsageSub::ByAgent => {
                let total = self.by_agent.len();
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if total > 0 {
                            let i = self.agent_list.selected().unwrap_or(0);
                            let next = if i == 0 { total - 1 } else { i - 1 };
                            self.agent_list.select(Some(next));
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if total > 0 {
                            let i = self.agent_list.selected().unwrap_or(0);
                            let next = (i + 1) % total;
                            self.agent_list.select(Some(next));
                        }
                    }
                    KeyCode::Char('r') => return UsageAction::Refresh,
                    _ => {}
                }
            }
        }
        UsageAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut UsageState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Usage ",
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

    // Sub-tab bar
    draw_sub_tabs(f, chunks[0], state.sub);

    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, theme::dim_style())),
        chunks[1],
    );

    match state.sub {
        UsageSub::Summary => draw_summary(f, chunks[2], state),
        UsageSub::ByModel => draw_by_model(f, chunks[2], state),
        UsageSub::ByAgent => draw_by_agent(f, chunks[2], state),
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [1] Summary  [2] By Model  [3] By Agent  [r] Refresh",
            theme::hint_style(),
        )])),
        chunks[3],
    );
}

fn draw_sub_tabs(f: &mut Frame, area: Rect, active: UsageSub) {
    let tabs = [
        (UsageSub::Summary, "1 Summary"),
        (UsageSub::ByModel, "2 By Model"),
        (UsageSub::ByAgent, "3 By Agent"),
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

fn draw_summary(f: &mut Frame, area: Rect, state: &UsageState) {
    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading usage data\u{2026}", theme::dim_style()),
            ])),
            area,
        );
        return;
    }

    let cols = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .split(area);

    draw_stat_card(
        f,
        cols[0],
        "Input Tokens",
        &format_tokens(state.summary.total_input_tokens),
        theme::BLUE,
    );
    draw_stat_card(
        f,
        cols[1],
        "Output Tokens",
        &format_tokens(state.summary.total_output_tokens),
        theme::GREEN,
    );
    draw_stat_card(
        f,
        cols[2],
        "Total Cost",
        &format!("${:.4}", state.summary.total_cost_usd),
        theme::YELLOW,
    );
    draw_stat_card(
        f,
        cols[3],
        "API Calls",
        &format_tokens(state.summary.total_calls),
        theme::CYAN,
    );
}

fn draw_stat_card(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    color: ratatui::style::Color,
) {
    let card = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::DIM));
    let card_inner = card.inner(area);
    f.render_widget(card, area);
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(" {value}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )])),
        card_inner,
    );
}

fn draw_by_model(f: &mut Frame, area: Rect, state: &mut UsageState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<28} {:<14} {:<14} {:<10} {}",
                "Model", "Input Tokens", "Output Tokens", "Cost", "Calls"
            ),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.by_model.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No usage data.", theme::dim_style())),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .by_model
            .iter()
            .map(|m| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<28}", truncate(&m.model_id, 27)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<14}", format_tokens(m.input_tokens)),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::styled(
                        format!(" {:<14}", format_tokens(m.output_tokens)),
                        Style::default().fg(theme::GREEN),
                    ),
                    Span::styled(
                        format!(" ${:<9.4}", m.cost_usd),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(format!(" {}", m.calls), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.model_list);
    }
}

fn draw_by_agent(f: &mut Frame, area: Rect, state: &mut UsageState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<24} {:<16} {:<12} {}",
                "Agent", "Total Tokens", "Cost", "Tool Calls"
            ),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.by_agent.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No usage data.", theme::dim_style())),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .by_agent
            .iter()
            .map(|a| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<24}", truncate(&a.agent_name, 23)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<16}", format_tokens(a.total_tokens)),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::styled(
                        format!(" ${:<11.4}", a.cost_usd),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(format!(" {}", a.tool_calls), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.agent_list);
    }
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}
