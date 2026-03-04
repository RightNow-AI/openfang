//! Interactive launcher — lightweight Ratatui one-shot menu.
//!
//! Shown when `openfang` is run with no subcommand in a TTY.
//! Full-width left-aligned layout, adapts for first-time vs returning users.

use ratatui::crossterm::event::{self, Event as CtEvent, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::tui::theme;
use crate::ui;
use std::path::PathBuf;
use std::time::Duration;

// ── Provider detection ──────────────────────────────────────────────────────

const PROVIDER_ENV_VARS: &[(&str, &str)] = &[
    ("ANTHROPIC_API_KEY", "Anthropic"),
    ("OPENAI_API_KEY", "OpenAI"),
    ("DEEPSEEK_API_KEY", "DeepSeek"),
    ("GEMINI_API_KEY", "Gemini"),
    ("GOOGLE_API_KEY", "Gemini"),
    ("GROQ_API_KEY", "Groq"),
    ("OPENROUTER_API_KEY", "OpenRouter"),
    ("TOGETHER_API_KEY", "Together"),
    ("MISTRAL_API_KEY", "Mistral"),
    ("FIREWORKS_API_KEY", "Fireworks"),
];

fn detect_provider() -> Option<(&'static str, &'static str)> {
    for &(var, name) in PROVIDER_ENV_VARS {
        if std::env::var(var).is_ok() {
            return Some((name, var));
        }
    }
    None
}

fn is_first_run() -> bool {
    let of_home = if let Ok(h) = std::env::var("OPENFANG_HOME") {
        std::path::PathBuf::from(h)
    } else {
        match dirs::home_dir() {
            Some(h) => h.join(".openfang"),
            None => return true,
        }
    };
    !of_home.join("config.toml").exists()
}

fn has_openclaw() -> bool {
    // Quick check: does ~/.openclaw exist?
    dirs::home_dir()
        .map(|h| h.join(".openclaw").exists())
        .unwrap_or(false)
}

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LauncherChoice {
    GetStarted,
    Chat,
    Dashboard,
    DesktopApp,
    TerminalUI,
    ShowHelp,
    Quit,
}

struct MenuItem {
    label: &'static str,
    hint: &'static str,
    choice: LauncherChoice,
}

// Menu for first-time users: "Get started" is first and prominent
const MENU_FIRST_RUN: &[MenuItem] = &[
    MenuItem {
        label: "Get started",
        hint: "Providers, API keys, models, migration",
        choice: LauncherChoice::GetStarted,
    },
    MenuItem {
        label: "Chat with an agent",
        hint: "Quick chat in the terminal",
        choice: LauncherChoice::Chat,
    },
    MenuItem {
        label: "Open dashboard",
        hint: "Launch the web UI in your browser",
        choice: LauncherChoice::Dashboard,
    },
    MenuItem {
        label: "Open desktop app",
        hint: "Launch the native desktop app",
        choice: LauncherChoice::DesktopApp,
    },
    MenuItem {
        label: "Launch terminal UI",
        hint: "Full interactive TUI dashboard",
        choice: LauncherChoice::TerminalUI,
    },
    MenuItem {
        label: "Show all commands",
        hint: "Print full --help output",
        choice: LauncherChoice::ShowHelp,
    },
];

// Menu for returning users: action-first, setup at the bottom
const MENU_RETURNING: &[MenuItem] = &[
    MenuItem {
        label: "Chat with an agent",
        hint: "Quick chat in the terminal",
        choice: LauncherChoice::Chat,
    },
    MenuItem {
        label: "Open dashboard",
        hint: "Launch the web UI in your browser",
        choice: LauncherChoice::Dashboard,
    },
    MenuItem {
        label: "Launch terminal UI",
        hint: "Full interactive TUI dashboard",
        choice: LauncherChoice::TerminalUI,
    },
    MenuItem {
        label: "Open desktop app",
        hint: "Launch the native desktop app",
        choice: LauncherChoice::DesktopApp,
    },
    MenuItem {
        label: "Settings",
        hint: "Providers, API keys, models, routing",
        choice: LauncherChoice::GetStarted,
    },
    MenuItem {
        label: "Show all commands",
        hint: "Print full --help output",
        choice: LauncherChoice::ShowHelp,
    },
];

// ── Launcher state ──────────────────────────────────────────────────────────

struct LauncherState {
    list: ListState,
    daemon_url: Option<String>,
    daemon_agents: u64,
    detecting: bool,
    tick: usize,
    first_run: bool,
    openclaw_detected: bool,
}

impl LauncherState {
    fn new() -> Self {
        let first_run = is_first_run();
        let openclaw_detected = first_run && has_openclaw();
        let mut list = ListState::default();
        list.select(Some(0));
        Self {
            list,
            daemon_url: None,
            daemon_agents: 0,
            detecting: true,
            tick: 0,
            first_run,
            openclaw_detected,
        }
    }

    fn menu(&self) -> &'static [MenuItem] {
        if self.first_run {
            MENU_FIRST_RUN
        } else {
            MENU_RETURNING
        }
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn run(_config: Option<PathBuf>) -> LauncherChoice {
    let mut terminal = ratatui::init();

    // Panic hook: restore terminal on panic (set AFTER init succeeds)
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::try_restore();
        original_hook(info);
    }));

    let mut state = LauncherState::new();

    // Spawn background daemon detection (catch_unwind protects against thread panics)
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(|| {
            let result = crate::find_daemon();
            let agent_count = result.as_ref().map_or(0, |base| {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .ok();
                client
                    .and_then(|c| c.get(format!("{base}/api/agents")).send().ok())
                    .and_then(|r| r.json::<serde_json::Value>().ok())
                    .and_then(|v| v.as_array().map(|a| a.len() as u64))
                    .unwrap_or(0)
            });
            let _ = daemon_tx.send((result, agent_count));
        });
    });

    let choice;

    loop {
        // Check for daemon detection result
        if state.detecting {
            if let Ok((url, agents)) = daemon_rx.try_recv() {
                state.daemon_url = url;
                state.daemon_agents = agents;
                state.detecting = false;
            }
        }

        state.tick = state.tick.wrapping_add(1);

        // Draw (gracefully handle render failures)
        if terminal.draw(|frame| draw(frame, &mut state)).is_err() {
            choice = LauncherChoice::Quit;
            break;
        }

        // Poll for input (50ms = 20fps spinner)
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(CtEvent::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let menu = state.menu();
                if menu.is_empty() {
                    choice = LauncherChoice::Quit;
                    break;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        choice = LauncherChoice::Quit;
                        break;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = state.list.selected().unwrap_or(0);
                        let next = if i == 0 { menu.len() - 1 } else { i - 1 };
                        state.list.select(Some(next));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = state.list.selected().unwrap_or(0);
                        let next = (i + 1) % menu.len();
                        state.list.select(Some(next));
                    }
                    KeyCode::Enter => {
                        if let Some(i) = state.list.selected() {
                            if i < menu.len() {
                                choice = menu[i].choice;
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = ratatui::try_restore();
    choice
}

// ── Drawing ─────────────────────────────────────────────────────────────────

/// Left margin for content alignment.
const MARGIN_LEFT: u16 = 3;

/// Constrain content to a readable area within the terminal.
fn content_area(area: Rect) -> Rect {
    if area.width < 10 || area.height < 5 {
        // Terminal too small — use full area with no margin
        return area;
    }
    let margin = MARGIN_LEFT.min(area.width.saturating_sub(10));
    let w = 80u16.min(area.width.saturating_sub(margin));
    Rect {
        x: area.x.saturating_add(margin),
        y: area.y,
        width: w,
        height: area.height,
    }
}

fn draw(frame: &mut ratatui::Frame, state: &mut LauncherState) {
    let area = frame.area();

    // Fill background
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_PRIMARY)),
        area,
    );

    let content = content_area(area);
    let version = env!("CARGO_PKG_VERSION");
    let has_provider = detect_provider().is_some();
    let menu = state.menu();

    // Compute dynamic heights
    let header_h: u16 = if state.first_run { 3 } else { 1 }; // welcome text or just title
    let status_h: u16 = if state.detecting {
        1
    } else if has_provider {
        2
    } else {
        3
    };
    let migration_hint_h: u16 = if state.first_run && state.openclaw_detected {
        2
    } else {
        0
    };
    let menu_h = menu.len() as u16;

    let total_needed = 1 + header_h + 1 + status_h + 1 + menu_h + migration_hint_h + 1;

    // Vertical centering: place content block in the upper-third area
    let top_pad = if area.height > total_needed + 2 {
        ((area.height - total_needed) / 3).max(1)
    } else {
        1
    };

    let chunks = Layout::vertical([
        Constraint::Length(top_pad),          // top space
        Constraint::Length(header_h),         // header / welcome
        Constraint::Length(1),                // separator
        Constraint::Length(status_h),         // status indicators
        Constraint::Length(1),                // separator
        Constraint::Length(menu_h),           // menu items
        Constraint::Length(migration_hint_h), // openclaw migration hint (if any)
        Constraint::Length(1),                // keybind hints
        Constraint::Min(0),                   // remaining space
    ])
    .split(content);

    // ── Header ──────────────────────────────────────────────────────────────
    if state.first_run {
        let header_lines = vec![
            Line::from(vec![
                Span::styled(
                    "OpenFang",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  v{version}"),
                    Style::default().fg(theme::TEXT_TERTIARY),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Welcome! Let's get you set up.",
                Style::default().fg(theme::TEXT_PRIMARY),
            )]),
        ];
        frame.render_widget(Paragraph::new(header_lines), chunks[1]);
    } else {
        let header = Line::from(vec![
            Span::styled(
                "OpenFang",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  v{version}"),
                Style::default().fg(theme::TEXT_TERTIARY),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[1]);
    }

    // ── Separator ───────────────────────────────────────────────────────────
    render_separator(frame, chunks[2]);

    // ── Status block ────────────────────────────────────────────────────────
    if state.detecting {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        let line = Line::from(vec![
            Span::styled(format!("{spinner} "), Style::default().fg(theme::YELLOW)),
            Span::styled("Checking for daemon\u{2026}", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(line), chunks[3]);
    } else {
        let mut lines: Vec<Line> = Vec::new();

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
            lines.push(Line::from(vec![
                Span::styled(
                    "\u{25cf} ",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("Daemon running at {url}"),
                    Style::default().fg(theme::TEXT_PRIMARY),
                ),
                Span::styled(agent_suffix, Style::default().fg(theme::GREEN)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("\u{25cb} ", theme::dim_style()),
                Span::styled("No daemon running", theme::dim_style()),
            ]));
        }

        // Provider status
        if let Some((provider, env_var)) = detect_provider() {
            lines.push(Line::from(vec![
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
            lines.push(Line::from(vec![
                Span::styled("\u{25cb} ", Style::default().fg(theme::YELLOW)),
                Span::styled("No API keys detected", Style::default().fg(theme::YELLOW)),
            ]));
            if !state.first_run {
                lines.push(Line::from(vec![Span::styled(
                    "  Run 'Re-run setup' to configure a provider",
                    theme::hint_style(),
                )]));
            } else {
                lines.push(Line::from(vec![Span::styled(
                    "  Select 'Get started' to configure",
                    theme::hint_style(),
                )]));
            }
        }

        frame.render_widget(Paragraph::new(lines), chunks[3]);
    }

    // ── Separator 2 ─────────────────────────────────────────────────────────
    render_separator(frame, chunks[4]);

    // ── Menu ────────────────────────────────────────────────────────────────
    let items: Vec<ListItem> = menu
        .iter()
        .enumerate()
        .map(|(i, item)| {
            // Highlight "Get started" for first-run users
            let is_primary = state.first_run && i == 0;
            let label_style = if is_primary {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_PRIMARY)
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<26}", item.label), label_style),
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

    frame.render_stateful_widget(list, chunks[5], &mut state.list);

    // ── OpenClaw migration hint ─────────────────────────────────────────────
    if state.first_run && state.openclaw_detected {
        let hint_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("\u{2192} ", Style::default().fg(theme::BLUE)),
                Span::styled("Coming from OpenClaw? ", Style::default().fg(theme::BLUE)),
                Span::styled(
                    "'Get started' includes automatic migration.",
                    theme::hint_style(),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(hint_lines), chunks[6]);
    }

    // ── Keybind hints ───────────────────────────────────────────────────────
    let hints = Line::from(vec![Span::styled(
        "\u{2191}\u{2193} navigate  enter select  q quit",
        theme::hint_style(),
    )]);
    frame.render_widget(Paragraph::new(hints), chunks[7]);
}

fn render_separator(frame: &mut ratatui::Frame, area: Rect) {
    let w = (area.width as usize).min(60);
    let line = Line::from(vec![Span::styled(
        "\u{2500}".repeat(w),
        Style::default().fg(theme::BORDER),
    )]);
    frame.render_widget(Paragraph::new(line), area);
}

// ── Desktop app launcher ────────────────────────────────────────────────────

pub fn launch_desktop_app() {
    let desktop_bin = {
        let exe = std::env::current_exe().ok();
        let dir = exe.as_ref().and_then(|e| e.parent());

        #[cfg(windows)]
        let name = "openfang-desktop.exe";
        #[cfg(not(windows))]
        let name = "openfang-desktop";

        // Check sibling of current exe first
        let sibling = dir.map(|d| d.join(name));

        match sibling {
            Some(ref path) if path.exists() => sibling,
            _ => which_lookup(name),
        }
    };

    match desktop_bin {
        Some(ref path) if path.exists() => {
            match std::process::Command::new(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(_) => {
                    ui::success("Desktop app launched.");
                }
                Err(e) => {
                    ui::error_with_fix(
                        &format!("Failed to launch desktop app: {e}"),
                        "Build it: cargo build -p openfang-desktop",
                    );
                }
            }
        }
        _ => {
            ui::error_with_fix(
                "Desktop app not found",
                "Build it: cargo build -p openfang-desktop",
            );
        }
    }
}

/// Simple PATH lookup for a binary name.
fn which_lookup(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    let separator = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(separator) {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}
