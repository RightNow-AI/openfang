//! Templates screen: browse agent templates and spawn with one click.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TemplateInfo {
    pub name: String,
    pub description: String,
    pub category: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone)]
pub struct ProviderAuth {
    pub name: String,
    pub configured: bool,
}

// ── Built-in templates ──────────────────────────────────────────────────────

const BUILTIN_TEMPLATES: &[(&str, &str, &str, &str, &str)] = &[
    (
        "General Assistant",
        "Versatile AI assistant for everyday tasks",
        "General",
        "anthropic",
        "claude-sonnet-4-20250514",
    ),
    (
        "Code Helper",
        "Programming assistant with code review and debugging",
        "Development",
        "anthropic",
        "claude-sonnet-4-20250514",
    ),
    (
        "Researcher",
        "Deep research and analysis with web search",
        "Research",
        "anthropic",
        "claude-sonnet-4-20250514",
    ),
    (
        "Writer",
        "Creative and technical writing assistant",
        "Writing",
        "anthropic",
        "claude-sonnet-4-20250514",
    ),
    (
        "Data Analyst",
        "Data analysis, visualization, and SQL queries",
        "Development",
        "gemini",
        "gemini-2.5-flash",
    ),
    (
        "DevOps Engineer",
        "Infrastructure, CI/CD, and deployment assistance",
        "Development",
        "groq",
        "llama-3.3-70b-versatile",
    ),
    (
        "Customer Support",
        "Professional customer service agent",
        "Business",
        "groq",
        "llama-3.3-70b-versatile",
    ),
    (
        "Tutor",
        "Patient educational assistant for learning any subject",
        "General",
        "gemini",
        "gemini-2.5-flash",
    ),
    (
        "API Designer",
        "REST/GraphQL API design and documentation",
        "Development",
        "anthropic",
        "claude-sonnet-4-20250514",
    ),
    (
        "Meeting Notes",
        "Meeting transcription, summary, and action items",
        "Business",
        "groq",
        "llama-3.3-70b-versatile",
    ),
];

// ── Categories ──────────────────────────────────────────────────────────────

const CATEGORIES: &[&str] = &[
    "All",
    "General",
    "Development",
    "Research",
    "Writing",
    "Business",
];

// ── State ───────────────────────────────────────────────────────────────────

pub struct TemplatesState {
    pub templates: Vec<TemplateInfo>,
    pub providers: Vec<ProviderAuth>,
    pub category_filter: usize,
    pub filtered: Vec<usize>,
    pub list_state: ListState,
    pub loading: bool,
    pub tick: usize,
    pub status_msg: String,
}

pub enum TemplatesAction {
    Continue,
    Refresh,
    SpawnTemplate(String),
}

impl TemplatesState {
    pub fn new() -> Self {
        let templates: Vec<TemplateInfo> = BUILTIN_TEMPLATES
            .iter()
            .map(|(name, desc, cat, prov, model)| TemplateInfo {
                name: name.to_string(),
                description: desc.to_string(),
                category: cat.to_string(),
                provider: prov.to_string(),
                model: model.to_string(),
            })
            .collect();
        let filtered: Vec<usize> = (0..templates.len()).collect();
        let mut state = Self {
            templates,
            providers: Vec::new(),
            category_filter: 0,
            filtered,
            list_state: ListState::default(),
            loading: false,
            tick: 0,
            status_msg: String::new(),
        };
        state.list_state.select(Some(0));
        state
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    fn refilter(&mut self) {
        let cat = CATEGORIES[self.category_filter];
        if cat == "All" {
            self.filtered = (0..self.templates.len()).collect();
        } else {
            self.filtered = self
                .templates
                .iter()
                .enumerate()
                .filter(|(_, t)| t.category == cat)
                .map(|(i, _)| i)
                .collect();
        }
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    fn provider_configured(&self, provider: &str) -> bool {
        self.providers
            .iter()
            .any(|p| p.name == provider && p.configured)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> TemplatesAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return TemplatesAction::Continue;
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
            KeyCode::Enter => {
                if let Some(sel) = self.list_state.selected() {
                    if let Some(&idx) = self.filtered.get(sel) {
                        let t = &self.templates[idx];
                        if !self.provider_configured(&t.provider) && !self.providers.is_empty() {
                            self.status_msg = format!(
                                "Provider '{}' not configured. Set API key in Settings first.",
                                t.provider
                            );
                            return TemplatesAction::Continue;
                        }
                        return TemplatesAction::SpawnTemplate(t.name.clone());
                    }
                }
            }
            KeyCode::Char('f') => {
                self.category_filter = (self.category_filter + 1) % CATEGORIES.len();
                self.refilter();
            }
            KeyCode::Char('r') => return TemplatesAction::Refresh,
            _ => {}
        }
        TemplatesAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut TemplatesState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Templates ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(2), // header + category filter
        Constraint::Min(3),    // list
        Constraint::Length(3), // detail preview
        Constraint::Length(1), // hints
    ])
    .split(inner);

    // ── Category filter + header ──
    let active_cat = CATEGORIES[state.category_filter];
    let cat_spans: Vec<Span> = CATEGORIES
        .iter()
        .map(|&c| {
            if c == active_cat {
                Span::styled(
                    format!(" [{c}] "),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(format!(" {c} "), theme::dim_style())
            }
        })
        .collect();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(cat_spans),
            Line::from(vec![Span::styled(
                format!(
                    "  {:<22} {:<14} {:<16} {}",
                    "Template", "Category", "Provider/Model", "Description"
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
                Span::styled("Loading templates\u{2026}", theme::dim_style()),
            ])),
            chunks[1],
        );
    } else if state.filtered.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  No templates in this category.",
                theme::dim_style(),
            )),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = state
            .filtered
            .iter()
            .map(|&idx| {
                let t = &state.templates[idx];
                let configured = state.provider_configured(&t.provider);
                let auth_badge = if state.providers.is_empty() {
                    Span::raw("")
                } else if configured {
                    Span::styled(" \u{2714}", Style::default().fg(theme::GREEN))
                } else {
                    Span::styled(" \u{2718}", Style::default().fg(theme::RED))
                };
                let prov_model = format!("{}/{}", t.provider, truncate(&t.model, 12));
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<22}", truncate(&t.name, 21)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(" {:<14}", &t.category),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(
                        format!(" {:<16}", truncate(&prov_model, 15)),
                        Style::default().fg(theme::BLUE),
                    ),
                    auth_badge,
                    Span::styled(
                        format!("  {}", truncate(&t.description, 28)),
                        theme::dim_style(),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut state.list_state);
    }

    // ── Detail preview ──
    if let Some(sel) = state.list_state.selected() {
        if let Some(&idx) = state.filtered.get(sel) {
            let t = &state.templates[idx];
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![Span::styled(
                        format!("  {} ", t.name),
                        Style::default()
                            .fg(theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(&t.description, theme::dim_style()),
                    ]),
                    Line::from(vec![Span::styled(
                        format!("  Provider: {}/{}  ", t.provider, t.model),
                        Style::default().fg(theme::BLUE),
                    )]),
                ]),
                chunks[2],
            );
        }
    }

    // ── Hints / status ──
    if !state.status_msg.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!("  {}", state.status_msg),
                Style::default().fg(theme::YELLOW),
            )])),
            chunks[3],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  [\u{2191}\u{2193}] Navigate  [Enter] Spawn Agent  [f] Filter Category  [r] Refresh",
                theme::hint_style(),
            )])),
            chunks[3],
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}
