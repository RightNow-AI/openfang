//! Triggers screen: CRUD with pattern type picker.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct TriggerInfo {
    pub id: String,
    pub agent_id: String,
    pub pattern: String,
    pub fires: u64,
    pub enabled: bool,
}

const PATTERN_TYPES: &[(&str, &str)] = &[
    ("Lifecycle", "Agent lifecycle events (start, stop, error)"),
    ("AgentSpawned", "Fires when a new agent is spawned"),
    ("ContentMatch", "Match on message content (regex)"),
    ("Schedule", "Cron-like schedule trigger"),
    ("Webhook", "HTTP webhook trigger"),
    ("ChannelMessage", "Message received on a channel"),
];

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq)]
pub enum TriggerSubScreen {
    List,
    Create,
}

pub struct TriggerState {
    pub sub: TriggerSubScreen,
    pub triggers: Vec<TriggerInfo>,
    pub list_state: ListState,
    // Create wizard
    pub create_step: usize, // 0=agent, 1=pattern_type, 2=param, 3=prompt, 4=max_fires, 5=review
    pub create_agent_id: String,
    pub create_pattern_type: usize,
    pub create_pattern_param: String,
    pub create_prompt: String,
    pub create_max_fires: String,
    pub pattern_type_list: ListState,
    pub loading: bool,
    pub tick: usize,
    pub status_msg: String,
}

pub enum TriggerAction {
    Continue,
    Refresh,
    CreateTrigger {
        agent_id: String,
        pattern_type: String,
        pattern_param: String,
        prompt: String,
        max_fires: u64,
    },
    DeleteTrigger(String),
}

impl TriggerState {
    pub fn new() -> Self {
        Self {
            sub: TriggerSubScreen::List,
            triggers: Vec::new(),
            list_state: ListState::default(),
            create_step: 0,
            create_agent_id: String::new(),
            create_pattern_type: 0,
            create_pattern_param: String::new(),
            create_prompt: String::new(),
            create_max_fires: String::new(),
            pattern_type_list: ListState::default(),
            loading: false,
            tick: 0,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> TriggerAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return TriggerAction::Continue;
        }
        match self.sub {
            TriggerSubScreen::List => self.handle_list(key),
            TriggerSubScreen::Create => self.handle_create(key),
        }
    }

    fn handle_list(&mut self, key: KeyEvent) -> TriggerAction {
        let total = self.triggers.len() + 1; // +1 for "Create new"
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list_state.selected().unwrap_or(0);
                let next = if i == 0 { total - 1 } else { i - 1 };
                self.list_state.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.list_state.selected().unwrap_or(0);
                let next = (i + 1) % total;
                self.list_state.select(Some(next));
            }
            KeyCode::Char('d') => {
                if let Some(idx) = self.list_state.selected() {
                    if idx < self.triggers.len() {
                        let id = self.triggers[idx].id.clone();
                        return TriggerAction::DeleteTrigger(id);
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = self.list_state.selected() {
                    if idx >= self.triggers.len() {
                        // "Create new"
                        self.create_step = 0;
                        self.create_agent_id.clear();
                        self.create_pattern_type = 0;
                        self.create_pattern_param.clear();
                        self.create_prompt.clear();
                        self.create_max_fires.clear();
                        self.pattern_type_list.select(Some(0));
                        self.sub = TriggerSubScreen::Create;
                    }
                }
            }
            KeyCode::Char('r') => return TriggerAction::Refresh,
            _ => {}
        }
        TriggerAction::Continue
    }

    fn handle_create(&mut self, key: KeyEvent) -> TriggerAction {
        match self.create_step {
            1 => return self.handle_pattern_picker(key),
            5 => return self.handle_review(key),
            _ => {}
        }

        match key.code {
            KeyCode::Esc => {
                if self.create_step == 0 {
                    self.sub = TriggerSubScreen::List;
                } else {
                    self.create_step -= 1;
                }
            }
            KeyCode::Enter => {
                if self.create_step < 5 {
                    self.create_step += 1;
                }
            }
            KeyCode::Char(c) => match self.create_step {
                0 => self.create_agent_id.push(c),
                2 => self.create_pattern_param.push(c),
                3 => self.create_prompt.push(c),
                4 => {
                    if c.is_ascii_digit() {
                        self.create_max_fires.push(c);
                    }
                }
                _ => {}
            },
            KeyCode::Backspace => match self.create_step {
                0 => {
                    self.create_agent_id.pop();
                }
                2 => {
                    self.create_pattern_param.pop();
                }
                3 => {
                    self.create_prompt.pop();
                }
                4 => {
                    self.create_max_fires.pop();
                }
                _ => {}
            },
            _ => {}
        }
        TriggerAction::Continue
    }

    fn handle_pattern_picker(&mut self, key: KeyEvent) -> TriggerAction {
        match key.code {
            KeyCode::Esc => {
                self.create_step = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.pattern_type_list.selected().unwrap_or(0);
                let next = if i == 0 {
                    PATTERN_TYPES.len() - 1
                } else {
                    i - 1
                };
                self.pattern_type_list.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.pattern_type_list.selected().unwrap_or(0);
                let next = (i + 1) % PATTERN_TYPES.len();
                self.pattern_type_list.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(idx) = self.pattern_type_list.selected() {
                    self.create_pattern_type = idx;
                    self.create_step = 2;
                }
            }
            _ => {}
        }
        TriggerAction::Continue
    }

    fn handle_review(&mut self, key: KeyEvent) -> TriggerAction {
        match key.code {
            KeyCode::Esc => {
                self.create_step = 4;
            }
            KeyCode::Enter => {
                let max_fires = self.create_max_fires.parse::<u64>().unwrap_or(0);
                let pattern_type = PATTERN_TYPES
                    .get(self.create_pattern_type)
                    .map(|(n, _)| n.to_string())
                    .unwrap_or_default();
                self.sub = TriggerSubScreen::List;
                return TriggerAction::CreateTrigger {
                    agent_id: self.create_agent_id.clone(),
                    pattern_type,
                    pattern_param: self.create_pattern_param.clone(),
                    prompt: self.create_prompt.clone(),
                    max_fires,
                };
            }
            _ => {}
        }
        TriggerAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut TriggerState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Triggers ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match state.sub {
        TriggerSubScreen::List => draw_list(f, inner, state),
        TriggerSubScreen::Create => draw_create(f, inner, state),
    }
}

fn draw_list(f: &mut Frame, area: Rect, state: &mut TriggerState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<14} {:<20} {:<8} {}",
                "Agent", "Pattern", "Fires", "Enabled"
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
                Span::styled("Loading triggers\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else {
        let mut items: Vec<ListItem> = state
            .triggers
            .iter()
            .map(|tr| {
                let enabled_str = if tr.enabled { "\u{2714}" } else { "\u{2718}" };
                let enabled_style = if tr.enabled {
                    Style::default().fg(theme::GREEN)
                } else {
                    Style::default().fg(theme::RED)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<14}", truncate(&tr.agent_id, 13)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<20}", truncate(&tr.pattern, 19)),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(format!(" {:<8}", tr.fires), theme::dim_style()),
                    Span::styled(format!(" {enabled_str}"), enabled_style),
                ]))
            })
            .collect();

        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  + Create new trigger",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::BOLD),
        )])));

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.list_state);
    }

    if !state.status_msg.is_empty() {
        // Overlay status msg at bottom of list area
        let msg_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y + chunks[1].height.saturating_sub(1),
            width: chunks[1].width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                format!("  {}", state.status_msg),
                Style::default().fg(theme::YELLOW),
            )),
            msg_area,
        );
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [\u{2191}\u{2193}] Navigate  [Enter] Create  [d] Delete  [r] Refresh",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_create(f: &mut Frame, area: Rect, state: &mut TriggerState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // title
        Constraint::Length(1), // separator
        Constraint::Min(6),    // content
        Constraint::Length(1), // step indicator
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Create New Trigger",
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, theme::dim_style())),
        chunks[1],
    );

    match state.create_step {
        0 => draw_text_field(
            f,
            chunks[2],
            "Agent ID:",
            &state.create_agent_id,
            "agent-uuid",
        ),
        1 => draw_pattern_picker(f, chunks[2], state),
        2 => draw_text_field(
            f,
            chunks[2],
            &format!(
                "Pattern param for {}:",
                PATTERN_TYPES
                    .get(state.create_pattern_type)
                    .map(|(n, _)| *n)
                    .unwrap_or("?")
            ),
            &state.create_pattern_param,
            "e.g. .*error.*",
        ),
        3 => draw_text_field(
            f,
            chunks[2],
            "Prompt template:",
            &state.create_prompt,
            "Handle this: {{event}}",
        ),
        4 => draw_text_field(
            f,
            chunks[2],
            "Max fires (0 = unlimited):",
            &state.create_max_fires,
            "0",
        ),
        _ => draw_trigger_review(f, chunks[2], state),
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  Step {} of 6", state.create_step + 1),
            theme::dim_style(),
        )])),
        chunks[3],
    );

    let hint_text = if state.create_step == 5 {
        "  [Enter] Create  [Esc] Back"
    } else if state.create_step == 1 {
        "  [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Back"
    } else {
        "  [Enter] Next  [Esc] Back"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            hint_text,
            theme::hint_style(),
        )])),
        chunks[4],
    );
}

fn draw_text_field(f: &mut Frame, area: Rect, label: &str, value: &str, placeholder: &str) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(format!("  {label}"))])),
        chunks[0],
    );

    let display = if value.is_empty() { placeholder } else { value };
    let style = if value.is_empty() {
        theme::dim_style()
    } else {
        theme::input_style()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  > "),
            Span::styled(display, style),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])),
        chunks[1],
    );
}

fn draw_pattern_picker(f: &mut Frame, area: Rect, state: &mut TriggerState) {
    let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw("  Select pattern type:")])),
        chunks[0],
    );

    let items: Vec<ListItem> = PATTERN_TYPES
        .iter()
        .map(|(name, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<20}", name), Style::default().fg(theme::CYAN)),
                Span::styled(*desc, theme::dim_style()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");
    f.render_stateful_widget(list, chunks[1], &mut state.pattern_type_list);
}

fn draw_trigger_review(f: &mut Frame, area: Rect, state: &TriggerState) {
    let pattern_name = PATTERN_TYPES
        .get(state.create_pattern_type)
        .map(|(n, _)| *n)
        .unwrap_or("?");
    let max_fires = if state.create_max_fires.is_empty() {
        "unlimited"
    } else {
        &state.create_max_fires
    };

    let lines = vec![
        Line::from(vec![
            Span::raw("  Agent:   "),
            Span::styled(&state.create_agent_id, Style::default().fg(theme::CYAN)),
        ]),
        Line::from(vec![
            Span::raw("  Pattern: "),
            Span::styled(pattern_name, Style::default().fg(theme::YELLOW)),
            Span::raw(format!(" ({})", state.create_pattern_param)),
        ]),
        Line::from(vec![
            Span::raw("  Prompt:  "),
            Span::styled(&state.create_prompt, theme::dim_style()),
        ]),
        Line::from(vec![
            Span::raw("  Max:     "),
            Span::styled(max_fires, Style::default().fg(theme::GREEN)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Press Enter to create this trigger.",
            theme::dim_style(),
        )]),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}
