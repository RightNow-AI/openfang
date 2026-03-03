//! Agent selection + creation: list running agents, template picker, custom builder.
//! Overhauled with search/filter, state badges, detail view, and new actions.

use crate::templates::{self, AgentTemplate};
use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

/// Available built-in tools for the custom agent builder.
const TOOL_OPTIONS: &[(&str, &str)] = &[
    ("file_read", "Read files"),
    ("file_write", "Write files"),
    ("file_list", "List directory contents"),
    ("memory_store", "Store data in agent memory"),
    ("memory_recall", "Recall data from memory"),
    ("web_fetch", "Fetch web pages"),
    ("shell_exec", "Execute shell commands"),
    ("agent_send", "Send messages to other agents"),
    ("agent_list", "List running agents"),
];

const DEFAULT_TOOLS: &[bool] = &[true, false, true, true, true, true, false, false, false];

#[derive(Clone, PartialEq, Eq)]
pub enum AgentSubScreen {
    /// Pick an existing agent or "create new"
    AgentList,
    /// View agent detail
    AgentDetail,
    /// Pick creation method: template or custom
    CreateMethod,
    /// Pick a template
    TemplatePicker,
    /// Custom builder: name
    CustomName,
    /// Custom builder: description
    CustomDesc,
    /// Custom builder: system prompt
    CustomPrompt,
    /// Custom builder: tool selection
    CustomTools,
    /// Custom builder: skill selection
    CustomSkills,
    /// Custom builder: MCP server selection
    CustomMcpServers,
    /// Edit skills for existing agent
    EditSkills,
    /// Edit MCP servers for existing agent
    EditMcpServers,
    /// Spawning agent (waiting for result)
    Spawning,
}

pub struct AgentSelectState {
    pub sub: AgentSubScreen,
    pub list: ListState,

    // Daemon mode
    pub daemon_agents: Vec<DaemonAgent>,

    // In-process mode
    pub inprocess_agents: Vec<InProcessAgent>,

    // Search/filter
    pub search_active: bool,
    pub search_query: String,
    filtered_indices: Vec<usize>, // indices into combined agent list

    // Detail view
    pub detail: Option<AgentDetail>,

    // Create method
    pub create_method_list: ListState,

    // Template picker
    pub templates: Vec<AgentTemplate>,
    pub template_list: ListState,

    // Custom builder
    pub custom_name: String,
    pub custom_desc: String,
    pub custom_prompt: String,
    pub tool_checks: Vec<bool>,
    pub tool_cursor: usize,

    // Skill/MCP editor (shared by creation wizard + detail editor)
    pub available_skills: Vec<(String, bool)>,
    pub skill_cursor: usize,
    pub available_mcp: Vec<(String, bool)>,
    pub mcp_cursor: usize,

    // Result
    pub spawned_toml: Option<String>,
    pub status_msg: String,
}

#[derive(Clone)]
pub struct DaemonAgent {
    pub id: String,
    pub name: String,
    pub state: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone)]
pub struct InProcessAgent {
    pub id: openfang_types::agent::AgentId,
    pub name: String,
    pub state: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Default)]
pub struct AgentDetail {
    pub id: String,
    pub name: String,
    pub state: String,
    pub model: String,
    pub provider: String,
    pub created: String,
    pub last_active: String,
    pub tags: Vec<String>,
    pub capabilities: Vec<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub skills: Vec<String>,
    pub skills_mode: String,
    pub mcp_servers: Vec<String>,
    pub mcp_servers_mode: String,
}

/// What the agent screen decided.
pub enum AgentAction {
    /// No action yet, keep rendering.
    Continue,
    /// User created a new agent manifest (TOML).
    CreatedManifest(String),
    /// User pressed Esc from the top-level list.
    Back,
    /// User wants to chat with a specific agent (from detail view).
    ChatWithAgent { id: String, name: String },
    /// User wants to kill an agent (from detail view).
    KillAgent(String),
    /// Update skills for an agent.
    UpdateSkills { id: String, skills: Vec<String> },
    /// Update MCP servers for an agent.
    UpdateMcpServers { id: String, servers: Vec<String> },
    /// Fetch skills/mcp data for an agent.
    FetchAgentSkills(String),
    /// Fetch MCP data for an agent.
    FetchAgentMcpServers(String),
}

impl AgentSelectState {
    pub fn new() -> Self {
        Self {
            sub: AgentSubScreen::AgentList,
            list: ListState::default(),
            daemon_agents: Vec::new(),
            inprocess_agents: Vec::new(),
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            detail: None,
            create_method_list: ListState::default(),
            templates: Vec::new(),
            template_list: ListState::default(),
            custom_name: String::new(),
            custom_desc: String::new(),
            custom_prompt: String::new(),
            tool_checks: DEFAULT_TOOLS.to_vec(),
            tool_cursor: 0,
            available_skills: Vec::new(),
            skill_cursor: 0,
            available_mcp: Vec::new(),
            mcp_cursor: 0,
            spawned_toml: None,
            status_msg: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.sub = AgentSubScreen::AgentList;
        self.list.select(Some(0));
        self.create_method_list.select(Some(0));
        self.template_list.select(Some(0));
        self.custom_name.clear();
        self.custom_desc.clear();
        self.custom_prompt.clear();
        self.tool_checks = DEFAULT_TOOLS.to_vec();
        self.tool_cursor = 0;
        self.available_skills.clear();
        self.skill_cursor = 0;
        self.available_mcp.clear();
        self.mcp_cursor = 0;
        self.spawned_toml = None;
        self.status_msg.clear();
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
        self.detail = None;
    }

    /// Load daemon agents from the daemon API.
    pub fn load_daemon_agents(&mut self, base_url: &str) {
        let client = crate::daemon_client();
        if let Ok(resp) = client.get(format!("{base_url}/api/agents")).send() {
            if let Ok(body) = resp.json::<serde_json::Value>() {
                self.daemon_agents.clear();
                if let Some(arr) = body.as_array() {
                    for a in arr {
                        self.daemon_agents.push(DaemonAgent {
                            id: a["id"].as_str().unwrap_or("?").to_string(),
                            name: a["name"].as_str().unwrap_or("?").to_string(),
                            state: a["state"].as_str().unwrap_or("?").to_string(),
                            provider: a["model_provider"].as_str().unwrap_or("?").to_string(),
                            model: a["model_name"].as_str().unwrap_or("?").to_string(),
                        });
                    }
                }
            }
        }
        self.rebuild_filter();
        self.list.select(Some(0));
    }

    /// Load in-process agents from the kernel.
    pub fn load_inprocess_agents(&mut self, kernel: &openfang_kernel::OpenFangKernel) {
        self.inprocess_agents.clear();
        for entry in kernel.registry.list() {
            self.inprocess_agents.push(InProcessAgent {
                id: entry.id,
                name: entry.name.clone(),
                state: format!("{:?}", entry.state),
                provider: entry.manifest.model.provider.clone(),
                model: entry.manifest.model.model.clone(),
            });
        }
        self.rebuild_filter();
        self.list.select(Some(0));
    }

    fn total_agents(&self) -> usize {
        self.daemon_agents.len() + self.inprocess_agents.len()
    }

    /// Visible items: filtered agents + "Create new" item.
    fn visible_count(&self) -> usize {
        if self.search_query.is_empty() {
            self.total_agents() + 1
        } else {
            self.filtered_indices.len() + 1
        }
    }

    fn rebuild_filter(&mut self) {
        self.filtered_indices.clear();
        if self.search_query.is_empty() {
            return;
        }
        let q = self.search_query.to_lowercase();
        let total = self.total_agents();
        for i in 0..total {
            let (name, model, tags) = self.agent_info_at(i);
            if name.to_lowercase().contains(&q)
                || model.to_lowercase().contains(&q)
                || tags.to_lowercase().contains(&q)
            {
                self.filtered_indices.push(i);
            }
        }
    }

    /// Get display info for the agent at combined index.
    fn agent_info_at(&self, combined_idx: usize) -> (String, String, String) {
        let daemon_count = self.daemon_agents.len();
        if combined_idx < daemon_count {
            let a = &self.daemon_agents[combined_idx];
            (
                a.name.clone(),
                format!("{}/{}", a.provider, a.model),
                String::new(),
            )
        } else {
            let local_idx = combined_idx - daemon_count;
            if local_idx < self.inprocess_agents.len() {
                let a = &self.inprocess_agents[local_idx];
                (
                    a.name.clone(),
                    format!("{}/{}", a.provider, a.model),
                    String::new(),
                )
            } else {
                (String::new(), String::new(), String::new())
            }
        }
    }

    /// Map a visible list index to a combined agent index.
    fn visible_to_combined(&self, visible_idx: usize) -> Option<usize> {
        if self.search_query.is_empty() {
            if visible_idx < self.total_agents() {
                Some(visible_idx)
            } else {
                None // "Create new"
            }
        } else if visible_idx < self.filtered_indices.len() {
            Some(self.filtered_indices[visible_idx])
        } else {
            None // "Create new"
        }
    }

    fn load_templates(&mut self) {
        if self.templates.is_empty() {
            self.templates = templates::load_all_templates();
        }
        self.template_list.select(Some(0));
    }

    /// Build detail from daemon agent.
    fn build_detail_daemon(&self, idx: usize) -> AgentDetail {
        let a = &self.daemon_agents[idx];
        AgentDetail {
            id: a.id.clone(),
            name: a.name.clone(),
            state: a.state.clone(),
            model: a.model.clone(),
            provider: a.provider.clone(),
            ..Default::default()
        }
    }

    /// Build detail from in-process agent.
    fn build_detail_inprocess(&self, idx: usize) -> AgentDetail {
        let a = &self.inprocess_agents[idx];
        AgentDetail {
            id: format!("{}", a.id),
            name: a.name.clone(),
            state: a.state.clone(),
            model: a.model.clone(),
            provider: a.provider.clone(),
            ..Default::default()
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AgentAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return AgentAction::Back;
        }

        match self.sub {
            AgentSubScreen::AgentList => self.handle_agent_list(key),
            AgentSubScreen::AgentDetail => self.handle_detail(key),
            AgentSubScreen::CreateMethod => self.handle_create_method(key),
            AgentSubScreen::TemplatePicker => self.handle_template_picker(key),
            AgentSubScreen::CustomName => self.handle_custom_name(key),
            AgentSubScreen::CustomDesc => self.handle_custom_desc(key),
            AgentSubScreen::CustomPrompt => self.handle_custom_prompt(key),
            AgentSubScreen::CustomTools => self.handle_custom_tools(key),
            AgentSubScreen::CustomSkills => self.handle_custom_skills(key),
            AgentSubScreen::CustomMcpServers => self.handle_custom_mcp_servers(key),
            AgentSubScreen::EditSkills => self.handle_edit_skills(key),
            AgentSubScreen::EditMcpServers => self.handle_edit_mcp_servers(key),
            AgentSubScreen::Spawning => AgentAction::Continue,
        }
    }

    fn handle_agent_list(&mut self, key: KeyEvent) -> AgentAction {
        // Search mode input
        if self.search_active {
            match key.code {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.rebuild_filter();
                    self.list.select(Some(0));
                    return AgentAction::Continue;
                }
                KeyCode::Enter => {
                    self.search_active = false;
                    return AgentAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.rebuild_filter();
                    self.list.select(Some(0));
                    return AgentAction::Continue;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.rebuild_filter();
                    self.list.select(Some(0));
                    return AgentAction::Continue;
                }
                _ => return AgentAction::Continue,
            }
        }

        let total = self.visible_count();
        if total == 0 {
            return AgentAction::Continue;
        }

        match key.code {
            KeyCode::Esc => return AgentAction::Back,
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                return AgentAction::Continue;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list.selected().unwrap_or(0);
                let next = if i == 0 { total - 1 } else { i - 1 };
                self.list.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.list.selected().unwrap_or(0);
                let next = (i + 1) % total;
                self.list.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(vis_idx) = self.list.selected() {
                    match self.visible_to_combined(vis_idx) {
                        Some(combined) => {
                            // Open detail view
                            let daemon_count = self.daemon_agents.len();
                            if combined < daemon_count {
                                self.detail = Some(self.build_detail_daemon(combined));
                            } else {
                                let local = combined - daemon_count;
                                if local < self.inprocess_agents.len() {
                                    self.detail = Some(self.build_detail_inprocess(local));
                                }
                            }
                            self.sub = AgentSubScreen::AgentDetail;
                        }
                        None => {
                            // "Create new"
                            self.create_method_list.select(Some(0));
                            self.sub = AgentSubScreen::CreateMethod;
                        }
                    }
                }
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_detail(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::AgentList;
            }
            KeyCode::Char('c') => {
                // Chat with this agent
                if let Some(ref detail) = self.detail {
                    return AgentAction::ChatWithAgent {
                        id: detail.id.clone(),
                        name: detail.name.clone(),
                    };
                }
            }
            KeyCode::Char('k') => {
                // Kill this agent
                if let Some(ref detail) = self.detail {
                    return AgentAction::KillAgent(detail.id.clone());
                }
            }
            KeyCode::Char('s') => {
                // Edit skills for this agent
                if let Some(ref detail) = self.detail {
                    let id = detail.id.clone();
                    self.sub = AgentSubScreen::EditSkills;
                    return AgentAction::FetchAgentSkills(id);
                }
            }
            KeyCode::Char('m') => {
                // Edit MCP servers for this agent
                if let Some(ref detail) = self.detail {
                    let id = detail.id.clone();
                    self.sub = AgentSubScreen::EditMcpServers;
                    return AgentAction::FetchAgentMcpServers(id);
                }
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_create_method(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::AgentList;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.create_method_list.selected().unwrap_or(0);
                self.create_method_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.create_method_list.selected().unwrap_or(0);
                self.create_method_list
                    .select(Some(if i == 0 { 1 } else { 0 }));
            }
            KeyCode::Enter => {
                match self.create_method_list.selected() {
                    Some(0) => {
                        self.load_templates();
                        if self.templates.is_empty() {
                            // No templates, go straight to custom
                            self.custom_name.clear();
                            self.sub = AgentSubScreen::CustomName;
                        } else {
                            self.sub = AgentSubScreen::TemplatePicker;
                        }
                    }
                    Some(1) => {
                        self.custom_name.clear();
                        self.custom_desc.clear();
                        self.custom_prompt.clear();
                        self.tool_checks = DEFAULT_TOOLS.to_vec();
                        self.tool_cursor = 0;
                        self.sub = AgentSubScreen::CustomName;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_template_picker(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CreateMethod;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.template_list.selected().unwrap_or(0);
                let total = self.templates.len();
                let next = if i == 0 {
                    total.saturating_sub(1)
                } else {
                    i - 1
                };
                self.template_list.select(Some(next));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.template_list.selected().unwrap_or(0);
                let next = (i + 1) % self.templates.len().max(1);
                self.template_list.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(idx) = self.template_list.selected() {
                    if idx < self.templates.len() {
                        let toml = self.templates[idx].content.clone();
                        return AgentAction::CreatedManifest(toml);
                    }
                }
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_name(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CreateMethod;
            }
            KeyCode::Enter => {
                if !self.custom_name.is_empty() {
                    if self.custom_desc.is_empty() {
                        self.custom_desc = format!("A custom {} agent", self.custom_name);
                    }
                    self.sub = AgentSubScreen::CustomDesc;
                }
            }
            KeyCode::Char(c) => {
                self.custom_name.push(c);
            }
            KeyCode::Backspace => {
                self.custom_name.pop();
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_desc(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CustomName;
            }
            KeyCode::Enter => {
                if self.custom_prompt.is_empty() {
                    self.custom_prompt = format!("You are {}, a helpful agent.", self.custom_name);
                }
                self.sub = AgentSubScreen::CustomPrompt;
            }
            KeyCode::Char(c) => {
                self.custom_desc.push(c);
            }
            KeyCode::Backspace => {
                self.custom_desc.pop();
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_prompt(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CustomDesc;
            }
            KeyCode::Enter => {
                self.sub = AgentSubScreen::CustomTools;
            }
            KeyCode::Char(c) => {
                self.custom_prompt.push(c);
            }
            KeyCode::Backspace => {
                self.custom_prompt.pop();
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_tools(&mut self, key: KeyEvent) -> AgentAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CustomPrompt;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.tool_cursor > 0 {
                    self.tool_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.tool_cursor < TOOL_OPTIONS.len() - 1 {
                    self.tool_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                self.tool_checks[self.tool_cursor] = !self.tool_checks[self.tool_cursor];
            }
            KeyCode::Enter => {
                // Advance to skill selection (populate with all unchecked = "all skills" mode)
                if self.available_skills.is_empty() {
                    // Pre-populate on first entry (will be empty until backend fills it)
                    // Default: all unchecked = use all skills
                }
                self.skill_cursor = 0;
                self.sub = AgentSubScreen::CustomSkills;
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_skills(&mut self, key: KeyEvent) -> AgentAction {
        let len = self.available_skills.len();
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CustomTools;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.skill_cursor > 0 {
                    self.skill_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 && self.skill_cursor < len - 1 {
                    self.skill_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if len > 0 {
                    let checked = &mut self.available_skills[self.skill_cursor].1;
                    *checked = !*checked;
                }
            }
            KeyCode::Enter => {
                // Advance to MCP server selection
                self.mcp_cursor = 0;
                self.sub = AgentSubScreen::CustomMcpServers;
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_custom_mcp_servers(&mut self, key: KeyEvent) -> AgentAction {
        let len = self.available_mcp.len();
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::CustomSkills;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.mcp_cursor > 0 {
                    self.mcp_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 && self.mcp_cursor < len - 1 {
                    self.mcp_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if len > 0 {
                    let checked = &mut self.available_mcp[self.mcp_cursor].1;
                    *checked = !*checked;
                }
            }
            KeyCode::Enter => {
                let toml = self.build_custom_toml();
                return AgentAction::CreatedManifest(toml);
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_edit_skills(&mut self, key: KeyEvent) -> AgentAction {
        let len = self.available_skills.len();
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::AgentDetail;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.skill_cursor > 0 {
                    self.skill_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 && self.skill_cursor < len - 1 {
                    self.skill_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if len > 0 {
                    let checked = &mut self.available_skills[self.skill_cursor].1;
                    *checked = !*checked;
                }
            }
            KeyCode::Enter => {
                // Save — collect checked skill names (none checked = "all")
                if let Some(ref detail) = self.detail {
                    let skills: Vec<String> = self
                        .available_skills
                        .iter()
                        .filter(|(_, checked)| *checked)
                        .map(|(name, _)| name.clone())
                        .collect();
                    return AgentAction::UpdateSkills {
                        id: detail.id.clone(),
                        skills,
                    };
                }
                self.sub = AgentSubScreen::AgentDetail;
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn handle_edit_mcp_servers(&mut self, key: KeyEvent) -> AgentAction {
        let len = self.available_mcp.len();
        match key.code {
            KeyCode::Esc => {
                self.sub = AgentSubScreen::AgentDetail;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.mcp_cursor > 0 {
                    self.mcp_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 && self.mcp_cursor < len - 1 {
                    self.mcp_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if len > 0 {
                    let checked = &mut self.available_mcp[self.mcp_cursor].1;
                    *checked = !*checked;
                }
            }
            KeyCode::Enter => {
                // Save — collect checked server names (none checked = "all")
                if let Some(ref detail) = self.detail {
                    let servers: Vec<String> = self
                        .available_mcp
                        .iter()
                        .filter(|(_, checked)| *checked)
                        .map(|(name, _)| name.clone())
                        .collect();
                    return AgentAction::UpdateMcpServers {
                        id: detail.id.clone(),
                        servers,
                    };
                }
                self.sub = AgentSubScreen::AgentDetail;
            }
            _ => {}
        }
        AgentAction::Continue
    }

    fn build_custom_toml(&self) -> String {
        let tools_str: String = TOOL_OPTIONS
            .iter()
            .zip(self.tool_checks.iter())
            .filter(|(_, &checked)| checked)
            .map(|((name, _), _)| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");

        let selected_skills: Vec<String> = self
            .available_skills
            .iter()
            .filter(|(_, checked)| *checked)
            .map(|(name, _)| format!("\"{}\"", name))
            .collect();
        let skills_str = selected_skills.join(", ");

        let selected_mcp: Vec<String> = self
            .available_mcp
            .iter()
            .filter(|(_, checked)| *checked)
            .map(|(name, _)| format!("\"{}\"", name))
            .collect();
        let mcp_str = selected_mcp.join(", ");

        format!(
            r#"name = "{name}"
version = "0.1.0"
description = "{desc}"
author = "user"
module = "builtin:chat"
tags = ["custom"]
skills = [{skills_str}]
mcp_servers = [{mcp_str}]

[model]
max_tokens = 8192
temperature = 0.5
system_prompt = """{prompt}"""

[resources]
max_llm_tokens_per_hour = 200000
max_concurrent_tools = 10

[capabilities]
tools = [{tools_str}]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            name = self.custom_name,
            desc = self.custom_desc,
            prompt = self.custom_prompt,
        )
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

/// Render the agent screen.
pub fn draw(f: &mut Frame, area: Rect, state: &mut AgentSelectState) {
    // Clear background
    f.render_widget(Block::default(), area);

    match state.sub {
        AgentSubScreen::AgentDetail => {
            draw_detail(f, area, state);
            return;
        }
        AgentSubScreen::AgentList => {
            draw_agent_list_full(f, area, state);
            return;
        }
        AgentSubScreen::EditSkills | AgentSubScreen::EditMcpServers => {
            draw_edit_allowlist(f, area, state);
            return;
        }
        _ => {}
    }

    let sub_title = match state.sub {
        AgentSubScreen::AgentList
        | AgentSubScreen::AgentDetail
        | AgentSubScreen::EditSkills
        | AgentSubScreen::EditMcpServers => unreachable!(),
        AgentSubScreen::CreateMethod => "Create Agent",
        AgentSubScreen::TemplatePicker => "Templates",
        AgentSubScreen::CustomName => "Custom \u{2014} Name",
        AgentSubScreen::CustomDesc => "Custom \u{2014} Description",
        AgentSubScreen::CustomPrompt => "Custom \u{2014} System Prompt",
        AgentSubScreen::CustomTools => "Custom \u{2014} Tools",
        AgentSubScreen::CustomSkills => "Custom \u{2014} Skills",
        AgentSubScreen::CustomMcpServers => "Custom \u{2014} MCP Servers",
        AgentSubScreen::Spawning => "Spawning...",
    };

    // Center a card
    let card_h = 18u16.min(area.height);
    let card_w = 64u16.min(area.width.saturating_sub(2));
    let [card_area] = Layout::horizontal([Constraint::Length(card_w)])
        .flex(Flex::Center)
        .areas(area);
    let [card_area] = Layout::vertical([Constraint::Length(card_h)])
        .flex(Flex::Center)
        .areas(card_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            format!(" {sub_title} "),
            theme::title_style(),
        )]))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(card_area);
    f.render_widget(block, card_area);

    match state.sub {
        AgentSubScreen::CreateMethod => draw_create_method(f, inner, state),
        AgentSubScreen::TemplatePicker => draw_template_picker(f, inner, state),
        AgentSubScreen::CustomName => {
            draw_text_input(f, inner, "Agent name:", &state.custom_name, "my-agent")
        }
        AgentSubScreen::CustomDesc => draw_text_input(
            f,
            inner,
            "Description:",
            &state.custom_desc,
            "A custom agent",
        ),
        AgentSubScreen::CustomPrompt => draw_text_input(
            f,
            inner,
            "System prompt:",
            &state.custom_prompt,
            "You are a helpful agent.",
        ),
        AgentSubScreen::CustomTools => draw_tool_select(f, inner, state),
        AgentSubScreen::CustomSkills => draw_skill_select(f, inner, state),
        AgentSubScreen::CustomMcpServers => draw_mcp_select(f, inner, state),
        AgentSubScreen::Spawning => {
            let msg = Paragraph::new(Line::from(vec![Span::styled(
                "  Spawning agent...",
                theme::dim_style(),
            )]));
            f.render_widget(msg, inner);
        }
        _ => {}
    }
}

/// Full-area agent list with table layout and search bar.
fn draw_agent_list_full(f: &mut Frame, area: Rect, state: &mut AgentSelectState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Agents ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let has_search = state.search_active || !state.search_query.is_empty();
    let search_height = if has_search { 1 } else { 0 };

    let chunks = Layout::vertical([
        Constraint::Length(search_height), // search bar
        Constraint::Length(2),             // table header
        Constraint::Min(3),                // list
        Constraint::Length(1),             // hints
    ])
    .split(inner);

    // ── Search bar ──────────────────────────────────────────────────────────
    if has_search {
        let cursor = if state.search_active { "\u{2588}" } else { "" };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  / ", Style::default().fg(theme::YELLOW)),
                Span::styled(&state.search_query, theme::input_style()),
                Span::styled(
                    cursor,
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])),
            chunks[0],
        );
    }

    // ── Table header ────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {:<5} {:<18} {:<24} {}", "State", "Name", "Model", "ID"),
            theme::table_header(),
        )])),
        chunks[1],
    );

    // ── Agent list ──────────────────────────────────────────────────────────
    let daemon_count = state.daemon_agents.len();
    let use_filter = !state.search_query.is_empty();

    let agent_indices: Vec<usize> = if use_filter {
        state.filtered_indices.clone()
    } else {
        (0..state.total_agents()).collect()
    };

    let mut items: Vec<ListItem> = agent_indices
        .iter()
        .map(|&combined| {
            if combined < daemon_count {
                let a = &state.daemon_agents[combined];
                let (badge, badge_style) = theme::state_badge(&a.state);
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<5}", badge), badge_style),
                    Span::styled(
                        format!(" {:<18}", truncate(&a.name, 17)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(
                            " {:<24}",
                            truncate(&format!("{}/{}", a.provider, a.model), 23)
                        ),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(format!(" {}", truncate(&a.id, 12)), theme::dim_style()),
                ]))
            } else {
                let local = combined - daemon_count;
                let a = &state.inprocess_agents[local];
                let (badge, badge_style) = theme::state_badge(&a.state);
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<5}", badge), badge_style),
                    Span::styled(
                        format!(" {:<18}", truncate(&a.name, 17)),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(
                        format!(
                            " {:<24}",
                            truncate(&format!("{}/{}", a.provider, a.model), 23)
                        ),
                        Style::default().fg(theme::YELLOW),
                    ),
                    Span::styled(
                        format!(" {}", truncate(&format!("{}", a.id), 12)),
                        theme::dim_style(),
                    ),
                ]))
            }
        })
        .collect();

    items.push(ListItem::new(Line::from(vec![Span::styled(
        "  + Create new agent",
        Style::default()
            .fg(theme::GREEN)
            .add_modifier(Modifier::BOLD),
    )])));

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");

    f.render_stateful_widget(list, chunks[2], &mut state.list);

    // ── Status message ──────────────────────────────────────────────────────
    if !state.status_msg.is_empty() {
        let msg_area = Rect {
            x: chunks[2].x,
            y: chunks[2].y + chunks[2].height.saturating_sub(1),
            width: chunks[2].width,
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

    // ── Hints ───────────────────────────────────────────────────────────────
    let hints = if state.search_active {
        "  [Type] Filter  [Enter] Accept  [Esc] Cancel search"
    } else {
        "  [\u{2191}\u{2193}] Navigate  [Enter] Detail  [/] Search  [Esc] Back"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(hints, theme::hint_style())])),
        chunks[3],
    );
}

/// Draw agent detail view.
fn draw_detail(f: &mut Frame, area: Rect, state: &AgentSelectState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Agent Detail ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Min(10),   // detail
        Constraint::Length(1), // hints
    ])
    .split(inner);

    match &state.detail {
        Some(detail) => {
            let (badge, badge_style) = theme::state_badge(&detail.state);
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  ID:       "),
                    Span::styled(&detail.id, theme::dim_style()),
                ]),
                Line::from(vec![
                    Span::raw("  Name:     "),
                    Span::styled(
                        &detail.name,
                        Style::default()
                            .fg(theme::CYAN)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("  State:    "),
                    Span::styled(badge, badge_style),
                    Span::styled(format!(" ({})", detail.state), theme::dim_style()),
                ]),
                Line::from(vec![
                    Span::raw("  Provider: "),
                    Span::styled(&detail.provider, Style::default().fg(theme::YELLOW)),
                ]),
                Line::from(vec![
                    Span::raw("  Model:    "),
                    Span::styled(&detail.model, Style::default().fg(theme::YELLOW)),
                ]),
            ];

            if !detail.created.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Created:  "),
                    Span::styled(&detail.created, theme::dim_style()),
                ]));
            }
            if !detail.last_active.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Active:   "),
                    Span::styled(&detail.last_active, theme::dim_style()),
                ]));
            }
            if !detail.tags.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Tags:     "),
                    Span::styled(detail.tags.join(", "), Style::default().fg(theme::CYAN)),
                ]));
            }
            if !detail.capabilities.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Caps:     "),
                    Span::styled(
                        detail.capabilities.join(", "),
                        Style::default().fg(theme::YELLOW),
                    ),
                ]));
            }
            if let Some(ref parent) = detail.parent {
                lines.push(Line::from(vec![
                    Span::raw("  Parent:   "),
                    Span::styled(parent, theme::dim_style()),
                ]));
            }
            if !detail.children.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Children: "),
                    Span::styled(detail.children.join(", "), theme::dim_style()),
                ]));
            }

            // Skills section
            lines.push(Line::from(""));
            if detail.skills.is_empty() || detail.skills_mode == "all" {
                lines.push(Line::from(vec![
                    Span::raw("  Skills:   "),
                    Span::styled("[All skills]", Style::default().fg(theme::GREEN)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  Skills:   "),
                    Span::styled(detail.skills.join(", "), Style::default().fg(theme::CYAN)),
                ]));
            }

            // MCP section
            if detail.mcp_servers.is_empty() || detail.mcp_servers_mode == "all" {
                lines.push(Line::from(vec![
                    Span::raw("  MCP:      "),
                    Span::styled("[All servers]", Style::default().fg(theme::GREEN)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  MCP:      "),
                    Span::styled(
                        detail.mcp_servers.join(", "),
                        Style::default().fg(theme::CYAN),
                    ),
                ]));
            }

            f.render_widget(Paragraph::new(lines), chunks[0]);
        }
        None => {
            f.render_widget(
                Paragraph::new(Span::styled("  No agent selected.", theme::dim_style())),
                chunks[0],
            );
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [s] Edit skills  [m] Edit MCP  [c] Chat  [k] Kill  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[1],
    );
}

fn draw_create_method(f: &mut Frame, area: Rect, state: &mut AgentSelectState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new("  How would you like to create your agent?");
    f.render_widget(prompt, chunks[0]);

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  Choose from templates"),
            Span::styled("  (pre-built agents)", theme::dim_style()),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  Build custom agent"),
            Span::styled("  (pick name, tools, prompt)", theme::dim_style()),
        ])),
    ];

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");

    f.render_stateful_widget(list, chunks[1], &mut state.create_method_list);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_template_picker(f: &mut Frame, area: Rect, state: &mut AgentSelectState) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

    let items: Vec<ListItem> = state
        .templates
        .iter()
        .map(|t| {
            let hint = templates::template_display_hint(t);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<20}", t.name),
                    Style::default().fg(theme::CYAN),
                ),
                Span::styled(hint, theme::dim_style()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected_style())
        .highlight_symbol("> ");

    f.render_stateful_widget(list, chunks[0], &mut state.template_list);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[1]);
}

fn draw_text_input(f: &mut Frame, area: Rect, label: &str, value: &str, placeholder: &str) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new(format!("  {label}"));
    f.render_widget(prompt, chunks[0]);

    let display = if value.is_empty() { placeholder } else { value };
    let style = if value.is_empty() {
        theme::dim_style()
    } else {
        theme::input_style()
    };

    let input = Paragraph::new(Line::from(vec![
        Span::raw("  > "),
        Span::styled(display, style),
        Span::styled(
            "\u{2588}",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]));
    f.render_widget(input, chunks[1]);

    if value.is_empty() {
        let hint = Paragraph::new(Line::from(vec![Span::styled(
            format!("    placeholder: {placeholder}"),
            theme::dim_style(),
        )]));
        f.render_widget(hint, chunks[2]);
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [Enter] Next  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[4]);
}

fn draw_tool_select(f: &mut Frame, area: Rect, state: &AgentSelectState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new("  Select tools (Space to toggle):");
    f.render_widget(prompt, chunks[0]);

    let items: Vec<ListItem> = TOOL_OPTIONS
        .iter()
        .zip(state.tool_checks.iter())
        .enumerate()
        .map(|(i, ((name, desc), &checked))| {
            let check = if checked { "\u{25c9}" } else { "\u{25cb}" };
            let highlight = if i == state.tool_cursor {
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {check} {name:<16}"), highlight),
                Span::styled(*desc, theme::dim_style()),
            ]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[1]);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "    [\u{2191}\u{2193}] Navigate  [Space] Toggle  [Enter] Create  [Esc] Back",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn draw_skill_select(f: &mut Frame, area: Rect, state: &AgentSelectState) {
    draw_checkbox_list(
        f,
        area,
        "Select skills (none checked = all skills):",
        &state.available_skills,
        state.skill_cursor,
        "    [\u{2191}\u{2193}] Navigate  [Space] Toggle  [Enter] Next  [Esc] Back",
    );
}

fn draw_mcp_select(f: &mut Frame, area: Rect, state: &AgentSelectState) {
    draw_checkbox_list(
        f,
        area,
        "Select MCP servers (none checked = all servers):",
        &state.available_mcp,
        state.mcp_cursor,
        "    [\u{2191}\u{2193}] Navigate  [Space] Toggle  [Enter] Create  [Esc] Back",
    );
}

fn draw_edit_allowlist(f: &mut Frame, area: Rect, state: &AgentSelectState) {
    let (title, items, cursor) = match state.sub {
        AgentSubScreen::EditSkills => {
            (" Edit Skills ", &state.available_skills, state.skill_cursor)
        }
        AgentSubScreen::EditMcpServers => {
            (" Edit MCP Servers ", &state.available_mcp, state.mcp_cursor)
        }
        _ => return,
    };

    let block = Block::default()
        .title(Line::from(vec![Span::styled(title, theme::title_style())]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    draw_checkbox_list(
        f,
        inner,
        "Space to toggle, Enter to save (none checked = all):",
        items,
        cursor,
        "    [\u{2191}\u{2193}] Navigate  [Space] Toggle  [Enter] Save  [Esc] Cancel",
    );
}

fn draw_checkbox_list(
    f: &mut Frame,
    area: Rect,
    prompt_text: &str,
    items: &[(String, bool)],
    cursor: usize,
    hints_text: &str,
) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let prompt = Paragraph::new(format!("  {prompt_text}"));
    f.render_widget(prompt, chunks[0]);

    if items.is_empty() {
        let msg = Paragraph::new(Span::styled("  (none available)", theme::dim_style()));
        f.render_widget(msg, chunks[1]);
    } else {
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(i, (name, checked))| {
                let check = if *checked { "\u{25c9}" } else { "\u{25cb}" };
                let highlight = if i == cursor {
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![Span::styled(
                    format!("  {check} {name}"),
                    highlight,
                )]))
            })
            .collect();

        let list = List::new(list_items);
        f.render_widget(list, chunks[1]);
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        hints_text,
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}\u{2026}", openfang_types::truncate_str(s, max.saturating_sub(1)))
    }
}
