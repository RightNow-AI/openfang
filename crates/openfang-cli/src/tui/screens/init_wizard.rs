//! Standalone ratatui init wizard: 6-step onboarding flow.
//!
//! Launched by `openfang init` (without `--quick`). Takes over the terminal,
//! runs its own event loop, and returns an `InitResult`.

use ratatui::crossterm::event::{self, Event as CtEvent, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::tui::theme;
use openfang_runtime::model_catalog::ModelCatalog;
use openfang_types::model_catalog::ModelTier;

// ── Provider metadata ──────────────────────────────────────────────────────

struct ProviderInfo {
    name: &'static str,
    display: &'static str,
    env_var: &'static str,
    default_model: &'static str,
    needs_key: bool,
    hint: &'static str,
}

const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        name: "groq",
        display: "Groq",
        env_var: "GROQ_API_KEY",
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
        hint: "free tier",
    },
    ProviderInfo {
        name: "gemini",
        display: "Gemini",
        env_var: "GEMINI_API_KEY",
        default_model: "gemini-2.5-flash",
        needs_key: true,
        hint: "free tier",
    },
    ProviderInfo {
        name: "deepseek",
        display: "DeepSeek",
        env_var: "DEEPSEEK_API_KEY",
        default_model: "deepseek-chat",
        needs_key: true,
        hint: "cheap",
    },
    ProviderInfo {
        name: "anthropic",
        display: "Anthropic",
        env_var: "ANTHROPIC_API_KEY",
        default_model: "claude-sonnet-4-20250514",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "openai",
        display: "OpenAI",
        env_var: "OPENAI_API_KEY",
        default_model: "gpt-4o",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "openrouter",
        display: "OpenRouter",
        env_var: "OPENROUTER_API_KEY",
        default_model: "openrouter/auto",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "together",
        display: "Together",
        env_var: "TOGETHER_API_KEY",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "mistral",
        display: "Mistral",
        env_var: "MISTRAL_API_KEY",
        default_model: "mistral-large-latest",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "fireworks",
        display: "Fireworks",
        env_var: "FIREWORKS_API_KEY",
        default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
        needs_key: true,
        hint: "",
    },
    ProviderInfo {
        name: "ollama",
        display: "Ollama",
        env_var: "OLLAMA_API_KEY",
        default_model: "llama3.2",
        needs_key: false,
        hint: "local",
    },
    ProviderInfo {
        name: "lmstudio",
        display: "LM Studio",
        env_var: "LMSTUDIO_API_KEY",
        default_model: "local-model",
        needs_key: false,
        hint: "local",
    },
    ProviderInfo {
        name: "custom",
        display: "Custom",
        env_var: "OPENAI_COMPAT_API_KEY",
        default_model: "custom-model",
        needs_key: false,
        hint: "openai-compatible",
    },
];

// ── Public result type ─────────────────────────────────────────────────────

/// What the user chose to do after init completes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LaunchChoice {
    Desktop,
    Dashboard,
    Chat,
}

pub enum InitResult {
    Completed {
        provider: String,
        model: String,
        daemon_started: bool,
        launch: LaunchChoice,
    },
    Cancelled,
}

// ── Internal state ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    Migration,
    Provider,
    Custom,
    ApiKey,
    Model,
    Routing,
    Complete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MigrationPhase {
    Detecting,
    Offer,
    Running,
    Done,
}

/// Sub-state within the Routing step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RoutingPhase {
    /// Yes / No choice
    Choice,
    /// Picking model for a tier (0=fast, 1=balanced, 2=frontier)
    PickTier(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyTestState {
    Idle,
    Testing,
    Ok,
    Warn,
}

/// A model entry for list display.
struct ModelEntry {
    id: String,
    display_name: String,
    tier: &'static str,
    cost: String,
}

const ROUTING_TIER_NAMES: [&str; 3] = ["Fast", "Balanced", "Frontier"];
const ROUTING_TIER_DESC: [&str; 3] = [
    "quick lookups, greetings, simple Q&A",
    "standard conversation, general tasks",
    "multi-step reasoning, code generation",
];

struct State {
    step: Step,
    tick: usize,

    // Migration
    migration_phase: MigrationPhase,
    migration_choice_list: ListState,
    openclaw_path: Option<PathBuf>,
    openclaw_scan: Option<openfang_migrate::openclaw::ScanResult>,
    migration_report: Option<openfang_migrate::report::MigrationReport>,
    migration_error: Option<String>,
    migration_done_at: Option<Instant>,
    migrated_provider: Option<String>,

    // Provider selection
    provider_list: ListState,
    provider_order: Vec<usize>,
    selected_provider: Option<usize>,
    custom_provider_name: String,
    custom_env_var: String,
    custom_base_url: String,
    custom_model: String,
    custom_field: usize,

    // API key
    api_key_input: String,
    api_key_from_env: bool,
    key_test: KeyTestState,
    key_test_started: Option<Instant>,

    // Model selection
    model_input: String,
    model_catalog: ModelCatalog,
    model_entries: Vec<ModelEntry>,
    model_list: ListState,

    // Routing
    routing_phase: RoutingPhase,
    routing_choice_list: ListState, // 0=Yes, 1=No
    routing_enabled: bool,
    /// Selected model IDs per tier: [fast, balanced, frontier]
    routing_models: [String; 3],
    routing_tier_list: ListState, // for PickTier model selection

    // Complete
    complete_list: ListState,
    daemon_started: bool,
    daemon_url: String,
    daemon_error: String,
    saving_done: bool,
    save_error: String,
}

impl State {
    fn new() -> Self {
        let mut s = Self {
            step: Step::Welcome,
            tick: 0,
            migration_phase: MigrationPhase::Detecting,
            migration_choice_list: ListState::default(),
            openclaw_path: None,
            openclaw_scan: None,
            migration_report: None,
            migration_error: None,
            migration_done_at: None,
            migrated_provider: None,
            provider_list: ListState::default(),
            provider_order: Vec::new(),
            selected_provider: None,
            custom_provider_name: "custom-openai".to_string(),
            custom_env_var: derive_env_var_from_provider("custom-openai"),
            custom_base_url: "http://localhost:8000/v1".to_string(),
            custom_model: "gpt-5.2".to_string(),
            custom_field: 0,
            api_key_input: String::new(),
            api_key_from_env: false,
            key_test: KeyTestState::Idle,
            key_test_started: None,
            model_input: String::new(),
            model_catalog: ModelCatalog::new(),
            model_entries: Vec::new(),
            model_list: ListState::default(),
            routing_phase: RoutingPhase::Choice,
            routing_choice_list: ListState::default(),
            routing_enabled: false,
            routing_models: [String::new(), String::new(), String::new()],
            routing_tier_list: ListState::default(),
            complete_list: ListState::default(),
            daemon_started: false,
            daemon_url: String::new(),
            daemon_error: String::new(),
            saving_done: false,
            save_error: String::new(),
        };
        s.build_provider_order();
        s.provider_list.select(Some(0));
        s.migration_choice_list.select(Some(0));
        s.routing_choice_list.select(Some(0));
        s.complete_list.select(Some(0));
        s
    }

    fn build_provider_order(&mut self) {
        self.provider_order.clear();
        let gemini_via_google = std::env::var("GOOGLE_API_KEY").is_ok();
        for (i, p) in PROVIDERS.iter().enumerate() {
            let detected =
                std::env::var(p.env_var).is_ok() || (p.name == "gemini" && gemini_via_google);
            if detected {
                self.provider_order.push(i);
            }
        }
        for (i, p) in PROVIDERS.iter().enumerate() {
            let detected =
                std::env::var(p.env_var).is_ok() || (p.name == "gemini" && gemini_via_google);
            if !detected {
                self.provider_order.push(i);
            }
        }
    }

    fn provider(&self) -> Option<&'static ProviderInfo> {
        self.selected_provider.map(|i| &PROVIDERS[i])
    }

    fn step_label(&self) -> &'static str {
        match self.step {
            Step::Welcome => "1 of 8",
            Step::Migration => "2 of 8",
            Step::Provider => "3 of 8",
            Step::Custom => "4 of 8",
            Step::ApiKey => "5 of 8",
            Step::Model => "6 of 8",
            Step::Routing => "7 of 8",
            Step::Complete => "8 of 8",
        }
    }

    /// Advance to the Provider step, optionally pre-selecting a migrated provider.
    fn advance_to_provider(&mut self) {
        if let Some(ref prov_name) = self.migrated_provider {
            // Find the provider in the ordered list and pre-select it
            for (list_idx, &prov_idx) in self.provider_order.iter().enumerate() {
                if PROVIDERS[prov_idx].name == prov_name.as_str() {
                    self.provider_list.select(Some(list_idx));
                    break;
                }
            }
        }
        self.step = Step::Provider;
    }

    fn is_provider_detected(&self, prov_idx: usize) -> bool {
        let p = &PROVIDERS[prov_idx];
        if p.name == "custom" {
            return false;
        }
        std::env::var(p.env_var).is_ok()
            || (p.name == "gemini" && std::env::var("GOOGLE_API_KEY").is_ok())
    }

    fn is_custom_provider(&self) -> bool {
        self.provider().is_some_and(|p| p.name == "custom")
    }

    fn selected_provider_name(&self) -> String {
        if self.is_custom_provider() {
            self.custom_provider_name.trim().to_string()
        } else {
            self.provider()
                .map(|p| p.name.to_string())
                .unwrap_or_default()
        }
    }

    fn selected_provider_display(&self) -> String {
        if self.is_custom_provider() {
            if self.custom_provider_name.trim().is_empty() {
                "Custom".to_string()
            } else {
                format!("Custom ({})", self.custom_provider_name.trim())
            }
        } else {
            self.provider()
                .map(|p| p.display.to_string())
                .unwrap_or_default()
        }
    }

    fn selected_env_var(&self) -> String {
        if self.is_custom_provider() {
            self.custom_env_var.trim().to_string()
        } else {
            self.provider()
                .map(|p| p.env_var.to_string())
                .unwrap_or_default()
        }
    }

    fn selected_default_model(&self) -> String {
        if self.is_custom_provider() {
            self.custom_model.trim().to_string()
        } else {
            self.provider()
                .map(|p| p.default_model.to_string())
                .unwrap_or_default()
        }
    }

    fn selected_base_url(&self) -> Option<String> {
        if self.is_custom_provider() {
            let url = self.custom_base_url.trim().to_string();
            if url.is_empty() {
                None
            } else {
                Some(url)
            }
        } else {
            None
        }
    }

    /// Populate model_entries from the catalog for the selected provider.
    fn load_models_for_provider(&mut self) {
        self.model_entries.clear();
        if self.is_custom_provider() {
            let model = self.selected_default_model();
            self.model_entries.push(ModelEntry {
                id: model.clone(),
                display_name: model,
                tier: "default",
                cost: String::new(),
            });
            self.model_list.select(Some(0));
            return;
        }

        let p = match self.provider() {
            Some(p) => p,
            None => return,
        };

        let models = self.model_catalog.models_by_provider(p.name);
        let mut default_idx = 0usize;

        for (i, m) in models.iter().enumerate() {
            let tier = tier_label(m.tier);
            let cost = if m.input_cost_per_m == 0.0 && m.output_cost_per_m == 0.0 {
                "free".to_string()
            } else {
                format!("${:.2}/${:.2}", m.input_cost_per_m, m.output_cost_per_m)
            };

            if m.id == p.default_model {
                default_idx = i;
            }

            self.model_entries.push(ModelEntry {
                id: m.id.clone(),
                display_name: m.display_name.clone(),
                tier,
                cost,
            });
        }

        if self.model_entries.is_empty() {
            self.model_entries.push(ModelEntry {
                id: p.default_model.to_string(),
                display_name: p.default_model.to_string(),
                tier: "default",
                cost: String::new(),
            });
        }

        self.model_list.select(Some(default_idx));
    }

    fn selected_model_id(&self) -> String {
        if let Some(idx) = self.model_list.selected() {
            if let Some(entry) = self.model_entries.get(idx) {
                return entry.id.clone();
            }
        }
        self.selected_default_model()
    }

    /// Auto-select routing models based on the provider's catalog entries.
    fn auto_select_routing_models(&mut self) {
        let p = match self.provider() {
            Some(p) => p,
            None => return,
        };

        let models = self.model_catalog.models_by_provider(p.name);

        // Find best candidates per target tier
        let mut fast: Option<&str> = None;
        let mut balanced: Option<&str> = None;
        let mut frontier: Option<&str> = None;

        for m in &models {
            match m.tier {
                ModelTier::Fast | ModelTier::Local => {
                    if fast.is_none() {
                        fast = Some(&m.id);
                    }
                }
                ModelTier::Balanced => {
                    if balanced.is_none() {
                        balanced = Some(&m.id);
                    }
                }
                ModelTier::Smart => {
                    // Smart is a good balanced pick; also good frontier if no frontier exists
                    if balanced.is_none() {
                        balanced = Some(&m.id);
                    }
                    if frontier.is_none() {
                        frontier = Some(&m.id);
                    }
                }
                ModelTier::Frontier => {
                    if frontier.is_none() {
                        frontier = Some(&m.id);
                    }
                }
            }
        }

        // Fallback: use selected default model for any missing tier
        let fallback = &self.model_input;
        self.routing_models[0] = fast.unwrap_or(fallback).to_string();
        self.routing_models[1] = balanced.unwrap_or(fallback).to_string();
        self.routing_models[2] = frontier.unwrap_or(fallback).to_string();
    }

    /// Pre-select the routing_tier_list to match the current routing_models[tier].
    fn select_routing_tier_model(&mut self, tier: usize) {
        let target = &self.routing_models[tier];
        let idx = self
            .model_entries
            .iter()
            .position(|e| e.id == *target)
            .unwrap_or(0);
        self.routing_tier_list.select(Some(idx));
    }
}

fn tier_label(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Frontier => "frontier",
        ModelTier::Smart => "smart",
        ModelTier::Balanced => "balanced",
        ModelTier::Fast => "fast",
        ModelTier::Local => "local",
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

pub fn run() -> InitResult {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        original_hook(info);
    }));

    let mut terminal = ratatui::init();
    let mut state = State::new();

    let (test_tx, test_rx) = std::sync::mpsc::channel::<bool>();
    let (migrate_tx, migrate_rx) =
        std::sync::mpsc::channel::<Result<openfang_migrate::report::MigrationReport, String>>();

    let result = loop {
        terminal
            .draw(|f| draw(f, f.area(), &mut state))
            .expect("draw failed");

        // Check for background key-test result
        if state.key_test == KeyTestState::Testing {
            if let Ok(ok) = test_rx.try_recv() {
                state.key_test = if ok {
                    KeyTestState::Ok
                } else {
                    KeyTestState::Warn
                };
                state.key_test_started = Some(Instant::now());
            }
        }

        // Auto-advance from key test result after 600ms
        if matches!(state.key_test, KeyTestState::Ok | KeyTestState::Warn) {
            if let Some(started) = state.key_test_started {
                if started.elapsed() >= Duration::from_millis(600) {
                    state.load_models_for_provider();
                    state.step = Step::Model;
                    state.key_test = KeyTestState::Idle;
                    state.key_test_started = None;
                }
            }
        }

        // ── Migration detection (resolves in 1 frame) ──
        if state.step == Step::Migration && state.migration_phase == MigrationPhase::Detecting {
            match openfang_migrate::openclaw::detect_openclaw_home() {
                None => {
                    // No OpenClaw found — skip migration entirely
                    state.advance_to_provider();
                }
                Some(path) => {
                    let scan = openfang_migrate::openclaw::scan_openclaw_workspace(&path);
                    let has_content = scan.has_config
                        || !scan.agents.is_empty()
                        || !scan.channels.is_empty()
                        || !scan.skills.is_empty()
                        || scan.has_memory;
                    if has_content {
                        state.openclaw_path = Some(path);
                        state.openclaw_scan = Some(scan);
                        state.migration_phase = MigrationPhase::Offer;
                    } else {
                        // Nothing useful to migrate
                        state.advance_to_provider();
                    }
                }
            }
        }

        // ── Migration background result polling ──
        if state.step == Step::Migration && state.migration_phase == MigrationPhase::Running {
            if let Ok(result) = migrate_rx.try_recv() {
                match result {
                    Ok(report) => {
                        // Extract provider from first imported agent for pre-selection
                        if let Some(scan) = &state.openclaw_scan {
                            for agent in &scan.agents {
                                if !agent.provider.is_empty() {
                                    state.migrated_provider = Some(agent.provider.clone());
                                    break;
                                }
                            }
                        }
                        state.migration_report = Some(report);
                        state.migration_phase = MigrationPhase::Done;
                        state.migration_done_at = Some(Instant::now());
                    }
                    Err(e) => {
                        state.migration_error = Some(e);
                        state.migration_phase = MigrationPhase::Done;
                        state.migration_done_at = Some(Instant::now());
                    }
                }
            }
        }

        // ── Migration auto-advance 1.5s after Done ──
        if state.step == Step::Migration && state.migration_phase == MigrationPhase::Done {
            if let Some(done_at) = state.migration_done_at {
                if done_at.elapsed() >= Duration::from_millis(1500) {
                    state.advance_to_provider();
                }
            }
        }

        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(CtEvent::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if key.code == KeyCode::Char('c')
                    && key
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
                {
                    break InitResult::Cancelled;
                }

                match state.step {
                    Step::Welcome => match key.code {
                        KeyCode::Enter => {
                            state.migration_phase = MigrationPhase::Detecting;
                            state.step = Step::Migration;
                        }
                        KeyCode::Esc => break InitResult::Cancelled,
                        _ => {}
                    },

                    Step::Migration => handle_migration_key(&mut state, key.code, &migrate_tx),

                    Step::Provider => match key.code {
                        KeyCode::Esc => break InitResult::Cancelled,
                        KeyCode::Up | KeyCode::Char('k') => {
                            let i = state.provider_list.selected().unwrap_or(0);
                            let next = if i == 0 {
                                state.provider_order.len() - 1
                            } else {
                                i - 1
                            };
                            state.provider_list.select(Some(next));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let i = state.provider_list.selected().unwrap_or(0);
                            let next = (i + 1) % state.provider_order.len();
                            state.provider_list.select(Some(next));
                        }
                        KeyCode::Enter => {
                            if let Some(list_idx) = state.provider_list.selected() {
                                let prov_idx = state.provider_order[list_idx];
                                state.selected_provider = Some(prov_idx);
                                let p = &PROVIDERS[prov_idx];

                                if p.name == "custom" {
                                    state.custom_field = 0;
                                    state.step = Step::Custom;
                                } else if !p.needs_key {
                                    state.api_key_from_env = false;
                                    state.load_models_for_provider();
                                    state.step = Step::Model;
                                } else if state.is_provider_detected(prov_idx) {
                                    state.api_key_from_env = true;
                                    state.load_models_for_provider();
                                    state.step = Step::Model;
                                } else {
                                    state.api_key_from_env = false;
                                    state.api_key_input.clear();
                                    state.key_test = KeyTestState::Idle;
                                    state.step = Step::ApiKey;
                                }
                            }
                        }
                        _ => {}
                    },

                    Step::Custom => match key.code {
                        KeyCode::Esc => state.step = Step::Provider,
                        KeyCode::Tab | KeyCode::Down => {
                            state.custom_field = (state.custom_field + 1) % 3;
                        }
                        KeyCode::Up => {
                            state.custom_field = if state.custom_field == 0 {
                                2
                            } else {
                                state.custom_field - 1
                            };
                        }
                        KeyCode::Enter => {
                            if state.custom_field < 2 {
                                state.custom_field += 1;
                            } else {
                                let provider_name = state.custom_provider_name.trim();
                                let model = state.custom_model.trim();
                                let base_url = state.custom_base_url.trim();
                                if provider_name.is_empty() || model.is_empty() {
                                    state.save_error =
                                        "Provider name and model cannot be empty".to_string();
                                    continue;
                                }
                                if !base_url.starts_with("http://")
                                    && !base_url.starts_with("https://")
                                {
                                    state.save_error =
                                        "base_url must start with http:// or https://".to_string();
                                    continue;
                                }
                                state.custom_env_var = derive_env_var_from_provider(provider_name);
                                state.save_error.clear();
                                state.api_key_from_env = std::env::var(state.selected_env_var())
                                    .map(|v| !v.trim().is_empty())
                                    .unwrap_or(false);
                                state.api_key_input.clear();
                                state.key_test = KeyTestState::Idle;
                                state.step = Step::ApiKey;
                            }
                        }
                        KeyCode::Backspace => match state.custom_field {
                            0 => {
                                state.custom_provider_name.pop();
                                state.custom_env_var =
                                    derive_env_var_from_provider(&state.custom_provider_name);
                            }
                            1 => {
                                state.custom_base_url.pop();
                            }
                            _ => {
                                state.custom_model.pop();
                            }
                        },
                        KeyCode::Char(c) => match state.custom_field {
                            0 => {
                                state.custom_provider_name.push(c);
                                state.custom_env_var =
                                    derive_env_var_from_provider(&state.custom_provider_name);
                            }
                            1 => state.custom_base_url.push(c),
                            _ => state.custom_model.push(c),
                        },
                        _ => {}
                    },

                    Step::ApiKey => {
                        if matches!(state.key_test, KeyTestState::Ok | KeyTestState::Warn) {
                            continue;
                        }

                        match key.code {
                            KeyCode::Esc => {
                                state.key_test = KeyTestState::Idle;
                                state.step = if state.is_custom_provider() {
                                    Step::Custom
                                } else {
                                    Step::Provider
                                };
                            }
                            KeyCode::Enter => {
                                if !state.api_key_input.is_empty()
                                    && state.key_test == KeyTestState::Idle
                                {
                                    if state.provider().is_some() {
                                        let env_var = state.selected_env_var();
                                        if !env_var.is_empty() {
                                            let _ = crate::dotenv::save_env_key(
                                                &env_var,
                                                &state.api_key_input,
                                            );
                                        }
                                    }
                                    state.key_test = KeyTestState::Testing;
                                    let provider_name = state.selected_provider_name();
                                    let env_var = state.selected_env_var();
                                    let tx = test_tx.clone();
                                    std::thread::spawn(move || {
                                        let ok = crate::test_api_key(&provider_name, &env_var);
                                        let _ = tx.send(ok);
                                    });
                                }
                            }
                            KeyCode::Char(c) => {
                                if state.key_test == KeyTestState::Idle {
                                    state.api_key_input.push(c);
                                }
                            }
                            KeyCode::Backspace => {
                                if state.key_test == KeyTestState::Idle {
                                    state.api_key_input.pop();
                                }
                            }
                            _ => {}
                        }
                    }

                    Step::Model => match key.code {
                        KeyCode::Esc => {
                            if state.is_custom_provider() {
                                if !state.selected_env_var().is_empty() && !state.api_key_from_env {
                                    state.key_test = KeyTestState::Idle;
                                    state.step = Step::ApiKey;
                                } else {
                                    state.step = Step::Custom;
                                }
                            } else if let Some(p) = state.provider() {
                                if p.needs_key && !state.api_key_from_env {
                                    state.key_test = KeyTestState::Idle;
                                    state.step = Step::ApiKey;
                                } else {
                                    state.step = Step::Provider;
                                }
                            } else {
                                state.step = Step::Provider;
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            let len = state.model_entries.len().max(1);
                            let i = state.model_list.selected().unwrap_or(0);
                            let next = if i == 0 { len - 1 } else { i - 1 };
                            state.model_list.select(Some(next));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let len = state.model_entries.len().max(1);
                            let i = state.model_list.selected().unwrap_or(0);
                            let next = (i + 1) % len;
                            state.model_list.select(Some(next));
                        }
                        KeyCode::Enter => {
                            state.model_input = state.selected_model_id();
                            // Prepare routing step
                            state.routing_phase = RoutingPhase::Choice;
                            state.routing_choice_list.select(Some(0));
                            // Only offer routing if provider has 2+ models
                            if state.model_entries.len() < 2 {
                                // Skip routing — not enough models
                                state.routing_enabled = false;
                                save_config(&mut state);
                                state.step = Step::Complete;
                            } else {
                                state.step = Step::Routing;
                            }
                        }
                        _ => {}
                    },

                    Step::Routing => handle_routing_key(&mut state, key.code),

                    Step::Complete => match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            let i = state.complete_list.selected().unwrap_or(0);
                            let next = if i == 0 { 2 } else { i - 1 };
                            state.complete_list.select(Some(next));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let i = state.complete_list.selected().unwrap_or(0);
                            let next = (i + 1) % 3;
                            state.complete_list.select(Some(next));
                        }
                        // Number shortcuts: 1=Desktop, 2=Dashboard, 3=Chat
                        KeyCode::Char('1') => {
                            state.complete_list.select(Some(0));
                        }
                        KeyCode::Char('2') => {
                            state.complete_list.select(Some(1));
                        }
                        KeyCode::Char('3') => {
                            state.complete_list.select(Some(2));
                        }
                        KeyCode::Enter => {
                            let choice = match state.complete_list.selected() {
                                Some(0) => LaunchChoice::Desktop,
                                Some(1) => LaunchChoice::Dashboard,
                                _ => LaunchChoice::Chat,
                            };
                            break InitResult::Completed {
                                provider: state.selected_provider_name(),
                                model: state.model_input.clone(),
                                daemon_started: state.daemon_started,
                                launch: choice,
                            };
                        }
                        KeyCode::Esc => {
                            break InitResult::Completed {
                                provider: state.selected_provider_name(),
                                model: state.model_input.clone(),
                                daemon_started: state.daemon_started,
                                launch: LaunchChoice::Chat,
                            };
                        }
                        _ => {}
                    },
                }
            }
        } else {
            state.tick = state.tick.wrapping_add(1);
        }
    };

    ratatui::restore();
    result
}

// ── Migration step key handler ─────────────────────────────────────────────

fn handle_migration_key(
    state: &mut State,
    code: KeyCode,
    migrate_tx: &std::sync::mpsc::Sender<Result<openfang_migrate::report::MigrationReport, String>>,
) {
    match state.migration_phase {
        MigrationPhase::Detecting => {} // auto-resolves, no keys
        MigrationPhase::Offer => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = state.migration_choice_list.selected().unwrap_or(0);
                state
                    .migration_choice_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = state.migration_choice_list.selected().unwrap_or(0);
                state
                    .migration_choice_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Esc => {
                state.advance_to_provider();
            }
            KeyCode::Enter => {
                let yes = state.migration_choice_list.selected() == Some(0);
                if yes {
                    state.migration_phase = MigrationPhase::Running;
                    let source_dir = state.openclaw_path.clone().unwrap_or_default();
                    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                    let target_dir = home.join(".openfang");
                    let tx = migrate_tx.clone();
                    std::thread::spawn(move || {
                        let options = openfang_migrate::MigrateOptions {
                            source: openfang_migrate::MigrateSource::OpenClaw,
                            source_dir,
                            target_dir,
                            dry_run: false,
                        };
                        let result =
                            openfang_migrate::run_migration(&options).map_err(|e| format!("{e}"));
                        let _ = tx.send(result);
                    });
                } else {
                    state.advance_to_provider();
                }
            }
            _ => {}
        },
        MigrationPhase::Running => {} // ignore keys while running
        MigrationPhase::Done => {
            if code == KeyCode::Enter {
                state.advance_to_provider();
            }
        }
    }
}

// ── Routing step key handler ───────────────────────────────────────────────

fn handle_routing_key(state: &mut State, code: KeyCode) {
    match state.routing_phase {
        RoutingPhase::Choice => match code {
            KeyCode::Esc => {
                state.step = Step::Model;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = state.routing_choice_list.selected().unwrap_or(0);
                state
                    .routing_choice_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = state.routing_choice_list.selected().unwrap_or(0);
                state
                    .routing_choice_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Enter => {
                let yes = state.routing_choice_list.selected() == Some(0);
                if yes {
                    state.routing_enabled = true;
                    state.auto_select_routing_models();
                    state.routing_phase = RoutingPhase::PickTier(0);
                    state.select_routing_tier_model(0);
                } else {
                    state.routing_enabled = false;
                    save_config(state);
                    state.step = Step::Complete;
                }
            }
            _ => {}
        },
        RoutingPhase::PickTier(tier) => match code {
            KeyCode::Esc => {
                if tier == 0 {
                    state.routing_phase = RoutingPhase::Choice;
                } else {
                    state.routing_phase = RoutingPhase::PickTier(tier - 1);
                    state.select_routing_tier_model(tier - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let len = state.model_entries.len().max(1);
                let i = state.routing_tier_list.selected().unwrap_or(0);
                let next = if i == 0 { len - 1 } else { i - 1 };
                state.routing_tier_list.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = state.model_entries.len().max(1);
                let i = state.routing_tier_list.selected().unwrap_or(0);
                let next = (i + 1) % len;
                state.routing_tier_list.select(Some(next));
            }
            KeyCode::Enter => {
                // Save selected model for this tier
                if let Some(idx) = state.routing_tier_list.selected() {
                    if let Some(entry) = state.model_entries.get(idx) {
                        state.routing_models[tier] = entry.id.clone();
                    }
                }

                if tier < 2 {
                    // Advance to next tier
                    let next_tier = tier + 1;
                    state.routing_phase = RoutingPhase::PickTier(next_tier);
                    state.select_routing_tier_model(next_tier);
                } else {
                    // All 3 tiers picked — save and advance
                    save_config(state);
                    state.step = Step::Complete;
                }
            }
            _ => {}
        },
    }
}

// ── Config save ────────────────────────────────────────────────────────────

fn save_config(state: &mut State) {
    if state.provider().is_none() {
        state.save_error = "No provider selected".to_string();
        return;
    }

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            state.save_error = "Could not determine home directory".to_string();
            return;
        }
    };

    let openfang_dir = home.join(".openfang");
    let _ = std::fs::create_dir_all(openfang_dir.join("agents"));
    let _ = std::fs::create_dir_all(openfang_dir.join("data"));
    crate::restrict_dir_permissions(&openfang_dir);

    let model = if state.model_input.is_empty() {
        state.selected_default_model()
    } else {
        state.model_input.clone()
    };

    let provider_name = state.selected_provider_name();
    let env_var = state.selected_env_var();
    let base_url_line = state
        .selected_base_url()
        .map(|u| format!("base_url = \"{}\"\n", toml_escape(&u)))
        .unwrap_or_default();

    let routing_section = if state.routing_enabled {
        format!(
            r#"
[routing]
simple_model = "{fast}"
medium_model = "{balanced}"
complex_model = "{frontier}"
simple_threshold = 100
complex_threshold = 500
"#,
            fast = state.routing_models[0],
            balanced = state.routing_models[1],
            frontier = state.routing_models[2],
        )
    } else {
        String::new()
    };

    let config_path = openfang_dir.join("config.toml");
    let config = format!(
        r#"# OpenFang Agent OS configuration
# See https://github.com/RightNow-AI/openfang for documentation

api_listen = "127.0.0.1:4200"

[default_model]
provider = "{provider}"
model = "{model}"
api_key_env = "{env_var}"
{base_url_line}

[memory]
decay_rate = 0.05
{routing_section}"#,
        provider = toml_escape(&provider_name),
        model = toml_escape(&model),
        env_var = toml_escape(&env_var),
        base_url_line = base_url_line,
    );

    match std::fs::write(&config_path, &config) {
        Ok(()) => {
            crate::restrict_file_permissions(&config_path);
        }
        Err(e) => {
            state.save_error = format!("Failed to write config: {e}");
            return;
        }
    }

    state.saving_done = true;

    // Auto-start the daemon so all launch options work immediately.
    match crate::start_daemon_background() {
        Ok(url) => {
            state.daemon_started = true;
            state.daemon_url = url;
        }
        Err(e) => {
            state.daemon_error = format!("Daemon failed: {e}");
        }
    }
}

/// Check if the `openfang-desktop` binary exists next to the current exe.
fn find_desktop_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    #[cfg(windows)]
    let name = "openfang-desktop.exe";
    #[cfg(not(windows))]
    let name = "openfang-desktop";

    let path = dir.join(name);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn derive_env_var_from_provider(provider: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for c in provider.chars() {
        let up = c.to_ascii_uppercase();
        if up.is_ascii_alphanumeric() {
            out.push(up);
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "OPENAI_COMPAT_API_KEY".to_string()
    } else {
        format!("{out}_API_KEY")
    }
}

// ── Drawing ────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, area: Rect, state: &mut State) {
    // Fill background
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG_PRIMARY)),
        area,
    );

    // Left-aligned content area (no centered card)
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

    // Header: "OpenFang Init  Step X of 7"
    let header = Line::from(vec![
        Span::styled(
            "OpenFang",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Init", Style::default().fg(theme::TEXT_PRIMARY)),
        Span::styled(format!("  {}", state.step_label()), theme::dim_style()),
    ]);
    f.render_widget(Paragraph::new(header), chunks[1]);

    // Separator
    let sep_w = content.width.min(60) as usize;
    let sep = Line::from(vec![Span::styled(
        "\u{2500}".repeat(sep_w),
        Style::default().fg(theme::BORDER),
    )]);
    f.render_widget(Paragraph::new(sep), chunks[2]);

    // Step content (full remaining area)
    match state.step {
        Step::Welcome => draw_welcome(f, chunks[3]),
        Step::Migration => draw_migration(f, chunks[3], state),
        Step::Provider => draw_provider(f, chunks[3], state),
        Step::Custom => draw_custom(f, chunks[3], state),
        Step::ApiKey => draw_api_key(f, chunks[3], state),
        Step::Model => draw_model(f, chunks[3], state),
        Step::Routing => draw_routing(f, chunks[3], state),
        Step::Complete => draw_complete(f, chunks[3], state),
    }
}

fn draw_welcome(f: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let logo = Paragraph::new(Line::from(vec![Span::styled(
        "O P E N F A N G",
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(logo, chunks[1]);

    let tagline = Paragraph::new(Line::from(vec![Span::styled(
        "Agent Operating System",
        theme::dim_style(),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[2]);

    let sep = Paragraph::new(Line::from(vec![Span::styled(
        "\u{2500}".repeat(area.width.saturating_sub(2) as usize),
        Style::default().fg(theme::BORDER),
    )]));
    f.render_widget(sep, chunks[3]);

    let sec1 = Paragraph::new(Line::from(vec![
        Span::styled("  \u{1f6e1} ", Style::default().fg(theme::GREEN)),
        Span::raw("Sandboxed execution, WASM isolation, SSRF protection"),
    ]));
    f.render_widget(sec1, chunks[5]);

    let sec2 = Paragraph::new(Line::from(vec![
        Span::styled("  \u{1f512} ", Style::default().fg(theme::GREEN)),
        Span::raw("Signed manifests, audit trail, taint tracking"),
    ]));
    f.render_widget(sec2, chunks[6]);

    let sec3 = Paragraph::new(Line::from(vec![
        Span::styled("  \u{1f50d} ", Style::default().fg(theme::GREEN)),
        Span::raw("RBAC, capability checks, secret zeroization"),
    ]));
    f.render_widget(sec3, chunks[7]);

    let sec4 = Paragraph::new(Line::from(vec![
        Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
        Span::raw("API keys never logged, 0600 file permissions"),
    ]));
    f.render_widget(sec4, chunks[8]);

    let sep2 = Paragraph::new(Line::from(vec![Span::styled(
        "\u{2500}".repeat(area.width.saturating_sub(2) as usize),
        Style::default().fg(theme::BORDER),
    )]));
    f.render_widget(sep2, chunks[10]);

    let resp1 = Paragraph::new(Line::from(vec![Span::styled(
        "  Agents can execute code, access the network, and act",
        Style::default().fg(theme::TEXT_SECONDARY),
    )]));
    f.render_widget(resp1, chunks[12]);

    let resp2 = Paragraph::new(Line::from(vec![
        Span::styled(
            "  on your behalf. ",
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            "You are responsible for what they do.",
            Style::default().fg(theme::YELLOW),
        ),
    ]));
    f.render_widget(resp2, chunks[13]);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [Enter] I understand    [Esc] Cancel",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[15]);
}

fn draw_migration(f: &mut Frame, area: Rect, state: &mut State) {
    match state.migration_phase {
        MigrationPhase::Detecting => draw_migration_detecting(f, area, state),
        MigrationPhase::Offer => draw_migration_offer(f, area, state),
        MigrationPhase::Running => draw_migration_running(f, area, state),
        MigrationPhase::Done => draw_migration_done(f, area, state),
    }
}

fn draw_migration_detecting(f: &mut Frame, area: Rect, state: &State) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(spinner, Style::default().fg(theme::ACCENT)),
            Span::raw(" Checking for existing installations..."),
        ])),
        chunks[1],
    );
}

fn draw_migration_offer(f: &mut Frame, area: Rect, state: &mut State) {
    let scan = match &state.openclaw_scan {
        Some(s) => s,
        None => return,
    };

    let path_display = state
        .openclaw_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // Count content lines to determine layout
    let mut content_lines: Vec<Line> = Vec::new();

    if !scan.agents.is_empty() {
        let names: Vec<&str> = scan.agents.iter().map(|a| a.name.as_str()).collect();
        let names_str = names.join(", ");
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
            Span::raw(format!("{} agents ({})", scan.agents.len(), names_str)),
        ]));
    } else {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2500} ", theme::dim_style()),
            Span::styled("No agents", theme::dim_style()),
        ]));
    }

    if !scan.channels.is_empty() {
        let chan_str = scan.channels.join(", ");
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
            Span::raw(format!("{} channels ({})", scan.channels.len(), chan_str)),
        ]));
    } else {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2500} ", theme::dim_style()),
            Span::styled("No channels", theme::dim_style()),
        ]));
    }

    if !scan.skills.is_empty() {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
            Span::raw(format!("{} skills", scan.skills.len())),
        ]));
    } else {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2500} ", theme::dim_style()),
            Span::styled("No skills", theme::dim_style()),
        ]));
    }

    if scan.has_memory {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
            Span::raw("Memory files"),
        ]));
    } else {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2500} ", theme::dim_style()),
            Span::styled("No memory files", theme::dim_style()),
        ]));
    }

    if scan.has_config {
        content_lines.push(Line::from(vec![
            Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
            Span::raw("Configuration"),
        ]));
    }

    let chunks = Layout::vertical([
        Constraint::Length(1),                          // 0: title
        Constraint::Length(1),                          // 1: path
        Constraint::Length(1),                          // 2: separator
        Constraint::Length(content_lines.len() as u16), // 3: scan items
        Constraint::Length(1),                          // 4: separator
        Constraint::Length(1),                          // 5: spacer
        Constraint::Length(1),                          // 6: option yes
        Constraint::Length(1),                          // 7: option no
        Constraint::Min(0),                             // 8: flex
        Constraint::Length(1),                          // 9: hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  OpenClaw Installation Detected",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", path_display),
            theme::dim_style(),
        )])),
        chunks[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  ".to_string() + &"\u{2500}".repeat(area.width.saturating_sub(6) as usize),
            Style::default().fg(theme::BORDER),
        )])),
        chunks[2],
    );

    // Render scan items
    for (i, line) in content_lines.iter().enumerate() {
        if i < chunks[3].height as usize {
            let line_area = Rect {
                x: chunks[3].x,
                y: chunks[3].y + i as u16,
                width: chunks[3].width,
                height: 1,
            };
            f.render_widget(Paragraph::new(line.clone()), line_area);
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  ".to_string() + &"\u{2500}".repeat(area.width.saturating_sub(6) as usize),
            Style::default().fg(theme::BORDER),
        )])),
        chunks[4],
    );

    // Yes / No options
    let options = [("Yes", "migrate settings and data"), ("No", "start fresh")];

    for (i, (label, desc)) in options.iter().enumerate() {
        let selected = state.migration_choice_list.selected() == Some(i);
        let arrow = if selected {
            Span::styled("  \u{25b8} ", Style::default().fg(theme::ACCENT))
        } else {
            Span::raw("    ")
        };
        let label_style = if selected {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_PRIMARY)
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                arrow,
                Span::styled(format!("{:<6}", label), label_style),
                Span::styled(*desc, theme::dim_style()),
            ])),
            chunks[6 + i],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Skip",
            theme::hint_style(),
        )])),
        chunks[9],
    );
}

fn draw_migration_running(f: &mut Frame, area: Rect, state: &State) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(spinner, Style::default().fg(theme::ACCENT)),
            Span::raw(" Migrating from OpenClaw..."),
        ])),
        chunks[1],
    );
}

fn draw_migration_done(f: &mut Frame, area: Rect, state: &State) {
    let mut lines: Vec<Line> = Vec::new();

    if let Some(ref error) = state.migration_error {
        lines.push(Line::from(vec![
            Span::styled("  \u{2718} ", Style::default().fg(theme::RED)),
            Span::raw(format!("Migration failed: {}", error)),
        ]));
    } else if let Some(ref report) = state.migration_report {
        // Group imported items by kind
        use openfang_migrate::report::ItemKind;
        let config_count = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Config)
            .count();
        let agent_items: Vec<&str> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Agent)
            .map(|i| i.name.as_str())
            .collect();
        let channel_items: Vec<&str> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Channel)
            .map(|i| i.name.as_str())
            .collect();
        let memory_count = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Memory)
            .count();
        let skill_count = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Skill)
            .count();
        let session_count = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Session)
            .count();

        if config_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw("Config migrated"),
            ]));
        }

        if !agent_items.is_empty() {
            let names = agent_items.join(", ");
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw(format!("{} agents imported ({})", agent_items.len(), names)),
            ]));
        }

        if !channel_items.is_empty() {
            let names = channel_items.join(", ");
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw(format!("{} channels ({})", channel_items.len(), names)),
            ]));
        }

        if memory_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw("Memory files copied"),
            ]));
        }

        if skill_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw(format!("{} skills imported", skill_count)),
            ]));
        }

        if session_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::raw(format!("{} sessions imported", session_count)),
            ]));
        }

        for skipped in &report.skipped {
            lines.push(Line::from(vec![
                Span::styled("  \u{26a0} ", Style::default().fg(theme::YELLOW)),
                Span::raw(format!("{} skipped ({})", skipped.name, skipped.reason)),
            ]));
        }

        for warning in &report.warnings {
            lines.push(Line::from(vec![
                Span::styled("  \u{26a0} ", Style::default().fg(theme::YELLOW)),
                Span::raw(warning.clone()),
            ]));
        }

        // Summary line
        lines.push(Line::from(vec![Span::styled(
            "  ".to_string() + &"\u{2500}".repeat(area.width.saturating_sub(6) as usize),
            Style::default().fg(theme::BORDER),
        )]));
        lines.push(Line::from(vec![Span::raw(format!(
            "  {} imported, {} skipped, {} warnings",
            report.imported.len(),
            report.skipped.len(),
            report.warnings.len(),
        ))]));
    }

    let content_height = lines.len() as u16;

    let chunks = Layout::vertical([
        Constraint::Length(1),              // 0: spacer
        Constraint::Length(content_height), // 1: results
        Constraint::Length(1),              // 2: spacer
        Constraint::Min(0),                 // 3: flex
        Constraint::Length(1),              // 4: hints
    ])
    .split(area);

    // Render result lines
    for (i, line) in lines.iter().enumerate() {
        if i < chunks[1].height as usize {
            let line_area = Rect {
                x: chunks[1].x,
                y: chunks[1].y + i as u16,
                width: chunks[1].width,
                height: 1,
            };
            f.render_widget(Paragraph::new(line.clone()), line_area);
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  [Enter] Continue  ", theme::hint_style()),
            Span::styled("(auto-advancing...)", theme::dim_style()),
        ])),
        chunks[4],
    );
}

fn draw_provider(f: &mut Frame, area: Rect, state: &mut State) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new(Line::from(vec![Span::raw("  Choose your LLM provider:")]));
    f.render_widget(prompt, chunks[0]);

    let items: Vec<ListItem> = state
        .provider_order
        .iter()
        .map(|&idx| {
            let p = &PROVIDERS[idx];
            let detected = state.is_provider_detected(idx);
            let icon = if detected {
                Span::styled("\u{25cf} ", Style::default().fg(theme::GREEN))
            } else if !p.needs_key {
                Span::styled("\u{25cb} ", Style::default().fg(theme::BLUE))
            } else {
                Span::styled("  ", Style::default())
            };
            let name_span = Span::raw(format!("{:<14}", p.display));
            let hint_text = if detected {
                format!("{} detected", p.env_var)
            } else if !p.needs_key {
                "local, no key needed".to_string()
            } else if !p.hint.is_empty() {
                format!("requires {} ({})", p.env_var, p.hint)
            } else {
                format!("requires {}", p.env_var)
            };
            ListItem::new(Line::from(vec![
                icon,
                name_span,
                Span::styled(hint_text, theme::dim_style()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("\u{25b8} ");
    f.render_stateful_widget(list, chunks[1], &mut state.provider_list);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [\u{2191}\u{2193}/jk] Navigate  [Enter] Select  [Esc] Cancel",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_api_key(f: &mut Frame, area: Rect, state: &mut State) {
    let provider_display = state.selected_provider_display();
    let env_var = state.selected_env_var();

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new(Line::from(vec![Span::raw(format!(
        "  Enter your {} API key:",
        provider_display
    ))]));
    f.render_widget(prompt, chunks[0]);

    match state.key_test {
        KeyTestState::Idle => {
            let masked: String = "\u{2022}".repeat(state.api_key_input.len());
            let input = Paragraph::new(Line::from(vec![
                Span::raw("  \u{25b8} "),
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
                format!("    Or set {} environment variable", env_var),
                theme::dim_style(),
            )]));
            f.render_widget(env_hint, chunks[3]);
        }
        KeyTestState::Testing => {
            let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
            let input = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(spinner, Style::default().fg(theme::ACCENT)),
                Span::raw(" Testing API key..."),
            ]));
            f.render_widget(input, chunks[1]);
        }
        KeyTestState::Ok => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                    Span::raw("API key verified"),
                ])),
                chunks[1],
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    "    Saved to ~/.openfang/.env",
                    theme::dim_style(),
                )])),
                chunks[3],
            );
        }
        KeyTestState::Warn => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  \u{26a0} ", Style::default().fg(theme::YELLOW)),
                    Span::raw("Could not verify (may still work)"),
                ])),
                chunks[1],
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    "    Saved to ~/.openfang/.env",
                    theme::dim_style(),
                )])),
                chunks[3],
            );
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Enter] Confirm  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[5],
    );
}

fn draw_model(f: &mut Frame, area: Rect, state: &mut State) {
    let provider_display = state.selected_provider_display();
    let default_model = state.selected_default_model();

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(format!(
            "  Choose default model for {}:",
            provider_display
        ))])),
        chunks[0],
    );

    let items = build_model_list_items(&state.model_entries, Some(default_model.as_str()));
    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("\u{25b8} ");
    f.render_stateful_widget(list, chunks[1], &mut state.model_list);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}/jk] Navigate  [Enter] Select  [Esc] Back    * = default",
            theme::hint_style(),
        )])),
        chunks[2],
    );
}

fn draw_routing(f: &mut Frame, area: Rect, state: &mut State) {
    match state.routing_phase {
        RoutingPhase::Choice => draw_routing_choice(f, area, state),
        RoutingPhase::PickTier(tier) => draw_routing_pick(f, area, state, tier),
    }
}

fn draw_routing_choice(f: &mut Frame, area: Rect, state: &mut State) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // title
        Constraint::Length(1), // description 1
        Constraint::Length(1), // description 2
        Constraint::Length(1), // description 3
        Constraint::Length(1), // spacer
        Constraint::Length(1), // separator
        Constraint::Length(1), // spacer
        Constraint::Length(1), // option yes
        Constraint::Length(1), // option no
        Constraint::Min(0),
        Constraint::Length(1), // hints
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Smart Model Routing",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Automatically picks the right model per task complexity.",
            theme::dim_style(),
        )])),
        chunks[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Simple tasks use cheap/fast models, complex tasks use",
            theme::dim_style(),
        )])),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  frontier models. Saves cost without sacrificing quality.",
            theme::dim_style(),
        )])),
        chunks[3],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "\u{2500}".repeat(area.width.saturating_sub(2) as usize),
            Style::default().fg(theme::BORDER),
        )])),
        chunks[5],
    );

    let options = [
        ("Yes", "pick 3 models (fast / balanced / frontier)"),
        ("No", "use one model for everything"),
    ];

    for (i, (label, desc)) in options.iter().enumerate() {
        let selected = state.routing_choice_list.selected() == Some(i);
        let arrow = if selected {
            Span::styled("  \u{25b8} ", Style::default().fg(theme::ACCENT))
        } else {
            Span::raw("    ")
        };
        let label_style = if selected {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_PRIMARY)
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                arrow,
                Span::styled(format!("{:<6}", label), label_style),
                Span::styled(*desc, theme::dim_style()),
            ])),
            chunks[7 + i],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[10],
    );
}

fn draw_routing_pick(f: &mut Frame, area: Rect, state: &mut State, tier: usize) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // tier label
        Constraint::Length(1), // tier description
        Constraint::Length(1), // spacer + current selections
        Constraint::Min(3),    // model list
        Constraint::Length(1), // hints
    ])
    .split(area);

    // Tier header with colored label
    let tier_color = match tier {
        0 => theme::GREEN,
        1 => theme::YELLOW,
        _ => theme::PURPLE,
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  Pick "),
            Span::styled(
                ROUTING_TIER_NAMES[tier],
                Style::default().fg(tier_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" model ({}/3):", tier + 1)),
        ])),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", ROUTING_TIER_DESC[tier]),
            theme::dim_style(),
        )])),
        chunks[1],
    );

    // Show already-picked tiers as summary
    let tier_colors = [theme::GREEN, theme::YELLOW, theme::PURPLE];
    let mut summary_spans: Vec<Span> = vec![Span::raw("  ")];
    for (t, (name, c)) in ROUTING_TIER_NAMES
        .iter()
        .zip(tier_colors.iter())
        .enumerate()
    {
        if t == tier {
            summary_spans.push(Span::styled(
                format!("[{name}]"),
                Style::default().fg(*c).add_modifier(Modifier::BOLD),
            ));
        } else if t < tier {
            // Already picked — show short model name
            let short = state.routing_models[t]
                .split('/')
                .next_back()
                .unwrap_or(&state.routing_models[t]);
            let display = if short.len() > 14 {
                &short[..14]
            } else {
                short
            };
            summary_spans.push(Span::styled(
                format!("{name}:{display}"),
                Style::default().fg(*c),
            ));
        } else {
            summary_spans.push(Span::styled(*name, theme::dim_style()));
        }
        if t < 2 {
            summary_spans.push(Span::raw("  "));
        }
    }
    f.render_widget(Paragraph::new(Line::from(summary_spans)), chunks[2]);

    // Reuse the same model list as Model step
    let items = build_model_list_items(&state.model_entries, None);
    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("\u{25b8} ");
    f.render_stateful_widget(list, chunks[3], &mut state.routing_tier_list);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}/jk] Navigate  [Enter] Select  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[4],
    );
}

/// Build list items for the model picker (shared between Model and Routing steps).
fn build_model_list_items<'a>(
    entries: &'a [ModelEntry],
    default_id: Option<&str>,
) -> Vec<ListItem<'a>> {
    entries
        .iter()
        .map(|entry| {
            let is_default = default_id.is_some_and(|d| entry.id == d);
            let default_marker = if is_default {
                Span::styled(" *", Style::default().fg(theme::GREEN))
            } else {
                Span::raw("  ")
            };

            let tier_style = match entry.tier {
                "frontier" => Style::default().fg(theme::PURPLE),
                "smart" => Style::default().fg(theme::BLUE),
                "balanced" => Style::default().fg(theme::YELLOW),
                "fast" => Style::default().fg(theme::GREEN),
                "local" => Style::default().fg(theme::TEXT_SECONDARY),
                _ => theme::dim_style(),
            };

            let cost_text = if entry.cost.is_empty() {
                String::new()
            } else {
                format!("  {}", entry.cost)
            };

            ListItem::new(Line::from(vec![
                Span::raw(format!("  {:<32}", entry.display_name)),
                Span::styled(entry.tier, tier_style),
                Span::styled(cost_text, theme::dim_style()),
                default_marker,
            ]))
        })
        .collect()
}

fn draw_complete(f: &mut Frame, area: Rect, state: &mut State) {
    let model = if state.model_input.is_empty() {
        state.selected_default_model()
    } else {
        state.model_input.clone()
    };
    let provider_display = state.selected_provider_display();

    let has_desktop = find_desktop_binary().is_some();

    let chunks = Layout::vertical([
        Constraint::Length(1), // 0: spacer
        Constraint::Length(1), // 1: status line
        Constraint::Length(1), // 2: spacer
        Constraint::Length(1), // 3: provider
        Constraint::Length(1), // 4: model
        Constraint::Length(1), // 5: daemon
        Constraint::Length(1), // 6: spacer
        Constraint::Length(1), // 7: separator
        Constraint::Length(1), // 8: spacer
        Constraint::Length(1), // 9: question
        Constraint::Length(1), // 10: spacer
        Constraint::Length(1), // 11: option 1 — Desktop
        Constraint::Length(1), // 12: option 2 — Dashboard
        Constraint::Length(1), // 13: option 3 — Chat
        Constraint::Min(0),    // 14: flex
        Constraint::Length(1), // 15: hints
    ])
    .split(area);

    // ── Status line ──
    if !state.save_error.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  \u{2718} ", Style::default().fg(theme::RED)),
                Span::raw(&state.save_error),
            ])),
            chunks[1],
        );
    } else if state.daemon_started {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::styled(
                    "Setup complete \u{2014} daemon running",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            chunks[1],
        );
    } else if !state.daemon_error.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  \u{26a0} ", Style::default().fg(theme::YELLOW)),
                Span::styled(
                    "Setup complete \u{2014} ",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&state.daemon_error, Style::default().fg(theme::YELLOW)),
            ])),
            chunks[1],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                Span::styled(
                    "Setup complete!",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            chunks[1],
        );
    }

    // ── Summary KVs ──
    let kv_style = theme::dim_style();
    let val_style = Style::default().fg(theme::TEXT_PRIMARY);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Provider:    ", kv_style),
            Span::styled(provider_display, val_style),
        ])),
        chunks[3],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Model:       ", kv_style),
            Span::styled(model, val_style),
        ])),
        chunks[4],
    );

    let daemon_text = if state.daemon_started {
        format!("running at {}", state.daemon_url)
    } else if !state.daemon_error.is_empty() {
        "not running".to_string()
    } else {
        "pending".to_string()
    };
    let daemon_color = if state.daemon_started {
        theme::GREEN
    } else {
        theme::YELLOW
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Daemon:      ", kv_style),
            Span::styled(daemon_text, Style::default().fg(daemon_color)),
        ])),
        chunks[5],
    );

    // ── Separator ──
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  ".to_string() + &"\u{2500}".repeat(area.width.saturating_sub(6) as usize),
            Style::default().fg(theme::BORDER),
        )])),
        chunks[7],
    );

    // ── Question ──
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  How do you want to use OpenFang?",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[9],
    );

    // ── Options ──
    let desktop_hint = if has_desktop {
        "native window with system tray"
    } else {
        "not installed"
    };

    let options: [(&str, &str, &str); 3] = [
        ("Desktop app", "(recommended)", desktop_hint),
        ("Web dashboard", "", "opens in your default browser"),
        ("Terminal chat", "", "interactive chat right here"),
    ];

    for (i, (label, badge, desc)) in options.iter().enumerate() {
        let selected = state.complete_list.selected() == Some(i);
        let num = format!("[{}]", i + 1);

        let arrow = if selected {
            Span::styled("  \u{25b8} ", Style::default().fg(theme::ACCENT))
        } else {
            Span::raw("    ")
        };

        let num_style = if selected {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::dim_style()
        };

        let label_style = if i == 0 && !has_desktop {
            // Grey out desktop option if binary not found
            theme::dim_style()
        } else if selected {
            Style::default()
                .fg(theme::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_PRIMARY)
        };

        let badge_span = if badge.is_empty() {
            Span::raw("")
        } else {
            Span::styled(format!(" {badge}"), Style::default().fg(theme::GREEN))
        };

        let desc_span = if i == 0 && !has_desktop {
            Span::styled(format!("  {desc}"), Style::default().fg(theme::YELLOW))
        } else {
            Span::styled(format!("  {desc}"), theme::dim_style())
        };

        f.render_widget(
            Paragraph::new(Line::from(vec![
                arrow,
                Span::styled(num, num_style),
                Span::raw(" "),
                Span::styled(*label, label_style),
                badge_span,
                desc_span,
            ])),
            chunks[11 + i],
        );
    }

    // ── Bottom hints ──
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [\u{2191}\u{2193}/jk] Navigate  [Enter] Launch  [1/2/3] Quick select",
            theme::hint_style(),
        )])),
        chunks[15],
    );
}

fn draw_custom(f: &mut Frame, area: Rect, state: &mut State) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // title
        Constraint::Length(1), // provider
        Constraint::Length(1), // env
        Constraint::Length(1), // base_url
        Constraint::Length(1), // model
        Constraint::Length(1), // hint
        Constraint::Min(0),
        Constraint::Length(1), // controls
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(
            "  Configure custom OpenAI-compatible provider:",
        )])),
        chunks[0],
    );

    let field_style = |idx: usize, current: usize| {
        if idx == current {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_PRIMARY)
        }
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Provider: ", field_style(0, state.custom_field)),
            Span::styled(&state.custom_provider_name, theme::input_style()),
        ])),
        chunks[1],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  API env:  ", theme::dim_style()),
            Span::styled(&state.custom_env_var, theme::dim_style()),
            Span::styled(
                "  (API key entered next step)",
                theme::dim_style(),
            ),
        ])),
        chunks[2],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Base URL: ", field_style(1, state.custom_field)),
            Span::styled(&state.custom_base_url, theme::input_style()),
        ])),
        chunks[3],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Model:    ", field_style(2, state.custom_field)),
            Span::styled(&state.custom_model, theme::input_style()),
        ])),
        chunks[4],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Example base_url: http://localhost:8000/v1",
            theme::dim_style(),
        )])),
        chunks[5],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Tab/\u{2191}\u{2193}] Field  [↵] Next/Confirm  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[7],
    );
}
