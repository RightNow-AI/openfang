//! Channels screen: list all 40 adapters, setup wizards, test & toggle.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ChannelInfo {
    pub name: String,
    pub display_name: String,
    pub category: String,
    pub status: ChannelStatus,
    pub env_vars: Vec<(String, bool)>, // (var_name, is_set)
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelStatus {
    Ready,
    MissingEnv,
    NotConfigured,
}

// ── Channel definitions — all 40 adapters ───────────────────────────────────

struct ChannelDef {
    name: &'static str,
    display_name: &'static str,
    category: &'static str,
    env_vars: &'static [&'static str],
    description: &'static str,
}

const CHANNEL_DEFS: &[ChannelDef] = &[
    // ── Messaging (12)
    ChannelDef {
        name: "telegram",
        display_name: "Telegram",
        category: "Messaging",
        env_vars: &["TELEGRAM_BOT_TOKEN"],
        description: "Telegram Bot API adapter",
    },
    ChannelDef {
        name: "discord",
        display_name: "Discord",
        category: "Messaging",
        env_vars: &["DISCORD_BOT_TOKEN"],
        description: "Discord bot adapter",
    },
    ChannelDef {
        name: "slack",
        display_name: "Slack",
        category: "Messaging",
        env_vars: &["SLACK_APP_TOKEN", "SLACK_BOT_TOKEN"],
        description: "Slack Socket Mode adapter",
    },
    ChannelDef {
        name: "whatsapp",
        display_name: "WhatsApp",
        category: "Messaging",
        env_vars: &["WHATSAPP_ACCESS_TOKEN", "WHATSAPP_VERIFY_TOKEN"],
        description: "WhatsApp Cloud API adapter",
    },
    ChannelDef {
        name: "signal",
        display_name: "Signal",
        category: "Messaging",
        env_vars: &[],
        description: "Signal via signal-cli REST API",
    },
    ChannelDef {
        name: "matrix",
        display_name: "Matrix",
        category: "Messaging",
        env_vars: &["MATRIX_ACCESS_TOKEN"],
        description: "Matrix/Element adapter",
    },
    ChannelDef {
        name: "email",
        display_name: "Email",
        category: "Messaging",
        env_vars: &["EMAIL_PASSWORD"],
        description: "IMAP/SMTP email adapter",
    },
    ChannelDef {
        name: "line",
        display_name: "LINE",
        category: "Messaging",
        env_vars: &["LINE_CHANNEL_SECRET", "LINE_CHANNEL_ACCESS_TOKEN"],
        description: "LINE Messaging API adapter",
    },
    ChannelDef {
        name: "viber",
        display_name: "Viber",
        category: "Messaging",
        env_vars: &["VIBER_AUTH_TOKEN"],
        description: "Viber Bot API adapter",
    },
    ChannelDef {
        name: "messenger",
        display_name: "Messenger",
        category: "Messaging",
        env_vars: &["MESSENGER_PAGE_TOKEN", "MESSENGER_VERIFY_TOKEN"],
        description: "Facebook Messenger adapter",
    },
    ChannelDef {
        name: "threema",
        display_name: "Threema",
        category: "Messaging",
        env_vars: &["THREEMA_SECRET"],
        description: "Threema Gateway adapter",
    },
    ChannelDef {
        name: "keybase",
        display_name: "Keybase",
        category: "Messaging",
        env_vars: &["KEYBASE_PAPERKEY"],
        description: "Keybase chat adapter",
    },
    // ── Social (5)
    ChannelDef {
        name: "reddit",
        display_name: "Reddit",
        category: "Social",
        env_vars: &["REDDIT_CLIENT_SECRET", "REDDIT_PASSWORD"],
        description: "Reddit API bot adapter",
    },
    ChannelDef {
        name: "mastodon",
        display_name: "Mastodon",
        category: "Social",
        env_vars: &["MASTODON_ACCESS_TOKEN"],
        description: "Mastodon Streaming API adapter",
    },
    ChannelDef {
        name: "bluesky",
        display_name: "Bluesky",
        category: "Social",
        env_vars: &["BLUESKY_APP_PASSWORD"],
        description: "Bluesky/AT Protocol adapter",
    },
    ChannelDef {
        name: "linkedin",
        display_name: "LinkedIn",
        category: "Social",
        env_vars: &["LINKEDIN_ACCESS_TOKEN"],
        description: "LinkedIn Messaging API adapter",
    },
    ChannelDef {
        name: "nostr",
        display_name: "Nostr",
        category: "Social",
        env_vars: &["NOSTR_PRIVATE_KEY"],
        description: "Nostr relay protocol adapter",
    },
    // ── Enterprise (10)
    ChannelDef {
        name: "teams",
        display_name: "Teams",
        category: "Enterprise",
        env_vars: &["TEAMS_APP_PASSWORD"],
        description: "Microsoft Teams Bot Framework adapter",
    },
    ChannelDef {
        name: "mattermost",
        display_name: "Mattermost",
        category: "Enterprise",
        env_vars: &["MATTERMOST_TOKEN"],
        description: "Mattermost WebSocket adapter",
    },
    ChannelDef {
        name: "google_chat",
        display_name: "Google Chat",
        category: "Enterprise",
        env_vars: &["GOOGLE_CHAT_SERVICE_ACCOUNT"],
        description: "Google Chat service account adapter",
    },
    ChannelDef {
        name: "webex",
        display_name: "Webex",
        category: "Enterprise",
        env_vars: &["WEBEX_BOT_TOKEN"],
        description: "Cisco Webex bot adapter",
    },
    ChannelDef {
        name: "feishu",
        display_name: "Feishu/Lark",
        category: "Enterprise",
        env_vars: &["FEISHU_APP_SECRET"],
        description: "Feishu/Lark Open Platform adapter",
    },
    ChannelDef {
        name: "dingtalk",
        display_name: "DingTalk",
        category: "Enterprise",
        env_vars: &["DINGTALK_ACCESS_TOKEN", "DINGTALK_SECRET"],
        description: "DingTalk Robot API adapter (webhook mode)",
    },
    ChannelDef {
        name: "dingtalk_stream",
        display_name: "DingTalk Stream",
        category: "Enterprise",
        env_vars: &[
            "DINGTALK_APP_KEY",
            "DINGTALK_APP_SECRET",
            "DINGTALK_ROBOT_CODE",
        ],
        description: "DingTalk Stream Mode (WebSocket long-connection)",
    },
    ChannelDef {
        name: "pumble",
        display_name: "Pumble",
        category: "Enterprise",
        env_vars: &["PUMBLE_BOT_TOKEN"],
        description: "Pumble bot adapter",
    },
    ChannelDef {
        name: "flock",
        display_name: "Flock",
        category: "Enterprise",
        env_vars: &["FLOCK_BOT_TOKEN"],
        description: "Flock bot adapter",
    },
    ChannelDef {
        name: "twist",
        display_name: "Twist",
        category: "Enterprise",
        env_vars: &["TWIST_TOKEN"],
        description: "Twist API v3 adapter",
    },
    ChannelDef {
        name: "zulip",
        display_name: "Zulip",
        category: "Enterprise",
        env_vars: &["ZULIP_API_KEY"],
        description: "Zulip event queue adapter",
    },
    // ── Developer (9)
    ChannelDef {
        name: "irc",
        display_name: "IRC",
        category: "Developer",
        env_vars: &[],
        description: "IRC raw TCP adapter",
    },
    ChannelDef {
        name: "xmpp",
        display_name: "XMPP",
        category: "Developer",
        env_vars: &["XMPP_PASSWORD"],
        description: "XMPP/Jabber adapter",
    },
    ChannelDef {
        name: "gitter",
        display_name: "Gitter",
        category: "Developer",
        env_vars: &["GITTER_TOKEN"],
        description: "Gitter Streaming API adapter",
    },
    ChannelDef {
        name: "discourse",
        display_name: "Discourse",
        category: "Developer",
        env_vars: &["DISCOURSE_API_KEY"],
        description: "Discourse forum API adapter",
    },
    ChannelDef {
        name: "revolt",
        display_name: "Revolt",
        category: "Developer",
        env_vars: &["REVOLT_BOT_TOKEN"],
        description: "Revolt bot adapter",
    },
    ChannelDef {
        name: "guilded",
        display_name: "Guilded",
        category: "Developer",
        env_vars: &["GUILDED_BOT_TOKEN"],
        description: "Guilded bot adapter",
    },
    ChannelDef {
        name: "nextcloud",
        display_name: "Nextcloud",
        category: "Developer",
        env_vars: &["NEXTCLOUD_TOKEN"],
        description: "Nextcloud Talk adapter",
    },
    ChannelDef {
        name: "rocketchat",
        display_name: "Rocket.Chat",
        category: "Developer",
        env_vars: &["ROCKETCHAT_TOKEN"],
        description: "Rocket.Chat REST adapter",
    },
    ChannelDef {
        name: "twitch",
        display_name: "Twitch",
        category: "Developer",
        env_vars: &["TWITCH_OAUTH_TOKEN"],
        description: "Twitch IRC gateway adapter",
    },
    // ── Notifications (4)
    ChannelDef {
        name: "ntfy",
        display_name: "ntfy",
        category: "Notifications",
        env_vars: &["NTFY_TOKEN"],
        description: "ntfy.sh pub/sub adapter",
    },
    ChannelDef {
        name: "gotify",
        display_name: "Gotify",
        category: "Notifications",
        env_vars: &["GOTIFY_APP_TOKEN", "GOTIFY_CLIENT_TOKEN"],
        description: "Gotify WebSocket adapter",
    },
    ChannelDef {
        name: "webhook",
        display_name: "Webhook",
        category: "Notifications",
        env_vars: &["WEBHOOK_SECRET"],
        description: "Generic webhook adapter",
    },
    ChannelDef {
        name: "mumble",
        display_name: "Mumble",
        category: "Notifications",
        env_vars: &["MUMBLE_PASSWORD"],
        description: "Mumble text chat adapter",
    },
];

const CATEGORIES: &[&str] = &[
    "All",
    "Messaging",
    "Social",
    "Enterprise",
    "Developer",
    "Notifications",
];

// ── State ───────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq)]
pub enum ChannelSubScreen {
    List,
    Setup,
    Testing,
}

pub struct ChannelState {
    pub sub: ChannelSubScreen,
    pub channels: Vec<ChannelInfo>,
    pub list_state: ListState,
    pub loading: bool,
    pub tick: usize,
    // Category filter
    pub category_idx: usize,
    // Setup wizard
    pub setup_channel_idx: Option<usize>,
    pub setup_field_idx: usize,
    pub setup_input: String,
    pub setup_values: Vec<(String, String)>, // collected (env_var, value) pairs
    // Test
    pub test_result: Option<(bool, String)>,
    pub status_msg: String,
}

pub enum ChannelAction {
    Continue,
    Refresh,
    TestChannel(String),
    ToggleChannel(String, bool),
    SaveChannel(String, Vec<(String, String)>),
}

impl ChannelState {
    pub fn new() -> Self {
        Self {
            sub: ChannelSubScreen::List,
            channels: Vec::new(),
            list_state: ListState::default(),
            loading: false,
            tick: 0,
            category_idx: 0,
            setup_channel_idx: None,
            setup_field_idx: 0,
            setup_input: String::new(),
            setup_values: Vec::new(),
            test_result: None,
            status_msg: String::new(),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    fn current_category(&self) -> &str {
        CATEGORIES[self.category_idx]
    }

    fn filtered_channels(&self) -> Vec<&ChannelInfo> {
        let cat = self.current_category();
        self.channels
            .iter()
            .filter(|ch| cat == "All" || ch.category == cat)
            .collect()
    }

    fn ready_count(&self) -> usize {
        self.channels
            .iter()
            .filter(|ch| ch.status == ChannelStatus::Ready)
            .count()
    }

    /// Build the default channel list from env var detection.
    pub fn build_default_channels(&mut self) {
        self.channels.clear();
        for def in CHANNEL_DEFS {
            let env_vars: Vec<(String, bool)> = def
                .env_vars
                .iter()
                .map(|v| (v.to_string(), std::env::var(v).is_ok()))
                .collect();
            let all_set = env_vars.is_empty() || env_vars.iter().all(|(_, set)| *set);
            let any_set = env_vars.iter().any(|(_, set)| *set);
            let status = if all_set && !env_vars.is_empty() {
                ChannelStatus::Ready
            } else if any_set {
                ChannelStatus::MissingEnv
            } else {
                ChannelStatus::NotConfigured
            };
            self.channels.push(ChannelInfo {
                name: def.name.to_string(),
                display_name: def.display_name.to_string(),
                category: def.category.to_string(),
                status,
                env_vars,
                enabled: false,
            });
        }
        self.list_state.select(Some(0));
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ChannelAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ChannelAction::Continue;
        }
        match self.sub {
            ChannelSubScreen::List => self.handle_list(key),
            ChannelSubScreen::Setup => self.handle_setup(key),
            ChannelSubScreen::Testing => self.handle_testing(key),
        }
    }

    fn handle_list(&mut self, key: KeyEvent) -> ChannelAction {
        let filtered = self.filtered_channels();
        let total = filtered.len();
        if total == 0 {
            match key.code {
                KeyCode::Char('r') => return ChannelAction::Refresh,
                KeyCode::Tab => {
                    self.category_idx = (self.category_idx + 1) % CATEGORIES.len();
                    self.list_state.select(Some(0));
                }
                KeyCode::BackTab => {
                    self.category_idx = if self.category_idx == 0 {
                        CATEGORIES.len() - 1
                    } else {
                        self.category_idx - 1
                    };
                    self.list_state.select(Some(0));
                }
                _ => {}
            }
            return ChannelAction::Continue;
        }
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
            KeyCode::Tab => {
                self.category_idx = (self.category_idx + 1) % CATEGORIES.len();
                self.list_state.select(Some(0));
            }
            KeyCode::BackTab => {
                self.category_idx = if self.category_idx == 0 {
                    CATEGORIES.len() - 1
                } else {
                    self.category_idx - 1
                };
                self.list_state.select(Some(0));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.list_state.selected() {
                    let filtered = self.filtered_channels();
                    if let Some(ch) = filtered.get(sel) {
                        // Find the global index for this channel
                        let ch_name = ch.name.clone();
                        if let Some(idx) = self.channels.iter().position(|c| c.name == ch_name) {
                            self.setup_channel_idx = Some(idx);
                            self.setup_field_idx = 0;
                            self.setup_input.clear();
                            self.setup_values.clear();
                            self.sub = ChannelSubScreen::Setup;
                        }
                    }
                }
            }
            KeyCode::Char('t') => {
                if let Some(sel) = self.list_state.selected() {
                    let filtered = self.filtered_channels();
                    if let Some(ch) = filtered.get(sel) {
                        let name = ch.name.clone();
                        self.test_result = None;
                        self.sub = ChannelSubScreen::Testing;
                        return ChannelAction::TestChannel(name);
                    }
                }
            }
            KeyCode::Char('e') => {
                if let Some(sel) = self.list_state.selected() {
                    let filtered = self.filtered_channels();
                    if let Some(ch) = filtered.get(sel) {
                        let name = ch.name.clone();
                        if let Some(c) = self.channels.iter_mut().find(|c| c.name == name) {
                            c.enabled = true;
                        }
                        return ChannelAction::ToggleChannel(name, true);
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(sel) = self.list_state.selected() {
                    let filtered = self.filtered_channels();
                    if let Some(ch) = filtered.get(sel) {
                        let name = ch.name.clone();
                        if let Some(c) = self.channels.iter_mut().find(|c| c.name == name) {
                            c.enabled = false;
                        }
                        return ChannelAction::ToggleChannel(name, false);
                    }
                }
            }
            KeyCode::Char('r') => return ChannelAction::Refresh,
            _ => {}
        }
        ChannelAction::Continue
    }

    fn handle_setup(&mut self, key: KeyEvent) -> ChannelAction {
        match key.code {
            KeyCode::Esc => {
                self.sub = ChannelSubScreen::List;
            }
            KeyCode::Char(c) => {
                self.setup_input.push(c);
            }
            KeyCode::Backspace => {
                self.setup_input.pop();
            }
            KeyCode::Enter => {
                if let Some(idx) = self.setup_channel_idx {
                    if idx < self.channels.len() {
                        let env_vars = &CHANNEL_DEFS
                            .iter()
                            .find(|d| d.name == self.channels[idx].name)
                            .map(|d| d.env_vars)
                            .unwrap_or(&[]);

                        // Save current field value
                        if self.setup_field_idx < env_vars.len() && !self.setup_input.is_empty() {
                            self.setup_values.push((
                                env_vars[self.setup_field_idx].to_string(),
                                self.setup_input.clone(),
                            ));
                        }

                        if self.setup_field_idx + 1 < env_vars.len() {
                            self.setup_field_idx += 1;
                            self.setup_input.clear();
                        } else {
                            // All fields collected — emit save action
                            let name = self.channels[idx].name.clone();
                            let values = self.setup_values.clone();
                            self.sub = ChannelSubScreen::List;
                            if !values.is_empty() {
                                return ChannelAction::SaveChannel(name, values);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        ChannelAction::Continue
    }

    fn handle_testing(&mut self, key: KeyEvent) -> ChannelAction {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.sub = ChannelSubScreen::List;
            }
            _ => {}
        }
        ChannelAction::Continue
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, state: &mut ChannelState) {
    let ready = state.ready_count();
    let total = state.channels.len();
    let title = format!(" Channels ({ready}/{total} ready) ");

    let block = Block::default()
        .title(Line::from(vec![Span::styled(title, theme::title_style())]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match state.sub {
        ChannelSubScreen::List => draw_list(f, inner, state),
        ChannelSubScreen::Setup => draw_setup(f, inner, state),
        ChannelSubScreen::Testing => draw_testing(f, inner, state),
    }
}

fn draw_list(f: &mut Frame, area: Rect, state: &mut ChannelState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // category tabs
        Constraint::Length(2), // header
        Constraint::Min(3),    // list
        Constraint::Length(1), // hints
    ])
    .split(area);

    // Category tabs
    let cat_spans: Vec<Span> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            if i == state.category_idx {
                Span::styled(
                    format!(" [{cat}] "),
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(format!("  {cat}  "), theme::dim_style())
            }
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(cat_spans)), chunks[0]);

    // Header
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(
                "  {:<18} {:<14} {:<16} {}",
                "Channel", "Category", "Status", "Env Vars"
            ),
            theme::table_header(),
        )])),
        chunks[1],
    );

    if state.loading {
        let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                Span::styled("Loading channels\u{2026}", theme::dim_style()),
            ])),
            chunks[2],
        );
    } else {
        let filtered = state.filtered_channels();
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|ch| {
                let (badge, badge_style) = match ch.status {
                    ChannelStatus::Ready => ("[Ready]", theme::channel_ready()),
                    ChannelStatus::MissingEnv => ("[Missing env]", theme::channel_missing()),
                    ChannelStatus::NotConfigured => ("[Not configured]", theme::channel_off()),
                };
                let env_summary: String = ch
                    .env_vars
                    .iter()
                    .map(|(v, set)| {
                        if *set {
                            format!("\u{2714}{v}")
                        } else {
                            format!("\u{2718}{v}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let cat_display = format!("{:<14}", ch.category);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {:<18}", ch.display_name),
                        Style::default().fg(theme::CYAN),
                    ),
                    Span::styled(cat_display, theme::dim_style()),
                    Span::styled(format!(" {:<16}", badge), badge_style),
                    Span::styled(format!(" {env_summary}"), theme::dim_style()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme::selected_style())
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[2], &mut state.list_state);
    }

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        "  [\u{2191}\u{2193}] Navigate  [Tab] Category  [Enter] Setup  [t] Test  [e/d] Enable/Disable  [r] Refresh",
        theme::hint_style(),
    )]));
    f.render_widget(hints, chunks[3]);
}

fn draw_setup(f: &mut Frame, area: Rect, state: &ChannelState) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // title + description
        Constraint::Length(1), // separator
        Constraint::Length(2), // current field
        Constraint::Length(1), // input
        Constraint::Min(2),    // TOML preview
        Constraint::Length(1), // hints
    ])
    .split(area);

    let (ch_name, ch_display, ch_desc, env_vars) = if let Some(idx) = state.setup_channel_idx {
        if let Some(def) = CHANNEL_DEFS
            .iter()
            .find(|d| idx < state.channels.len() && d.name == state.channels[idx].name)
        {
            (def.name, def.display_name, def.description, def.env_vars)
        } else {
            ("?", "?", "", &[] as &[&str])
        }
    } else {
        ("?", "?", "", &[] as &[&str])
    };

    // Title
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                format!("  Setup: {ch_display}"),
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                format!("  {ch_desc}"),
                theme::dim_style(),
            )]),
        ]),
        chunks[0],
    );

    // Separator
    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, theme::dim_style())),
        chunks[1],
    );

    // Current field
    if env_vars.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  This channel has no secret env vars — configure via config.toml",
                theme::dim_style(),
            )])),
            chunks[2],
        );
    } else if state.setup_field_idx < env_vars.len() {
        let var = env_vars[state.setup_field_idx];
        let field_num = state.setup_field_idx + 1;
        let total = env_vars.len();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(format!("  [{field_num}/{total}] Set ")),
                Span::styled(var, Style::default().fg(theme::YELLOW)),
                Span::raw(":"),
            ])),
            chunks[2],
        );
    }

    // Input
    let display = if state.setup_input.is_empty() {
        "paste value here..."
    } else {
        &state.setup_input
    };
    let style = if state.setup_input.is_empty() {
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

    // TOML preview
    let mut toml_lines = vec![Line::from(Span::styled(
        "  Add to config.toml:",
        theme::dim_style(),
    ))];
    toml_lines.push(Line::from(Span::styled(
        format!("  [channels.{ch_name}]"),
        Style::default().fg(theme::YELLOW),
    )));
    for var in env_vars {
        toml_lines.push(Line::from(Span::styled(
            format!("  # {var} = \"...\""),
            Style::default().fg(theme::YELLOW),
        )));
    }
    f.render_widget(Paragraph::new(toml_lines), chunks[4]);

    // Hints
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Enter] Next field / Save  [Esc] Back",
            theme::hint_style(),
        )])),
        chunks[5],
    );
}

fn draw_testing(f: &mut Frame, area: Rect, state: &ChannelState) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .split(area);

    let ch_name = state
        .setup_channel_idx
        .and_then(|i| state.channels.get(i))
        .map(|c| c.display_name.as_str())
        .or_else(|| {
            state.list_state.selected().and_then(|i| {
                let filtered = state.filtered_channels();
                filtered.get(i).map(|c| c.display_name.as_str())
            })
        })
        .unwrap_or("?");

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  Testing {ch_name}\u{2026}"),
            Style::default().fg(theme::CYAN),
        )])),
        chunks[0],
    );

    match &state.test_result {
        None => {
            let spinner = theme::SPINNER_FRAMES[state.tick % theme::SPINNER_FRAMES.len()];
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
                    Span::styled("Checking credentials\u{2026}", theme::dim_style()),
                ])),
                chunks[1],
            );
        }
        Some((true, msg)) => {
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![
                        Span::styled("  \u{2714} ", Style::default().fg(theme::GREEN)),
                        Span::raw("Test passed"),
                    ]),
                    Line::from(vec![Span::styled(format!("  {msg}"), theme::dim_style())]),
                ]),
                chunks[1],
            );
        }
        Some((false, msg)) => {
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![
                        Span::styled("  \u{2718} ", Style::default().fg(theme::RED)),
                        Span::raw("Test failed"),
                    ]),
                    Line::from(vec![Span::styled(
                        format!("  {msg}"),
                        Style::default().fg(theme::RED),
                    )]),
                ]),
                chunks[1],
            );
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  [Enter/Esc] Back",
            theme::hint_style(),
        )])),
        chunks[2],
    );
}
