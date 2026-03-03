//! Comms screen: Agent communication topology + live event feed.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct CommsNode {
    pub id: String,
    pub name: String,
    pub state: String,
    pub model: String,
}

#[derive(Clone, Default)]
pub struct CommsEdge {
    pub from: String,
    pub to: String,
    pub kind: String, // "parent_child" or "peer"
}

#[derive(Clone, Default)]
pub struct CommsEventItem {
    /// Event ID — used by the dashboard for dedup, kept for wire compat.
    #[allow(dead_code)]
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub source_name: String,
    pub target_name: String,
    pub detail: String,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommsFocus {
    Topology,
    EventList,
}

pub struct CommsState {
    pub nodes: Vec<CommsNode>,
    pub edges: Vec<CommsEdge>,
    pub events: Vec<CommsEventItem>,
    pub event_list_state: ListState,
    pub focus: CommsFocus,
    pub loading: bool,
    pub tick: usize,
    pub poll_tick: usize,
    // Send modal
    pub show_send_modal: bool,
    pub send_from: String,
    pub send_to: String,
    pub send_msg: String,
    pub send_field: usize,
    // Task modal
    pub show_task_modal: bool,
    pub task_title: String,
    pub task_desc: String,
    pub task_assign: String,
    pub task_field: usize,
    // Status
    pub status_msg: String,
}

pub enum CommsAction {
    Continue,
    Refresh,
    SendMessage {
        from: String,
        to: String,
        msg: String,
    },
    PostTask {
        title: String,
        desc: String,
        assign: String,
    },
}

impl CommsState {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            events: Vec::new(),
            event_list_state: ListState::default(),
            focus: CommsFocus::Topology,
            loading: false,
            tick: 0,
            poll_tick: 0,
            show_send_modal: false,
            send_from: String::new(),
            send_to: String::new(),
            send_msg: String::new(),
            send_field: 0,
            show_task_modal: false,
            task_title: String::new(),
            task_desc: String::new(),
            task_assign: String::new(),
            task_field: 0,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.poll_tick = self.poll_tick.wrapping_add(1);
    }

    /// Auto-refresh every ~5s at 20fps tick rate.
    pub fn should_poll(&self) -> bool {
        self.poll_tick > 0 && self.poll_tick.is_multiple_of(100)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> CommsAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return CommsAction::Continue;
        }

        // Modal key handling
        if self.show_send_modal {
            return self.handle_send_modal_key(key);
        }
        if self.show_task_modal {
            return self.handle_task_modal_key(key);
        }

        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    CommsFocus::Topology => CommsFocus::EventList,
                    CommsFocus::EventList => CommsFocus::Topology,
                };
            }
            KeyCode::Char('s') => {
                self.show_send_modal = true;
                self.send_from.clear();
                self.send_to.clear();
                self.send_msg.clear();
                self.send_field = 0;
            }
            KeyCode::Char('t') => {
                self.show_task_modal = true;
                self.task_title.clear();
                self.task_desc.clear();
                self.task_assign.clear();
                self.task_field = 0;
            }
            KeyCode::Char('r') => return CommsAction::Refresh,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focus == CommsFocus::EventList && !self.events.is_empty() {
                    let i = self.event_list_state.selected().unwrap_or(0);
                    let next = if i == 0 {
                        self.events.len() - 1
                    } else {
                        i - 1
                    };
                    self.event_list_state.select(Some(next));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.focus == CommsFocus::EventList && !self.events.is_empty() {
                    let i = self.event_list_state.selected().unwrap_or(0);
                    let next = (i + 1) % self.events.len();
                    self.event_list_state.select(Some(next));
                }
            }
            _ => {}
        }
        CommsAction::Continue
    }

    fn handle_send_modal_key(&mut self, key: KeyEvent) -> CommsAction {
        match key.code {
            KeyCode::Esc => {
                self.show_send_modal = false;
            }
            KeyCode::Tab => {
                self.send_field = (self.send_field + 1) % 3;
            }
            KeyCode::BackTab => {
                self.send_field = if self.send_field == 0 {
                    2
                } else {
                    self.send_field - 1
                };
            }
            KeyCode::Enter => {
                if !self.send_from.is_empty()
                    && !self.send_to.is_empty()
                    && !self.send_msg.is_empty()
                {
                    self.show_send_modal = false;
                    return CommsAction::SendMessage {
                        from: self.send_from.clone(),
                        to: self.send_to.clone(),
                        msg: self.send_msg.clone(),
                    };
                }
            }
            KeyCode::Char(c) => match self.send_field {
                0 => self.send_from.push(c),
                1 => self.send_to.push(c),
                _ => self.send_msg.push(c),
            },
            KeyCode::Backspace => match self.send_field {
                0 => {
                    self.send_from.pop();
                }
                1 => {
                    self.send_to.pop();
                }
                _ => {
                    self.send_msg.pop();
                }
            },
            _ => {}
        }
        CommsAction::Continue
    }

    fn handle_task_modal_key(&mut self, key: KeyEvent) -> CommsAction {
        match key.code {
            KeyCode::Esc => {
                self.show_task_modal = false;
            }
            KeyCode::Tab => {
                self.task_field = (self.task_field + 1) % 3;
            }
            KeyCode::BackTab => {
                self.task_field = if self.task_field == 0 {
                    2
                } else {
                    self.task_field - 1
                };
            }
            KeyCode::Enter => {
                if !self.task_title.is_empty() {
                    self.show_task_modal = false;
                    return CommsAction::PostTask {
                        title: self.task_title.clone(),
                        desc: self.task_desc.clone(),
                        assign: self.task_assign.clone(),
                    };
                }
            }
            KeyCode::Char(c) => match self.task_field {
                0 => self.task_title.push(c),
                1 => self.task_desc.push(c),
                _ => self.task_assign.push(c),
            },
            KeyCode::Backspace => match self.task_field {
                0 => {
                    self.task_title.pop();
                }
                1 => {
                    self.task_desc.pop();
                }
                _ => {
                    self.task_assign.pop();
                }
            },
            _ => {}
        }
        CommsAction::Continue
    }

    // ── Topology helpers ─────────────────────────────────────────────────────

    fn root_nodes(&self) -> Vec<&CommsNode> {
        let child_ids: std::collections::HashSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.kind == "parent_child")
            .map(|e| e.to.as_str())
            .collect();
        self.nodes
            .iter()
            .filter(|n| !child_ids.contains(n.id.as_str()))
            .collect()
    }

    fn children_of(&self, id: &str) -> Vec<&CommsNode> {
        let child_ids: Vec<&str> = self
            .edges
            .iter()
            .filter(|e| e.kind == "parent_child" && e.from == id)
            .map(|e| e.to.as_str())
            .collect();
        self.nodes
            .iter()
            .filter(|n| child_ids.contains(&n.id.as_str()))
            .collect()
    }

    fn peers_of(&self, id: &str) -> Vec<&CommsNode> {
        let peer_ids: std::collections::HashSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.kind == "peer")
            .filter_map(|e| {
                if e.from == id {
                    Some(e.to.as_str())
                } else if e.to == id {
                    Some(e.from.as_str())
                } else {
                    None
                }
            })
            .collect();
        self.nodes
            .iter()
            .filter(|n| peer_ids.contains(n.id.as_str()))
            .collect()
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut CommsState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Comms ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(2),  // header
        Constraint::Length(1),  // separator
        Constraint::Percentage(35), // topology
        Constraint::Length(1),  // separator
        Constraint::Min(4),     // event list
        Constraint::Length(1),  // hints
    ])
    .split(inner);

    // Header
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                format!(
                    "  Agent Topology  ({} agents, {} edges)",
                    state.nodes.len(),
                    state.edges.len()
                ),
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
        ]),
        chunks[0],
    );

    // Separator
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            theme::dim_style(),
        ))),
        chunks[1],
    );

    // Topology tree
    draw_topology(f, chunks[2], state);

    // Separator
    let event_label = if state.focus == CommsFocus::EventList {
        "  \u{25b6} Live Event Feed"
    } else {
        "    Live Event Feed"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                event_label,
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} events)", state.events.len()),
                theme::dim_style(),
            ),
        ])),
        chunks[3],
    );

    // Event list
    draw_event_list(f, chunks[4], state);

    // Status message or hints
    let hint_text = if !state.status_msg.is_empty() {
        format!(
            "  {} | [s]end  [t]ask  [r]efresh  [Tab] focus  [\u{2191}\u{2193}] scroll",
            state.status_msg
        )
    } else {
        "  [s]end  [t]ask  [r]efresh  [Tab] focus  [\u{2191}\u{2193}] scroll".to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hint_text, theme::hint_style()))),
        chunks[5],
    );

    // Modal overlays
    if state.show_send_modal {
        draw_send_modal(f, area, state);
    }
    if state.show_task_modal {
        draw_task_modal(f, area, state);
    }
}

fn draw_topology(f: &mut Frame, area: Rect, state: &CommsState) {
    if state.loading && state.nodes.is_empty() {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading topology\u{2026}", theme::dim_style()),
            ])),
            area,
        );
        return;
    }

    if state.nodes.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No agents running.",
                theme::dim_style(),
            )),
            area,
        );
        return;
    }

    let focus_highlight = state.focus == CommsFocus::Topology;
    let mut lines = Vec::new();

    for root in state.root_nodes() {
        let state_style = state_color(&root.state);
        let mut spans = vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("[{}]", &root.state), state_style),
            Span::styled(
                format!(" {} ", root.name),
                Style::default()
                    .fg(if focus_highlight {
                        theme::CYAN
                    } else {
                        theme::TEXT
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("({})", root.model), theme::dim_style()),
        ];
        // Peer annotations
        for peer in state.peers_of(&root.id) {
            spans.push(Span::styled(
                format!("  \u{2194} {}", peer.name),
                Style::default().fg(theme::PURPLE),
            ));
        }
        lines.push(Line::from(spans));

        // Children
        let children = state.children_of(&root.id);
        for (i, child) in children.iter().enumerate() {
            let branch = if i < children.len() - 1 {
                "\u{251c}\u{2500}\u{2500} "
            } else {
                "\u{2514}\u{2500}\u{2500} "
            };
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(branch, theme::dim_style()),
                Span::styled(format!("[{}]", child.state), state_color(&child.state)),
                Span::styled(format!(" {} ", child.name), Style::default().fg(theme::TEXT)),
                Span::styled(format!("({})", child.model), theme::dim_style()),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_event_list(f: &mut Frame, area: Rect, state: &mut CommsState) {
    if state.events.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No inter-agent events yet.",
                theme::dim_style(),
            )),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = state
        .events
        .iter()
        .map(|ev| {
            let kind_style = kind_color(&ev.kind);
            let kind_label = kind_short(&ev.kind);
            let target_part = if ev.target_name.is_empty() {
                String::new()
            } else {
                format!(" \u{2192} {}", ev.target_name)
            };
            let detail = truncate(&ev.detail, 50);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<8}", short_time(&ev.timestamp)),
                    theme::dim_style(),
                ),
                Span::styled(format!(" {:<10}", kind_label), kind_style),
                Span::styled(
                    format!(" {}", ev.source_name),
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(target_part, Style::default().fg(theme::PURPLE)),
                Span::styled(format!("  {detail}"), theme::dim_style()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");
    f.render_stateful_widget(list, area, &mut state.event_list_state);
}

fn draw_send_modal(f: &mut Frame, area: Rect, state: &CommsState) {
    let modal = centered_rect(50, 12, area);
    f.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(" Send Message ", theme::title_style()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::uniform(1));
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let field_style = |idx: usize| {
        if state.send_field == idx {
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::dim_style()
        }
    };

    f.render_widget(
        Paragraph::new(Span::styled("From (agent ID):", field_style(0))),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.send_from),
            Style::default().fg(theme::TEXT),
        )),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(Span::styled("To (agent ID):", field_style(1))),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.send_to),
            Style::default().fg(theme::TEXT),
        )),
        rows[3],
    );
    f.render_widget(
        Paragraph::new(Span::styled("Message:", field_style(2))),
        rows[4],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.send_msg),
            Style::default().fg(theme::TEXT),
        )),
        rows[5],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            "[Tab] field  [Enter] send  [Esc] cancel",
            theme::hint_style(),
        )),
        rows[6],
    );
}

fn draw_task_modal(f: &mut Frame, area: Rect, state: &CommsState) {
    let modal = centered_rect(50, 12, area);
    f.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(" Post Task ", theme::title_style()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::uniform(1));
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let field_style = |idx: usize| {
        if state.task_field == idx {
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::dim_style()
        }
    };

    f.render_widget(
        Paragraph::new(Span::styled("Title:", field_style(0))),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.task_title),
            Style::default().fg(theme::TEXT),
        )),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(Span::styled("Description:", field_style(1))),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.task_desc),
            Style::default().fg(theme::TEXT),
        )),
        rows[3],
    );
    f.render_widget(
        Paragraph::new(Span::styled("Assign to (agent ID, optional):", field_style(2))),
        rows[4],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}\u{2588}", &state.task_assign),
            Style::default().fg(theme::TEXT),
        )),
        rows[5],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            "[Tab] field  [Enter] post  [Esc] cancel",
            theme::hint_style(),
        )),
        rows[6],
    );
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn state_color(state: &str) -> Style {
    match state {
        "Running" => Style::default().fg(theme::GREEN),
        "Suspended" => Style::default().fg(theme::YELLOW),
        "Terminated" | "Crashed" => Style::default().fg(theme::RED),
        _ => theme::dim_style(),
    }
}

fn kind_color(kind: &str) -> Style {
    match kind {
        "agent_message" => Style::default().fg(theme::CYAN),
        "agent_spawned" => Style::default().fg(theme::GREEN),
        "agent_terminated" => Style::default().fg(theme::RED),
        "task_posted" => Style::default().fg(theme::YELLOW),
        "task_claimed" => Style::default().fg(theme::CYAN),
        "task_completed" => Style::default().fg(theme::GREEN),
        _ => theme::dim_style(),
    }
}

fn kind_short(kind: &str) -> &str {
    match kind {
        "agent_message" => "MSG",
        "agent_spawned" => "SPAWNED",
        "agent_terminated" => "KILLED",
        "task_posted" => "TASK+",
        "task_claimed" => "CLAIM",
        "task_completed" => "DONE",
        _ => kind,
    }
}

fn short_time(ts: &str) -> String {
    // Extract HH:MM:SS from ISO-8601
    if let Some(t_pos) = ts.find('T') {
        let time_part = &ts[t_pos + 1..];
        if time_part.len() >= 8 {
            return time_part[..8].to_string();
        }
    }
    ts.chars().take(8).collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!(
            "{}\u{2026}",
            openfang_types::truncate_str(s, max.saturating_sub(1))
        )
    }
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}
