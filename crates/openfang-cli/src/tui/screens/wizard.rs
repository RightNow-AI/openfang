//! Setup wizard: provider list → API key → model → config save.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;

use crate::tui::theme;

/// Provider metadata for the setup wizard.
struct ProviderInfo {
    name: &'static str,
    env_var: &'static str,
    default_model: &'static str,
    needs_key: bool,
}

const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        name: "groq",
        env_var: "GROQ_API_KEY",
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
    },
    ProviderInfo {
        name: "anthropic",
        env_var: "ANTHROPIC_API_KEY",
        default_model: "claude-sonnet-4-20250514",
        needs_key: true,
    },
    ProviderInfo {
        name: "openai",
        env_var: "OPENAI_API_KEY",
        default_model: "gpt-4o",
        needs_key: true,
    },
    ProviderInfo {
        name: "openrouter",
        env_var: "OPENROUTER_API_KEY",
        default_model: "google/gemini-2.5-flash",
        needs_key: true,
    },
    ProviderInfo {
        name: "deepseek",
        env_var: "DEEPSEEK_API_KEY",
        default_model: "deepseek-chat",
        needs_key: true,
    },
    ProviderInfo {
        name: "together",
        env_var: "TOGETHER_API_KEY",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        needs_key: true,
    },
    ProviderInfo {
        name: "mistral",
        env_var: "MISTRAL_API_KEY",
        default_model: "mistral-large-latest",
        needs_key: true,
    },
    ProviderInfo {
        name: "fireworks",
        env_var: "FIREWORKS_API_KEY",
        default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
        needs_key: true,
    },
    ProviderInfo {
        name: "gemini",
        env_var: "GEMINI_API_KEY",
        default_model: "gemini-2.5-flash",
        needs_key: true,
    },
    ProviderInfo {
        name: "xai",
        env_var: "XAI_API_KEY",
        default_model: "grok-4-0709",
        needs_key: true,
    },
    ProviderInfo {
        name: "qwen",
        env_var: "DASHSCOPE_API_KEY",
        default_model: "qwen-plus",
        needs_key: true,
    },
    ProviderInfo {
        name: "perplexity",
        env_var: "PERPLEXITY_API_KEY",
        default_model: "sonar-pro",
        needs_key: true,
    },
    ProviderInfo {
        name: "cohere",
        env_var: "CO_API_KEY",
        default_model: "command-a",
        needs_key: true,
    },
    ProviderInfo {
        name: "cerebras",
        env_var: "CEREBRAS_API_KEY",
        default_model: "llama-3.3-70b",
        needs_key: true,
    },
    ProviderInfo {
        name: "sambanova",
        env_var: "SAMBANOVA_API_KEY",
        default_model: "Meta-Llama-3.3-70B-Instruct",
        needs_key: true,
    },
    ProviderInfo {
        name: "moonshot",
        env_var: "MOONSHOT_API_KEY",
        default_model: "moonshot-v1-128k",
        needs_key: true,
    },
    ProviderInfo {
        name: "zhipu",
        env_var: "ZHIPU_API_KEY",
        default_model: "glm-4-plus",
        needs_key: true,
    },
    ProviderInfo {
        name: "zhipu_coding",
        env_var: "ZHIPU_API_KEY",
        default_model: "codegeex-4",
        needs_key: true,
    },
    ProviderInfo {
        name: "nvidia",
        env_var: "NVIDIA_API_KEY",
        default_model: "nvidia/llama-3.1-nemotron-70b-instruct",
        needs_key: true,
    },
    ProviderInfo {
        name: "claude-code",
        env_var: "",
        default_model: "claude-code/sonnet",
        needs_key: false,
    },
    ProviderInfo {
        name: "ollama",
        env_var: "OLLAMA_API_KEY",
        default_model: "llama3.2",
        needs_key: false,
    },
    ProviderInfo {
        name: "vllm",
        env_var: "VLLM_API_KEY",
        default_model: "local-model",
        needs_key: false,
    },
    ProviderInfo {
        name: "lmstudio",
        env_var: "LMSTUDIO_API_KEY",
        default_model: "local-model",
        needs_key: false,
    },
];

/// Check if first-run setup is needed.
pub fn needs_setup() -> bool {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    Provider,
    ApiKey,
    Model,
    Saving,
    Done,
}

pub struct WizardState {
    pub step: WizardStep,
    pub provider_list: ListState,
    pub provider_order: Vec<usize>, // indices into PROVIDERS, detected first
    pub selected_provider: Option<usize>, // index into PROVIDERS
    pub api_key_input: String,
    pub api_key_from_env: bool,
    pub model_input: String,
    pub status_msg: String,
    pub created_config: Option<PathBuf>,
}

impl WizardState {
    pub fn new() -> Self {
        let mut state = Self {
            step: WizardStep::Provider,
            provider_list: ListState::default(),
            provider_order: Vec::new(),
            selected_provider: None,
            api_key_input: String::new(),
            api_key_from_env: false,
            model_input: String::new(),
            status_msg: String::new(),
            created_config: None,
        };
        state.build_provider_order();
        state.provider_list.select(Some(0));
        state
    }

    pub fn reset(&mut self) {
        self.step = WizardStep::Provider;
        self.selected_provider = None;
        self.api_key_input.clear();
        self.api_key_from_env = false;
        self.model_input.clear();
        self.status_msg.clear();
        self.created_config = None;
        self.build_provider_order();
        self.provider_list.select(Some(0));
    }

    fn build_provider_order(&mut self) {
        self.provider_order.clear();
        // Detected providers first
        for (i, p) in PROVIDERS.iter().enumerate() {
            let detected = if p.name == "claude-code" {
                openfang_runtime::drivers::claude_code::claude_code_available()
            } else {
                !p.env_var.is_empty() && std::env::var(p.env_var).is_ok()
            };
            if detected {
                self.provider_order.push(i);
            }
        }
        // Then the rest
        for (i, p) in PROVIDERS.iter().enumerate() {
            let detected = if p.name == "claude-code" {
                openfang_runtime::drivers::claude_code::claude_code_available()
            } else {
                !p.env_var.is_empty() && std::env::var(p.env_var).is_ok()
            };
            if !detected {
                self.provider_order.push(i);
            }
        }
    }

    fn selected_provider_info(&self) -> Option<&'static ProviderInfo> {
        self.selected_provider.map(|i| &PROVIDERS[i])
    }

    /// Handle a key event. Returns true if wizard is complete or cancelled.
    /// `cancelled` is set if the user backed out entirely.
    pub fn handle_key(&mut self, key: KeyEvent) -> WizardResult {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return WizardResult::Cancelled;
        }

        match self.step {
            WizardStep::Provider => self.handle_provider(key),
            WizardStep::ApiKey => self.handle_api_key(key),
            WizardStep::Model => self.handle_model(key),
            WizardStep::Saving | WizardStep::Done => WizardResult::Continue,
        }
    }

    fn handle_provider(&mut self, key: KeyEvent) -> WizardResult {
        match key.code {
            KeyCode::Esc => return WizardResult::Cancelled,
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.provider_list.selected().unwrap_or(0);
                let next = if i == 0 {
                    self.provider_order.len() - 1
                } else {
                    i - 1
                };
                self.provider_list.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.provider_list.selected().unwrap_or(0);
                let next = (i + 1) % self.provider_order.len();
                self.provider_list.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(list_idx) = self.provider_list.selected() {
                    let Some(&prov_idx) = self.provider_order.get(list_idx) else {
                        return WizardResult::Continue;
                    };
                    let Some(p) = PROVIDERS.get(prov_idx) else {
                        return WizardResult::Continue;
                    };
                    self.selected_provider = Some(prov_idx);

                    if !p.needs_key {
                        // No key needed, skip to model
                        self.api_key_from_env = false;
                        self.model_input = p.default_model.to_string();
                        self.step = WizardStep::Model;
                    } else if std::env::var(p.env_var).is_ok() {
                        // Key already in env
                        self.api_key_from_env = true;
                        self.model_input = p.default_model.to_string();
                        self.step = WizardStep::Model;
                    } else {
                        self.api_key_from_env = false;
                        self.api_key_input.clear();
                        self.step = WizardStep::ApiKey;
                    }
                }
            }
            _ => {}
        }
        WizardResult::Continue
    }

    fn handle_api_key(&mut self, key: KeyEvent) -> WizardResult {
        match key.code {
            KeyCode::Esc => {
                self.step = WizardStep::Provider;
            }
            KeyCode::Enter => {
                if !self.api_key_input.is_empty() {
                    if let Some(p) = self.selected_provider_info() {
                        self.model_input = p.default_model.to_string();
                    }
                    self.step = WizardStep::Model;
                }
            }
            KeyCode::Char(c) => {
                self.api_key_input.push(c);
            }
            KeyCode::Backspace => {
                self.api_key_input.pop();
            }
            _ => {}
        }
        WizardResult::Continue
    }

    fn handle_model(&mut self, key: KeyEvent) -> WizardResult {
        match key.code {
            KeyCode::Esc => {
                // Go back
                if let Some(p) = self.selected_provider_info() {
                    if p.needs_key && !self.api_key_from_env {
                        self.step = WizardStep::ApiKey;
                    } else {
                        self.step = WizardStep::Provider;
                    }
                } else {
                    self.step = WizardStep::Provider;
                }
            }
            KeyCode::Enter => {
                self.step = WizardStep::Saving;
                self.save_config();
            }
            KeyCode::Char(c) => {
                self.model_input.push(c);
            }
            KeyCode::Backspace => {
                self.model_input.pop();
            }
            _ => {}
        }
        WizardResult::Continue
    }

    fn save_config(&mut self) {
        let p = match self.selected_provider_info() {
            Some(p) => p,
            None => {
                self.status_msg = "No provider selected".to_string();
                self.step = WizardStep::Provider;
                return;
            }
        };

        let openfang_dir = if let Ok(h) = std::env::var("OPENFANG_HOME") {
            std::path::PathBuf::from(h)
        } else {
            match dirs::home_dir() {
                Some(h) => h.join(".openfang"),
                None => {
                    self.status_msg = "Could not determine home directory".to_string();
                    self.step = WizardStep::Done;
                    return;
                }
            }
        };
        let _ = std::fs::create_dir_all(openfang_dir.join("agents"));
        let _ = std::fs::create_dir_all(openfang_dir.join("data"));
        crate::restrict_dir_permissions(&openfang_dir);

        let api_key_line = if !self.api_key_input.is_empty() {
            format!("api_key = \"{}\"", self.api_key_input)
        } else if p.env_var.is_empty() {
            String::new()
        } else {
            format!("api_key_env = \"{}\"", p.env_var)
        };

        let model = if self.model_input.is_empty() {
            p.default_model
        } else {
            &self.model_input
        };

        let config = format!(
            r#"# OpenFang Agent OS configuration
# Generated by setup wizard

[default_model]
provider = "{provider}"
model = "{model}"
{api_key_line}

[memory]
decay_rate = 0.05

[network]
listen_addr = "127.0.0.1:4200"
"#,
            provider = p.name,
        );

        let config_path = openfang_dir.join("config.toml");
        match std::fs::write(&config_path, &config) {
            Ok(()) => {
                crate::restrict_file_permissions(&config_path);
                self.status_msg = format!("Config saved \u{2014} {} / {}", p.name, model);
                self.created_config = Some(config_path);
            }
            Err(e) => {
                self.status_msg = format!("Failed to save config: {e}");
            }
        }
        self.step = WizardStep::Done;
    }
}

pub enum WizardResult {
    Continue,
    Cancelled,
}

/// Render the wizard screen.
pub fn draw(f: &mut Frame, area: Rect, state: &mut WizardState) {
    // Fill background
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_PRIMARY)),
        area,
    );

    let step_label = match state.step {
        WizardStep::Provider => "Step 1 of 3",
        WizardStep::ApiKey => "Step 2 of 3",
        WizardStep::Model => "Step 3 of 3",
        WizardStep::Saving => "Saving...",
        WizardStep::Done => "Complete",
    };

    // Left-aligned content area
    let content = if area.width < 10 || area.height < 5 {
        area
    } else {
        let margin = 3u16.min(area.width.saturating_sub(10));
        let w = 72u16.min(area.width.saturating_sub(margin));
        Rect {
            x: area.x.saturating_add(margin),
            y: area.y,
            width: w,
            height: area.height,
        }
    };

    let chunks = Layout::vertical([
        Constraint::Length(1), // top pad
        Constraint::Length(1), // header
        Constraint::Length(1), // separator
        Constraint::Min(1),    // step content
    ])
    .split(content);

    // Header
    let header = Line::from(vec![
        Span::styled(
            "Setup",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {step_label}"), theme::dim_style()),
    ]);
    f.render_widget(Paragraph::new(header), chunks[1]);

    // Separator
    let sep_w = content.width.min(60) as usize;
    let sep = Line::from(vec![Span::styled(
        "\u{2500}".repeat(sep_w),
        Style::default().fg(theme::BORDER),
    )]);
    f.render_widget(Paragraph::new(sep), chunks[2]);

    match state.step {
        WizardStep::Provider => draw_provider(f, chunks[3], state),
        WizardStep::ApiKey => draw_api_key(f, chunks[3], state),
        WizardStep::Model => draw_model(f, chunks[3], state),
        WizardStep::Saving | WizardStep::Done => draw_done(f, chunks[3], state),
    }
}

fn draw_provider(f: &mut Frame, area: Rect, state: &mut WizardState) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // prompt
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    let prompt = Paragraph::new(Line::from(vec![Span::raw("  Choose your LLM provider:")]));
    f.render_widget(prompt, chunks[0]);

    let items: Vec<ListItem> = state
        .provider_order
        .iter()
        .map(|&idx| {
            let p = &PROVIDERS[idx];
            let hint = if p.name == "claude-code" {
                if openfang_runtime::drivers::claude_code::claude_code_available() {
                    "CLI detected".to_string()
                } else {
                    "no API key needed".to_string()
                }
            } else if !p.needs_key {
                "local, no key needed".to_string()
            } else if !p.env_var.is_empty() && std::env::var(p.env_var).is_ok() {
                format!("{} detected", p.env_var)
            } else {
                format!("requires {}", p.env_var)
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("  {:<14}", p.name)),
                Span::styled(hint, theme::dim_style()),
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

    f.render_stateful_widget(list, chunks[1], &mut state.provider_list);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Cancel",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_api_key(f: &mut Frame, area: Rect, state: &mut WizardState) {
    let p = match state.selected_provider_info() {
        Some(p) => p,
        None => return,
    };

    let chunks = Layout::vertical([
        Constraint::Length(2), // prompt
        Constraint::Length(1), // input
        Constraint::Length(2), // spacer + hint about env var
        Constraint::Min(0),    // spacer
        Constraint::Length(1), // hints
    ])
    .split(area);

    let prompt = Paragraph::new(Line::from(vec![Span::raw(format!(
        "  Enter your {} API key:",
        p.name
    ))]));
    f.render_widget(prompt, chunks[0]);

    // Masked input
    let masked: String = "\u{2022}".repeat(state.api_key_input.len());
    let input = Paragraph::new(Line::from(vec![
        Span::raw("  > "),
        Span::styled(&masked, theme::input_style()),
        Span::styled(
            "\u{2588}",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]));
    f.render_widget(input, chunks[1]);

    let env_hint = Paragraph::new(Line::from(vec![Span::styled(
        format!("    Or set {} environment variable", p.env_var),
        theme::dim_style(),
    )]));
    f.render_widget(env_hint, chunks[2]);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [Enter] Confirm  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[4]);
}

fn draw_model(f: &mut Frame, area: Rect, state: &mut WizardState) {
    let p = match state.selected_provider_info() {
        Some(p) => p,
        None => return,
    };

    let chunks = Layout::vertical([
        Constraint::Length(2), // prompt
        Constraint::Length(1), // input
        Constraint::Length(2), // default hint
        Constraint::Min(0),
        Constraint::Length(1), // hints
    ])
    .split(area);

    let prompt = Paragraph::new(Line::from(vec![Span::raw("  Model name:")]));
    f.render_widget(prompt, chunks[0]);

    let display_text = if state.model_input.is_empty() {
        p.default_model
    } else {
        &state.model_input
    };
    let input = Paragraph::new(Line::from(vec![
        Span::raw("  > "),
        Span::styled(display_text, theme::input_style()),
        Span::styled(
            "\u{2588}",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]));
    f.render_widget(input, chunks[1]);

    let default_hint = Paragraph::new(Line::from(vec![Span::styled(
        format!("    default: {}", p.default_model),
        theme::dim_style(),
    )]));
    f.render_widget(default_hint, chunks[2]);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [Enter] Confirm  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[4]);
}

fn draw_done(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    let icon = if state.created_config.is_some() {
        Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN))
    } else {
        Span::styled("  \u{2718} ", Style::default().fg(theme::RED))
    };

    let msg = Paragraph::new(Line::from(vec![icon, Span::raw(&state.status_msg)]));
    f.render_widget(msg, chunks[0]);

    if state.created_config.is_some() {
        let cont = Paragraph::new(Line::from(vec![Span::styled(
            "    Continuing...",
            theme::dim_style(),
        )]));
        f.render_widget(cont, chunks[1]);
    }
}
