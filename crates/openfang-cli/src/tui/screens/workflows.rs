//! Workflows screen: CRUD, run input, run history.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct WorkflowInfo {
    pub id: String,
    pub name: String,
    pub steps: usize,
    pub created: String,
}

#[derive(Clone, Default)]
pub struct WorkflowRun {
    pub id: String,
    pub state: String,
    pub duration: String,
    pub output_preview: String,
}

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq)]
pub enum WorkflowSubScreen {
    List,
    Runs,
    Create,
    RunInput,
    RunResult,
}

pub struct WorkflowState {
    pub sub: WorkflowSubScreen,
    pub workflows: Vec<WorkflowInfo>,
    pub list_state: ListState,
    pub selected_workflow: Option<usize>,
    // Run history
    pub runs: Vec<WorkflowRun>,
    pub runs_list_state: ListState,
    // Create wizard
    pub create_step: usize, // 0=name, 1=desc, 2=steps_json, 3=review
    pub create_name: String,
    pub create_desc: String,
    pub create_steps: String,
    // Run
    pub run_input: String,
    pub run_result: Option<String>,
    pub loading: bool,
    pub tick: usize,
    pub status_msg: String,
}

pub enum WorkflowAction {
    Continue,
    Refresh,
    LoadRuns(String),
    CreateWorkflow {
        name: String,
        description: String,
        steps_json: String,
    },
    RunWorkflow {
        id: String,
        input: String,
    },
}

impl WorkflowState {
    pub fn new() -> Self {
        Self {
            sub: WorkflowSubScreen::List,
            workflows: Vec::new(),
            list_state: ListState::default(),
            selected_workflow: None,
            runs: Vec::new(),
            runs_list_state: ListState::default(),
            create_step: 0,
            create_name: String::new(),
            create_desc: String::new(),
            create_steps: String::new(),
            run_input: String::new(),
            run_result: None,
            loading: false,
            tick: 0,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> WorkflowAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return WorkflowAction::Continue;
        }
        match self.sub {
            WorkflowSubScreen::List => self.handle_list(key),
            WorkflowSubScreen::Runs => self.handle_runs(key),
            WorkflowSubScreen::Create => self.handle_create(key),
            WorkflowSubScreen::RunInput => self.handle_run_input(key),
            WorkflowSubScreen::RunResult => self.handle_run_result(key),
        }
    }

    fn handle_list(&mut self, key: KeyEvent) -> WorkflowAction {
        let total = self.workflows.len() + 1; // +1 for "Create new"
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
            KeyCode::Enter => {
                if let Some(idx) = self.list_state.selected() {
                    if idx < self.workflows.len() {
                        self.selected_workflow = Some(idx);
                        let wf_id = self.workflows[idx].id.clone();
                        self.runs_list_state.select(Some(0));
                        self.sub = WorkflowSubScreen::Runs;
                        return WorkflowAction::LoadRuns(wf_id);
                    } else {
                        // "Create new"
                        self.create_step = 0;
                        self.create_name.clear();
                        self.create_desc.clear();
                        self.create_steps.clear();
                        self.sub = WorkflowSubScreen::Create;
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(idx) = self.list_state.selected() {
                    if idx < self.workflows.len() {
                        self.selected_workflow = Some(idx);
                        self.run_input.clear();
                        self.run_result = None;
                        self.sub = WorkflowSubScreen::RunInput;
                    }
                }
            }
            KeyCode::Char('r') => return WorkflowAction::Refresh,
            _ => {}
        }
        WorkflowAction::Continue
    }

    fn handle_runs(&mut self, key: KeyEvent) -> WorkflowAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = WorkflowSubScreen::List;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.runs_list_state.selected().unwrap_or(0);
                let next = if i == 0 {
                    self.runs.len().saturating_sub(1)
                } else {
                    i - 1
                };
                self.runs_list_state.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.runs_list_state.selected().unwrap_or(0);
                let total = self.runs.len().max(1);
                let next = (i + 1) % total;
                self.runs_list_state.select(Some(next));
            }
            KeyCode::Char('r') => {
                if let Some(idx) = self.selected_workflow {
                    if idx < self.workflows.len() {
                        let wf_id = self.workflows[idx].id.clone();
                        return WorkflowAction::LoadRuns(wf_id);
                    }
                }
            }
            _ => {}
        }
        WorkflowAction::Continue
    }

    fn handle_create(&mut self, key: KeyEvent) -> WorkflowAction {
        match key.code {
            KeyCode::Esc => {
                if self.create_step == 0 {
                    self.sub = WorkflowSubScreen::List;
                } else {
                    self.create_step -= 1;
                }
            }
            KeyCode::Enter => {
                if self.create_step < 3 {
                    self.create_step += 1;
                } else {
                    // Submit
                    let action = WorkflowAction::CreateWorkflow {
                        name: self.create_name.clone(),
                        description: self.create_desc.clone(),
                        steps_json: self.create_steps.clone(),
                    };
                    self.sub = WorkflowSubScreen::List;
                    return action;
                }
            }
            KeyCode::Char(c) => match self.create_step {
                0 => self.create_name.push(c),
                1 => self.create_desc.push(c),
                2 => self.create_steps.push(c),
                _ => {}
            },
            KeyCode::Backspace => match self.create_step {
                0 => {
                    self.create_name.pop();
                }
                1 => {
                    self.create_desc.pop();
                }
                2 => {
                    self.create_steps.pop();
                }
                _ => {}
            },
            _ => {}
        }
        WorkflowAction::Continue
    }

    fn handle_run_input(&mut self, key: KeyEvent) -> WorkflowAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = WorkflowSubScreen::List;
            }
            KeyCode::Enter => {
                if let Some(idx) = self.selected_workflow {
                    if idx < self.workflows.len() {
                        let wf_id = self.workflows[idx].id.clone();
                        let input = self.run_input.clone();
                        self.loading = true;
                        self.sub = WorkflowSubScreen::RunResult;
                        return WorkflowAction::RunWorkflow { id: wf_id, input };
                    }
                }
            }
            KeyCode::Char(c) => {
                self.run_input.push(c);
            }
            KeyCode::Backspace => {
                self.run_input.pop();
            }
            _ => {}
        }
        WorkflowAction::Continue
    }

    fn handle_run_result(&mut self, key: KeyEvent) -> WorkflowAction {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.sub = WorkflowSubScreen::List;
                self.loading = false;
            }
            _ => {}
        }
        WorkflowAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut WorkflowState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Workflows ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match state.sub {
        WorkflowSubScreen::List => draw_list(f, inner, state),
        WorkflowSubScreen::Runs => draw_runs(f, inner, state),
        WorkflowSubScreen::Create => draw_create(f, inner, state),
        WorkflowSubScreen::RunInput => draw_run_input(f, inner, state),
        WorkflowSubScreen::RunResult => draw_run_result(f, inner, state),
    }
}

fn draw_list(f: &mut Frame, area: Rect, state: &mut WorkflowState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {:<12} {:<24} {:<8} {}", "ID", "Name", "Steps", "Created"),
            theme::table_header(),
        )])),
        chunks[0],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading workflows\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else {
        let mut items: Vec<ListItem> = state
            .workflows
            .iter()
            .map(|wf| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<12}", truncate(&wf.id, 11)),
                        theme::dim_style(),
                    ),
                    Span::styled(
                        format!(" {:<24}", truncate(&wf.name, 23)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<8}", wf.steps),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(format!(" {}", wf.created), theme::dim_style()),
                ]))
            })
            .collect();

        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  + Create new workflow",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::BOLD),
        )])));

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.list_state);
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [\u{2191}\u{2193}] Navigate  [Enter] View runs  [x] Run  [r] Refresh",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_runs(f: &mut Frame, area: Rect, state: &mut WorkflowState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // title
        Constraint::Length(2), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    let wf_name = state
        .selected_workflow
        .and_then(|i| state.workflows.get(i))
        .map(|w| w.name.as_str())
        .unwrap_or("?");

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  Runs for: {wf_name}"),
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<12} {:<12} {:<12} {}",
                "Run ID", "State", "Duration", "Output"
            ),
            theme::table_header(),
        )])),
        chunks[1],
    );

    if state.runs.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No runs yet. Press [x] from the list to run.",
                theme::dim_style(),
            )),
            chunks[2],
        );
    } else {
        let items: Vec<ListItem> = state
            .runs
            .iter()
            .map(|run| {
                let (badge, badge_style) = theme::state_badge(&run.state);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<12}", truncate(&run.id, 11)),
                        theme::dim_style(),
                    ),
                    Span::styled(format!(" {:<12}", badge), badge_style),
                    Span::styled(
                        format!(" {:<12}", run.duration),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(
                        format!(" {}", truncate(&run.output_preview, 30)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[2], &mut state.runs_list_state);
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [\u{2191}\u{2193}] Navigate  [r] Refresh  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[3]);
}

fn draw_create(f: &mut Frame, area: Rect, state: &WorkflowState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // title
        Constraint::Length(1), // separator
        Constraint::Length(2), // field label
        Constraint::Length(1), // input
        Constraint::Length(1), // step indicator
        Constraint::Min(0),
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Create New Workflow",
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

    let (label, value, placeholder) = match state.create_step {
        0 => ("Workflow name:", &state.create_name, "my-workflow"),
        1 => (
            "Description:",
            &state.create_desc,
            "What this workflow does",
        ),
        2 => (
            "Steps (JSON array):",
            &state.create_steps,
            "[{\"action\":\"...\"}]",
        ),
        _ => (
            "Review \u{2014} press Enter to create",
            &state.create_name,
            "",
        ),
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(format!("  {label}"))])),
        chunks[2],
    );

    if state.create_step < 3 {
        let display = if value.is_empty() {
            placeholder
        } else {
            value.as_str()
        };
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
            chunks[3],
        );
    } else {
        // Review
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::raw("  Name: "),
                    Span::styled(&state.create_name, Style::default().fg(theme::CYAN)),
                ]),
                Line::from(vec![
                    Span::raw("  Desc: "),
                    Span::styled(&state.create_desc, theme::dim_style()),
                ]),
            ]),
            chunks[3],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  Step {} of 4", state.create_step + 1),
            theme::dim_style(),
        )])),
        chunks[4],
    );

    let hint_text = if state.create_step == 3 {
        "  [Enter] Create  [Esc] Back"
    } else {
        "  [Enter] Next  [Esc] Back"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            hint_text,
            theme::hint_style(),
        )])),
        chunks[6],
    );
}

fn draw_run_input(f: &mut Frame, area: Rect, state: &WorkflowState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let wf_name = state
        .selected_workflow
        .and_then(|i| state.workflows.get(i))
        .map(|w| w.name.as_str())
        .unwrap_or("?");

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  Run: {wf_name}"),
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

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw("  Input (JSON or text):")])),
        chunks[2],
    );

    let display = if state.run_input.is_empty() {
        "enter workflow input..."
    } else {
        &state.run_input
    };
    let style = if state.run_input.is_empty() {
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
        chunks[3],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Enter] Run  [Esc] Cancel",
            theme::hint_style(),
        )])),
        chunks[5],
    );
}

fn draw_run_result(f: &mut Frame, area: Rect, state: &WorkflowState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Workflow Run Result",
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Running workflow\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if let Some(ref result) = state.run_result {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                    Span::raw("Complete"),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!("  {result}"),
                    Style::default().fg(theme::CYAN),
                )]),
            ]),
            chunks[1],
        );
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  No result.", theme::dim_style())),
            chunks[1],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Enter/Esc] Back",
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
