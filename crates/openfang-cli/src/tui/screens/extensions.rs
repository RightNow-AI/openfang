//! Extensions screen: browse, install/remove integrations, view MCP health.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub installed: bool,
    pub status: String,
    pub tags: Vec<String>,
    #[allow(dead_code)]
    pub has_oauth: bool,
}

#[derive(Clone, Default)]
pub struct ExtensionHealthInfo {
    pub id: String,
    pub status: String,
    pub tool_count: usize,
    #[allow(dead_code)]
    pub last_ok: String,
    pub last_error: String,
    pub consecutive_failures: u32,
    pub reconnecting: bool,
    pub connected_since: String,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExtSub {
    Browse,
    Installed,
    Health,
}

pub struct ExtensionsState {
    pub sub: ExtSub,
    pub all_extensions: Vec<ExtensionInfo>,
    pub health_entries: Vec<ExtensionHealthInfo>,
    pub browse_list: ListState,
    pub installed_list: ListState,
    pub health_list: ListState,
    pub search_query: String,
    pub searching: bool,
    pub loading: bool,
    pub tick: usize,
    pub confirm_remove: bool,
    pub status_msg: String,
}

pub enum ExtensionsAction {
    Continue,
    RefreshAll,
    RefreshHealth,
    Install(String),
    Remove(String),
    Reconnect(String),
}

impl ExtensionsState {
    pub fn new() -> Self {
        Self {
            sub: ExtSub::Browse,
            all_extensions: Vec::new(),
            health_entries: Vec::new(),
            browse_list: ListState::default(),
            installed_list: ListState::default(),
            health_list: ListState::default(),
            search_query: String::new(),
            searching: false,
            loading: false,
            tick: 0,
            confirm_remove: false,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    fn filtered(&self) -> Vec<&ExtensionInfo> {
        let q = self.search_query.to_lowercase();
        self.all_extensions
            .iter()
            .filter(|e| {
                if q.is_empty() {
                    return true;
                }
                e.name.to_lowercase().contains(&q)
                    || e.id.to_lowercase().contains(&q)
                    || e.category.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    fn installed_list_data(&self) -> Vec<&ExtensionInfo> {
        self.all_extensions.iter().filter(|e| e.installed).collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ExtensionsAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ExtensionsAction::Continue;
        }

        // Search mode
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.searching = false;
                    self.search_query.clear();
                }
                KeyCode::Enter => {
                    self.searching = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                }
                _ => {}
            }
            return ExtensionsAction::Continue;
        }

        // Sub-tab switching (1/2/3)
        match key.code {
            KeyCode::Char('1') => {
                self.sub = ExtSub::Browse;
                return ExtensionsAction::RefreshAll;
            }
            KeyCode::Char('2') => {
                self.sub = ExtSub::Installed;
                return ExtensionsAction::RefreshAll;
            }
            KeyCode::Char('3') => {
                self.sub = ExtSub::Health;
                return ExtensionsAction::RefreshHealth;
            }
            KeyCode::Char('/') => {
                if self.sub == ExtSub::Browse {
                    self.searching = true;
                    self.search_query.clear();
                    return ExtensionsAction::Continue;
                }
            }
            _ => {}
        }

        match self.sub {
            ExtSub::Browse => self.handle_browse(key),
            ExtSub::Installed => self.handle_installed(key),
            ExtSub::Health => self.handle_health(key),
        }
    }

    fn handle_browse(&mut self, key: KeyEvent) -> ExtensionsAction {
        let total = self.filtered().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.browse_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.browse_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.browse_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.browse_list.select(Some(next));
                }
            }
            KeyCode::Enter => {
                let filtered = self.filtered();
                if let Some(sel) = self.browse_list.selected() {
                    if sel < filtered.len() {
                        let ext = filtered[sel];
                        if !ext.installed {
                            return ExtensionsAction::Install(ext.id.clone());
                        }
                    }
                }
            }
            KeyCode::Char('r') => return ExtensionsAction::RefreshAll,
            _ => {}
        }
        ExtensionsAction::Continue
    }

    fn handle_installed(&mut self, key: KeyEvent) -> ExtensionsAction {
        if self.confirm_remove {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_remove = false;
                    let installed = self.installed_list_data();
                    if let Some(sel) = self.installed_list.selected() {
                        if sel < installed.len() {
                            return ExtensionsAction::Remove(installed[sel].id.clone());
                        }
                    }
                }
                _ => self.confirm_remove = false,
            }
            return ExtensionsAction::Continue;
        }

        let total = self.installed_list_data().len();
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
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.installed_list.selected().is_some() {
                    self.confirm_remove = true;
                }
            }
            KeyCode::Char('r') => return ExtensionsAction::RefreshAll,
            _ => {}
        }
        ExtensionsAction::Continue
    }

    fn handle_health(&mut self, key: KeyEvent) -> ExtensionsAction {
        let total = self.health_entries.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.health_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.health_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.health_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.health_list.select(Some(next));
                }
            }
            KeyCode::Char('r') | KeyCode::Enter => {
                if let Some(sel) = self.health_list.selected() {
                    if sel < self.health_entries.len() {
                        return ExtensionsAction::Reconnect(self.health_entries[sel].id.clone());
                    }
                }
            }
            _ => {}
        }
        ExtensionsAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut ExtensionsState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Extensions ",
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

    draw_sub_tabs(f, chunks[0], state);

    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, theme::dim_style())),
        chunks[1],
    );

    match state.sub {
        ExtSub::Browse => draw_browse(f, chunks[2], state),
        ExtSub::Installed => draw_installed(f, chunks[2], state),
        ExtSub::Health => draw_health(f, chunks[2], state),
    }
}

fn draw_sub_tabs(f: &mut Frame, area: Rect, state: &ExtensionsState) {
    let tabs = [
        (ExtSub::Browse, "1 Browse"),
        (ExtSub::Installed, "2 Installed"),
        (ExtSub::Health, "3 Health"),
    ];
    let mut spans = vec![Span::raw("  ")];
    for (sub, label) in &tabs {
        let style = if *sub == state.sub {
            theme::tab_active()
        } else {
            theme::tab_inactive()
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }

    // Show search query if active
    if state.searching {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("Search: ", Style::default().fg(theme::YELLOW)));
        spans.push(Span::styled(
            format!("{}_", state.search_query),
            theme::input_style(),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn status_badge(status: &str) -> (String, Style) {
    let lower = status.to_lowercase();
    if lower.contains("ready") || lower.contains("connected") {
        ("[Ready]".to_string(), Style::default().fg(theme::GREEN))
    } else if lower.contains("setup") {
        ("[Setup]".to_string(), Style::default().fg(theme::YELLOW))
    } else if lower.contains("error") {
        ("[Error]".to_string(), Style::default().fg(theme::RED))
    } else if lower.contains("disabled") {
        ("[Off]".to_string(), theme::dim_style())
    } else {
        ("".to_string(), theme::dim_style())
    }
}

fn draw_browse(f: &mut Frame, area: Rect, state: &mut ExtensionsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<3} {:<18} {:<12} {:<10} {}",
                "", "Name", "Category", "Status", "Description"
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
                Span::styled("Loading integrations\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.all_extensions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No integrations loaded. Press r to refresh.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        // Collect filtered data to avoid borrow conflict with browse_list
        let items: Vec<ListItem> = state
            .filtered()
            .iter()
            .map(|ext| {
                let (badge, badge_style) = if ext.installed {
                    ("[Installed]".to_string(), Style::default().fg(theme::GREEN))
                } else {
                    ("[Available]".to_string(), theme::dim_style())
                };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} ", ext.icon), Style::default()),
                    Span::styled(
                        format!("{:<16} ", ext.name),
                        Style::default().fg(theme::TEXT_PRIMARY),
                    ),
                    Span::styled(format!("{:<12} ", ext.category), theme::dim_style()),
                    Span::styled(format!("{:<10} ", badge), badge_style),
                    Span::styled(ext.description.clone(), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items).highlight_style(theme::selected_style());
        f.render_stateful_widget(list, chunks[1], &mut state.browse_list);
    }

    let hints = if state.searching {
        "  Type to search \u{2022} Esc cancel \u{2022} Enter confirm"
    } else {
        "  j/k navigate \u{2022} Enter install \u{2022} / search \u{2022} r refresh"
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, theme::hint_style())),
        chunks[2],
    );
}

fn draw_installed(f: &mut Frame, area: Rect, state: &mut ExtensionsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<3} {:<18} {:<12} {:<10} {}",
                "", "Name", "Category", "Status", "ID"
            ),
            theme::table_header(),
        )])),
        chunks[0],
    );

    // Collect installed items into owned data to avoid borrow conflict with installed_list
    let items: Vec<ListItem> = state
        .all_extensions
        .iter()
        .filter(|e| e.installed)
        .map(|ext| {
            let (badge, badge_style) = status_badge(&ext.status);
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", ext.icon), Style::default()),
                Span::styled(
                    format!("{:<16} ", ext.name),
                    Style::default().fg(theme::TEXT_PRIMARY),
                ),
                Span::styled(format!("{:<12} ", ext.category), theme::dim_style()),
                Span::styled(format!("{:<10} ", badge), badge_style),
                Span::styled(ext.id.clone(), theme::dim_style()),
            ]))
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No integrations installed. Browse tab to add.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let list = List::new(items).highlight_style(theme::selected_style());
        f.render_stateful_widget(list, chunks[1], &mut state.installed_list);
    }

    let hints = if state.confirm_remove {
        "  Press y to confirm removal, any other key to cancel"
    } else {
        "  j/k navigate \u{2022} d remove \u{2022} r refresh"
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, theme::hint_style())),
        chunks[2],
    );
}

fn draw_health(f: &mut Frame, area: Rect, state: &mut ExtensionsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<18} {:<10} {:<6} {:<12} {:<6} {}",
                "Server", "Status", "Tools", "Connected", "Fails", "Last Error"
            ),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.health_entries.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No MCP health data. Install integrations first.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .health_entries
            .iter()
            .map(|h| {
                let (badge, badge_style) = status_badge(&h.status);
                let error_display = if h.last_error.is_empty() {
                    "\u{2014}".to_string()
                } else if h.last_error.len() > 30 {
                    format!("{}...", openfang_types::truncate_str(&h.last_error, 27))
                } else {
                    h.last_error.clone()
                };
                let reconn = if h.reconnecting { " \u{21bb}" } else { "" };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<16} ", h.id),
                        Style::default().fg(theme::TEXT_PRIMARY),
                    ),
                    Span::styled(format!("{:<10} ", badge), badge_style),
                    Span::styled(
                        format!("{:<6} ", h.tool_count),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::styled(
                        format!(
                            "{:<12} ",
                            if h.connected_since.is_empty() {
                                "\u{2014}"
                            } else {
                                &h.connected_since
                            }
                        ),
                        theme::dim_style(),
                    ),
                    Span::styled(
                        format!("{:<6}", h.consecutive_failures),
                        if h.consecutive_failures > 0 {
                            Style::default().fg(theme::RED)
                        } else {
                            theme::dim_style()
                        },
                    ),
                    Span::styled(format!(" {error_display}{reconn}"), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items).highlight_style(theme::selected_style());
        f.render_stateful_widget(list, chunks[1], &mut state.health_list);
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            "  j/k navigate \u{2022} r/Enter reconnect \u{2022} auto-reconnect active",
            theme::hint_style(),
        )),
        chunks[2],
    );
}
