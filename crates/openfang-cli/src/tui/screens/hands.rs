//! Hands screen: marketplace of curated autonomous capability packages + active instances.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct HandInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub requirements_met: bool,
}

#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct HandInstanceInfo {
    pub instance_id: String,
    pub hand_id: String,
    pub status: String,
    pub agent_name: String,
    pub agent_id: String,
    pub activated_at: String,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HandsSub {
    Marketplace,
    Active,
}

pub struct HandsState {
    pub sub: HandsSub,
    pub definitions: Vec<HandInfo>,
    pub instances: Vec<HandInstanceInfo>,
    pub marketplace_list: ListState,
    pub active_list: ListState,
    pub loading: bool,
    pub tick: usize,
    pub confirm_deactivate: bool,
    pub status_msg: String,
}

pub enum HandsAction {
    Continue,
    RefreshDefinitions,
    RefreshActive,
    ActivateHand(String),
    DeactivateHand(String),
    PauseHand(String),
    ResumeHand(String),
}

impl HandsState {
    pub fn new() -> Self {
        Self {
            sub: HandsSub::Marketplace,
            definitions: Vec::new(),
            instances: Vec::new(),
            marketplace_list: ListState::default(),
            active_list: ListState::default(),
            loading: false,
            tick: 0,
            confirm_deactivate: false,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> HandsAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return HandsAction::Continue;
        }

        // Sub-tab switching (1/2)
        match key.code {
            KeyCode::Char('1') => {
                self.sub = HandsSub::Marketplace;
                return HandsAction::RefreshDefinitions;
            }
            KeyCode::Char('2') => {
                self.sub = HandsSub::Active;
                return HandsAction::RefreshActive;
            }
            _ => {}
        }

        match self.sub {
            HandsSub::Marketplace => self.handle_marketplace(key),
            HandsSub::Active => self.handle_active(key),
        }
    }

    fn handle_marketplace(&mut self, key: KeyEvent) -> HandsAction {
        let total = self.definitions.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.marketplace_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.marketplace_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.marketplace_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.marketplace_list.select(Some(next));
                }
            }
            KeyCode::Enter | KeyCode::Char('a') => {
                if let Some(sel) = self.marketplace_list.selected() {
                    if sel < self.definitions.len() {
                        return HandsAction::ActivateHand(self.definitions[sel].id.clone());
                    }
                }
            }
            KeyCode::Char('r') => return HandsAction::RefreshDefinitions,
            _ => {}
        }
        HandsAction::Continue
    }

    fn handle_active(&mut self, key: KeyEvent) -> HandsAction {
        if self.confirm_deactivate {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_deactivate = false;
                    if let Some(sel) = self.active_list.selected() {
                        if sel < self.instances.len() {
                            return HandsAction::DeactivateHand(
                                self.instances[sel].instance_id.clone(),
                            );
                        }
                    }
                }
                _ => self.confirm_deactivate = false,
            }
            return HandsAction::Continue;
        }

        let total = self.instances.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if total > 0 {
                    let i = self.active_list.selected().unwrap_or(0);
                    let next = if i == 0 { total - 1 } else { i - 1 };
                    self.active_list.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total > 0 {
                    let i = self.active_list.selected().unwrap_or(0);
                    let next = (i + 1) % total;
                    self.active_list.select(Some(next));
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.active_list.selected().is_some() {
                    self.confirm_deactivate = true;
                }
            }
            KeyCode::Char('p') => {
                if let Some(sel) = self.active_list.selected() {
                    if sel < self.instances.len() {
                        let inst = &self.instances[sel];
                        if inst.status == "Active" {
                            return HandsAction::PauseHand(inst.instance_id.clone());
                        } else if inst.status == "Paused" {
                            return HandsAction::ResumeHand(inst.instance_id.clone());
                        }
                    }
                }
            }
            KeyCode::Char('r') => return HandsAction::RefreshActive,
            _ => {}
        }
        HandsAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut HandsState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Hands ",
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
        HandsSub::Marketplace => draw_marketplace(f, chunks[2], state),
        HandsSub::Active => draw_active(f, chunks[2], state),
    }
}

fn draw_sub_tabs(f: &mut Frame, area: Rect, active: HandsSub) {
    let tabs = [
        (HandsSub::Marketplace, "1 Marketplace"),
        (HandsSub::Active, "2 Active"),
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

fn draw_marketplace(f: &mut Frame, area: Rect, state: &mut HandsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<4} {:<16} {:<14} {:<6} {}",
                "", "Name", "Category", "Ready", "Description"
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
                Span::styled("Loading hands\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.definitions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No hands available.", theme::dim_style())),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .definitions
            .iter()
            .map(|h| {
                let ready_badge = if h.requirements_met {
                    Span::styled(" Ready ", Style::default().fg(theme::GREEN))
                } else {
                    Span::styled(" Setup ", Style::default().fg(theme::YELLOW))
                };
                let category_style = match h.category.as_str() {
                    "Content" => Style::default().fg(theme::PURPLE),
                    "Security" => Style::default().fg(theme::RED),
                    "Development" => Style::default().fg(theme::BLUE),
                    "Productivity" => Style::default().fg(theme::GREEN),
                    _ => Style::default().fg(theme::CYAN),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("  {:<4}", &h.icon)),
                    Span::styled(
                        format!("{:<16}", truncate(&h.name, 15)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!("{:<14}", truncate(&h.category, 13)), category_style),
                    ready_badge,
                    Span::styled(
                        format!(" {}", truncate(&h.description, 40)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.marketplace_list);
    }

    if !state.status_msg.is_empty() {
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
                "  [\u{2191}\u{2193}] Navigate  [a/Enter] Activate  [r] Refresh",
                theme::hint_style(),
            )])),
            chunks[2],
        );
    }
}

fn draw_active(f: &mut Frame, area: Rect, state: &mut HandsState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<16} {:<10} {:<20} {}",
                "Agent", "Status", "Hand", "Since"
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
                Span::styled("Loading active hands\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.instances.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No active hands. Press [1] to browse the marketplace.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .instances
            .iter()
            .map(|i| {
                let status_style = match i.status.as_str() {
                    "Active" => Style::default().fg(theme::GREEN),
                    "Paused" => Style::default().fg(theme::YELLOW),
                    _ => Style::default().fg(theme::RED),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<16}", truncate(&i.agent_name, 15)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(format!("{:<10}", &i.status), status_style),
                    Span::styled(
                        format!("{:<20}", truncate(&i.hand_id, 19)),
                        theme::dim_style(),
                    ),
                    Span::styled(
                        truncate(&i.activated_at, 19).to_string(),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.active_list);
    }

    if state.confirm_deactivate {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  Deactivate this hand? [y] Yes  [any] Cancel",
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
                "  [\u{2191}\u{2193}] Navigate  [p] Pause/Resume  [d] Deactivate  [r] Refresh",
                theme::hint_style(),
            )])),
            chunks[2],
        );
    }
}

fn truncate(s: &str, max: usize) -> &str {
    openfang_types::truncate_str(s, max)
}
