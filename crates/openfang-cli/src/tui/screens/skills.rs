//! Skills screen: installed skills, ClawHub marketplace, MCP servers.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct SkillInfo {
    pub name: String,
    pub runtime: String,
    pub source: String,
    pub description: String,
}

#[derive(Clone, Default)]
pub struct ClawHubResult {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub downloads: u64,
    pub runtime: String,
}

#[derive(Clone, Default)]
pub struct McpServerInfo {
    pub name: String,
    pub connected: bool,
    pub tool_count: usize,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SkillsSub {
    Installed,
    ClawHub,
    Mcp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClawHubSort {
    Trending,
    Popular,
    Recent,
}

impl ClawHubSort {
    fn label(self) -> &'static str {
        match self {
            Self::Trending => "trending",
            Self::Popular => "popular",
            Self::Recent => "recent",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Trending => Self::Popular,
            Self::Popular => Self::Recent,
            Self::Recent => Self::Trending,
        }
    }
}

pub struct SkillsState {
    pub sub: SkillsSub,
    pub installed: Vec<SkillInfo>,
    pub clawhub_results: Vec<ClawHubResult>,
    pub mcp_servers: Vec<McpServerInfo>,
    pub installed_list: ListState,
    pub clawhub_list: ListState,
    pub mcp_list: ListState,
    pub search_buf: String,
    pub search_mode: bool,
    pub sort: ClawHubSort,
    pub loading: bool,
    pub tick: usize,
    pub confirm_uninstall: bool,
    pub status_msg: String,
}

pub enum SkillsAction {
    Continue,
    RefreshInstalled,
    SearchClawHub(String),
    BrowseClawHub(String),
    InstallSkill(String),
    UninstallSkill(String),
    RefreshMcp,
}

impl SkillsState {
    pub fn new() -> Self {
        Self {
            sub: SkillsSub::Installed,
            installed: Vec::new(),
            clawhub_results: Vec::new(),
            mcp_servers: Vec::new(),
            installed_list: ListState::default(),
            clawhub_list: ListState::default(),
            mcp_list: ListState::default(),
            search_buf: String::new(),
            search_mode: false,
            sort: ClawHubSort::Trending,
            loading: false,
            tick: 0,
            confirm_uninstall: false,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SkillsAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return SkillsAction::Continue;
        }

        // Tab switching within Skills (1/2/3)
        if !self.search_mode {
            match key.code {
                KeyCode::Char('1') => {
                    self.sub = SkillsSub::Installed;
                    return SkillsAction::RefreshInstalled;
                }
                KeyCode::Char('2') => {
                    self.sub = SkillsSub::ClawHub;
                    return SkillsAction::BrowseClawHub(self.sort.label().to_string());
                }
                KeyCode::Char('3') => {
                    self.sub = SkillsSub::Mcp;
                    return SkillsAction::RefreshMcp;
                }
                _ => {}
            }
        }

        match self.sub {
            SkillsSub::Installed => self.handle_installed(key),
            SkillsSub::ClawHub => self.handle_clawhub(key),
            SkillsSub::Mcp => self.handle_mcp(key),
        }
    }

    fn handle_installed(&mut self, key: KeyEvent) -> SkillsAction {
        if self.confirm_uninstall {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_uninstall = false;
                    if let Some(sel) = self.installed_list.selected() {
                        if sel < self.installed.len() {
                            return SkillsAction::UninstallSkill(self.installed[sel].name.clone());
                        }
                    }
                }
                _ => self.confirm_uninstall = false,
            }
            return SkillsAction::Continue;
        }

        let total = self.installed.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.installed_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.installed_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.installed_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.installed_list.select(Some(next));
                }
            }
            KeyCode::Char('u') => {
                if self.installed_list.selected().is_some() {
                    self.confirm_uninstall = true;
                }
            }
            KeyCode::Char('r') => return SkillsAction::RefreshInstalled,
            _ => {}
        }
        SkillsAction::Continue
    }

    fn handle_clawhub(&mut self, key: KeyEvent) -> SkillsAction {
        if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                    if !self.search_buf.is_empty() {
                        return SkillsAction::SearchClawHub(self.search_buf.clone());
                    }
                }
                KeyCode::Backspace => {
                    self.search_buf.pop();
                }
                KeyCode::Char(c) => {
                    self.search_buf.push(c);
                }
                _ => {}
            }
            return SkillsAction::Continue;
        }

        let total = self.clawhub_results.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.clawhub_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.clawhub_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.clawhub_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.clawhub_list.select(Some(next));
                }
            }
            KeyCode::Char('i') => {
                if let Some(sel) = self.clawhub_list.selected() {
                    if sel < self.clawhub_results.len() {
                        return SkillsAction::InstallSkill(self.clawhub_results[sel].slug.clone());
                    }
                }
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_buf.clear();
            }
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                return SkillsAction::BrowseClawHub(self.sort.label().to_string());
            }
            KeyCode::Char('r') => {
                return SkillsAction::BrowseClawHub(self.sort.label().to_string());
            }
            _ => {}
        }
        SkillsAction::Continue
    }

    fn handle_mcp(&mut self, key: KeyEvent) -> SkillsAction {
        let total = self.mcp_servers.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.mcp_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.mcp_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.mcp_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.mcp_list.select(Some(next));
                }
            }
            KeyCode::Char('r') => return SkillsAction::RefreshMcp,
            _ => {}
        }
        SkillsAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut SkillsState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Skills ",
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
        SkillsSub::Installed => draw_installed(f, chunks[2], state),
        SkillsSub::ClawHub => draw_clawhub(f, chunks[2], state),
        SkillsSub::Mcp => draw_mcp(f, chunks[2], state),
    }
}

fn draw_sub_tabs(f: &mut Frame, area: Rect, active: SkillsSub) {
    let tabs = [
        (SkillsSub::Installed, "1 Installed"),
        (SkillsSub::ClawHub, "2 ClawHub"),
        (SkillsSub::Mcp, "3 MCP Servers"),
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

fn draw_installed(f: &mut Frame, area: Rect, state: &mut SkillsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<20} {:<8} {:<12} {}",
                "Name", "Runtime", "Source", "Description"
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
                Span::styled("Loading skills\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.installed.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No skills installed. Press [2] to browse ClawHub.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .installed
            .iter()
            .map(|s| {
                let runtime_style = match s.runtime.as_str() {
                    "python" | "py" => Style::default().fg(theme::BLUE),
                    "node" | "js" => Style::default().fg(theme::YELLOW),
                    "wasm" => Style::default().fg(theme::PURPLE),
                    _ => Style::default().fg(theme::GREEN),
                };
                let runtime_badge = match s.runtime.as_str() {
                    "python" | "py" => "PY",
                    "node" | "js" => "JS",
                    "wasm" => "WASM",
                    "prompt" => "PROMPT",
                    _ => &s.runtime,
                };
                let source_style = match s.source.as_str() {
                    "clawhub" => Style::default().fg(theme::ACCENT),
                    "builtin" | "built-in" => Style::default().fg(theme::GREEN),
                    _ => theme::dim_style(),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<20}", truncate(&s.name, 19)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!(" {:<8}", runtime_badge), runtime_style),
                    Span::styled(format!(" {:<12}", &s.source), source_style),
                    Span::styled(
                        format!(" {}", truncate(&s.description, 30)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.installed_list);
    }

    if state.confirm_uninstall {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  Uninstall this skill? [y] Yes  [any] Cancel",
                Style::default().fg(theme::YELLOW),
            )])),
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
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  [\u{2191}\u{2193}] Navigate  [u] Uninstall  [r] Refresh",
                theme::hint_style(),
            )])),
            chunks[2],
        );
    }
}

fn draw_clawhub(f: &mut Frame, area: Rect, state: &mut SkillsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // search / sort
        Constraint::Min(3),    // results
        Constraint::Length(1), // hints
    ])
    .split(area);

    if state.search_mode {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  / ", Style::default().fg(theme::ACCENT)),
                Span::styled(&state.search_buf, theme::input_style()),
                Span::styled(
                    "\u{2588}",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])),
            chunks[0],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(
                        "  {:<24} {:<10} {:<10} {}",
                        "Name", "Downloads", "Runtime", "Description"
                    ),
                    theme::table_header(),
                ),
                Span::styled(
                    format!("  Sort: {}", state.sort.label()),
                    Style::default().fg(theme::YELLOW),
                ),
            ])),
            chunks[0],
        );
    }

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Searching ClawHub\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.clawhub_results.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No results. Press [/] to search or [s] to change sort.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .clawhub_results
            .iter()
            .map(|r| {
                let dl = format_count(r.downloads);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<24}", truncate(&r.name, 23)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!(" {:<10}", dl), Style::default().fg(theme::GREEN)),
                    Span::styled(
                        format!(" {:<10}", &r.runtime),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::styled(
                        format!(" {}", truncate(&r.description, 30)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.clawhub_list);
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}] Navigate  [i] Install  [/] Search  [s] Sort  [r] Refresh",
            theme::hint_style(),
        )])),
        chunks[2],
    );
}

fn draw_mcp(f: &mut Frame, area: Rect, state: &mut SkillsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {:<20} {:<14} {}", "Server", "Status", "Tools"),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading MCP servers\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.mcp_servers.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No MCP servers configured.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .mcp_servers
            .iter()
            .map(|s| {
                let (badge, style) = if s.connected {
                    ("\u{2714} Connected", Style::default().fg(theme::GREEN))
                } else {
                    ("\u{2718} Disconnected", Style::default().fg(theme::RED))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<20}", truncate(&s.name, 19)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!(" {:<14}", badge), style),
                    Span::styled(format!(" {}", s.tool_count), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.mcp_list);
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}] Navigate  [r] Refresh",
            theme::hint_style(),
        )])),
        chunks[2],
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}
