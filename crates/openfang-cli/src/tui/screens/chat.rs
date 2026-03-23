//! Chat screen: scrollable message history, streaming output, tool spinners, input.

use crate::tui::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

/// Model entry for the picker.
#[derive(Clone)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub tier: String,
}

/// Tool call metadata for rich rendering.
#[derive(Clone)]
pub struct ToolInfo {
    pub name: String,
    pub input: String,
    pub result: String,
    pub is_error: bool,
}

/// A single message in the chat history.
#[derive(Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub text: String,
    pub tool: Option<ToolInfo>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Agent,
    System,
    Tool,
}

pub struct ChatState {
    /// Agent display name.
    pub agent_name: String,
    /// Provider/model for the title bar.
    pub model_label: String,
    /// Connection mode label.
    pub mode_label: String,
    /// Full chat history.
    pub messages: Vec<ChatMessage>,
    /// Current streaming text being accumulated.
    pub streaming_text: String,
    /// Whether we are currently streaming.
    pub is_streaming: bool,
    /// Waiting for first token (shows "thinking..." spinner).
    pub thinking: bool,
    /// Current tool being executed (spinner).
    pub active_tool: Option<String>,
    /// Spinner frame index.
    pub spinner_frame: usize,
    /// Input line buffer.
    pub input: String,
    /// Scroll offset (lines from the bottom).
    pub scroll_offset: u16,
    /// Token usage from last response.
    pub last_tokens: Option<(u64, u64)>,
    /// Cost in USD from last response.
    pub last_cost_usd: Option<f64>,
    /// Characters received during current stream (~4 chars ≈ 1 token).
    pub streaming_chars: usize,
    /// Status message (errors, etc.)
    pub status_msg: Option<String>,
    /// Messages staged while the agent is streaming — sent automatically when done.
    pub staged_messages: Vec<String>,
    /// Accumulates ToolInputDelta text for the current tool call.
    pub tool_input_buf: String,
    /// Model picker overlay state.
    pub show_model_picker: bool,
    /// Available models for the picker.
    pub model_picker_models: Vec<ModelEntry>,
    /// Filter text for model search.
    pub model_picker_filter: String,
    /// Selected index in the filtered model list.
    pub model_picker_idx: usize,
}

pub enum ChatAction {
    Continue,
    SendMessage(String),
    Back,
    SlashCommand(String),
    /// Open the model picker (fetch models first).
    OpenModelPicker,
    /// Switch to a specific model by id.
    SwitchModel(String),
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            agent_name: String::new(),
            model_label: String::new(),
            mode_label: String::new(),
            messages: Vec::new(),
            streaming_text: String::new(),
            is_streaming: false,
            thinking: false,
            active_tool: None,
            spinner_frame: 0,
            input: String::new(),
            scroll_offset: 0,
            last_tokens: None,
            last_cost_usd: None,
            streaming_chars: 0,
            status_msg: None,
            staged_messages: Vec::new(),
            tool_input_buf: String::new(),
            show_model_picker: false,
            model_picker_models: Vec::new(),
            model_picker_filter: String::new(),
            model_picker_idx: 0,
        }
    }

    pub fn reset(&mut self) {
        self.messages.clear();
        self.streaming_text.clear();
        self.is_streaming = false;
        self.thinking = false;
        self.active_tool = None;
        self.spinner_frame = 0;
        self.input.clear();
        self.scroll_offset = 0;
        self.last_tokens = None;
        self.last_cost_usd = None;
        self.streaming_chars = 0;
        self.status_msg = None;
        self.staged_messages.clear();
        self.tool_input_buf.clear();
        self.show_model_picker = false;
        self.model_picker_filter.clear();
        self.model_picker_idx = 0;
    }

    /// Push a completed message into history.
    pub fn push_message(&mut self, role: Role, text: String) {
        self.messages.push(ChatMessage {
            role,
            text,
            tool: None,
        });
        self.scroll_offset = 0; // Auto-scroll to bottom
    }

    /// Append streaming text delta.
    pub fn append_stream(&mut self, text: &str) {
        self.thinking = false;
        self.streaming_text.push_str(text);
        self.streaming_chars += text.len();
        self.scroll_offset = 0;
    }

    /// Take the next staged message (if any) for auto-send after stream completes.
    pub fn take_staged(&mut self) -> Option<String> {
        if self.staged_messages.is_empty() {
            None
        } else {
            Some(self.staged_messages.remove(0))
        }
    }

    /// Finalize streaming: move accumulated text to history.
    pub fn finalize_stream(&mut self) {
        if !self.streaming_text.is_empty() {
            let text = sanitize_function_tags(&std::mem::take(&mut self.streaming_text));
            self.push_message(Role::Agent, text);
        }
        self.is_streaming = false;
        self.thinking = false;
        self.active_tool = None;
        self.streaming_chars = 0;
        self.tool_input_buf.clear();
    }

    /// Set a tool as active (spinner) and clear the input accumulator.
    pub fn tool_start(&mut self, name: &str) {
        self.active_tool = Some(name.to_string());
        self.tool_input_buf.clear();
        self.spinner_frame = 0;
    }

    /// A tool_use block is complete — push a "running" tool message with input.
    pub fn tool_use_end(&mut self, name: &str, input: &str) {
        self.messages.push(ChatMessage {
            role: Role::Tool,
            text: name.to_string(),
            tool: Some(ToolInfo {
                name: name.to_string(),
                input: input.to_string(),
                result: String::new(),
                is_error: false,
            }),
        });
        self.scroll_offset = 0;
        self.active_tool = None;
    }

    /// Fill in the result for the most recent matching tool message.
    pub fn tool_result(&mut self, name: &str, result: &str, is_error: bool) {
        // Walk backwards to find the last Tool message matching this name
        for msg in self.messages.iter_mut().rev() {
            if msg.role == Role::Tool {
                if let Some(ref mut info) = msg.tool {
                    if info.name == name && info.result.is_empty() {
                        info.result = result.to_string();
                        info.is_error = is_error;
                        break;
                    }
                }
            }
        }
        self.active_tool = None;
        self.scroll_offset = 0;
    }

    /// Advance the spinner frame (called on tick).
    pub fn tick(&mut self) {
        if self.active_tool.is_some() || self.thinking {
            self.spinner_frame = (self.spinner_frame + 1) % theme::SPINNER_FRAMES.len();
        }
    }

    /// Return filtered models based on the current picker filter.
    pub fn filtered_models(&self) -> Vec<&ModelEntry> {
        if self.model_picker_filter.is_empty() {
            return self.model_picker_models.iter().collect();
        }
        let f = self.model_picker_filter.to_lowercase();
        self.model_picker_models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&f)
                    || m.display_name.to_lowercase().contains(&f)
                    || m.provider.to_lowercase().contains(&f)
            })
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ChatAction {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.show_model_picker {
                self.show_model_picker = false;
                return ChatAction::Continue;
            }
            return ChatAction::Back;
        }

        // Ctrl+M: toggle model picker
        if key.code == KeyCode::Char('m') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.is_streaming {
                return ChatAction::Continue;
            }
            if self.show_model_picker {
                self.show_model_picker = false;
                return ChatAction::Continue;
            }
            return ChatAction::OpenModelPicker;
        }

        // Model picker mode: intercept all keys
        if self.show_model_picker {
            match key.code {
                KeyCode::Esc => {
                    self.show_model_picker = false;
                }
                KeyCode::Up => {
                    self.model_picker_idx = self.model_picker_idx.saturating_sub(1);
                }
                KeyCode::Down => {
                    let max = self.filtered_models().len().saturating_sub(1);
                    if self.model_picker_idx < max {
                        self.model_picker_idx += 1;
                    }
                }
                KeyCode::Enter => {
                    let filtered = self.filtered_models();
                    if let Some(entry) = filtered.get(self.model_picker_idx) {
                        let model_id = entry.id.clone();
                        self.show_model_picker = false;
                        self.model_picker_filter.clear();
                        self.model_picker_idx = 0;
                        return ChatAction::SwitchModel(model_id);
                    }
                }
                KeyCode::Backspace => {
                    self.model_picker_filter.pop();
                    self.model_picker_idx = 0;
                }
                KeyCode::Char(c) => {
                    self.model_picker_filter.push(c);
                    self.model_picker_idx = 0;
                }
                _ => {}
            }
            return ChatAction::Continue;
        }

        // When streaming, allow typing + staging messages, scrolling, and Esc
        if self.is_streaming {
            match key.code {
                KeyCode::Esc => return ChatAction::Back,
                KeyCode::Enter => {
                    let msg = self.input.trim().to_string();
                    self.input.clear();
                    if !msg.is_empty() && !msg.starts_with('/') {
                        self.staged_messages.push(msg.clone());
                        self.push_message(Role::User, msg);
                    }
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
                KeyCode::Down => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                KeyCode::PageUp => {
                    self.scroll_offset = self.scroll_offset.saturating_add(10);
                }
                KeyCode::PageDown => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(10);
                }
                _ => {}
            }
            return ChatAction::Continue;
        }

        match key.code {
            KeyCode::Esc => ChatAction::Back,
            KeyCode::Enter => {
                let msg = self.input.trim().to_string();
                self.input.clear();
                if msg.is_empty() {
                    return ChatAction::Continue;
                }
                if msg.starts_with('/') {
                    return ChatAction::SlashCommand(msg);
                }
                self.push_message(Role::User, msg.clone());
                ChatAction::SendMessage(msg)
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                ChatAction::Continue
            }
            KeyCode::Backspace => {
                self.input.pop();
                ChatAction::Continue
            }
            KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                ChatAction::Continue
            }
            KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                ChatAction::Continue
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
                ChatAction::Continue
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                ChatAction::Continue
            }
            _ => ChatAction::Continue,
        }
    }
}

/// Render the chat screen.
pub fn draw(f: &mut Frame, area: Rect, state: &mut ChatState) {
    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            format!(" {} ", state.agent_name),
            theme::title_style(),
        )]))
        .title_alignment(Alignment::Left)
        .title_bottom(Line::from(vec![Span::styled(
            format!(" {} \u{2014} {} ", state.model_label, state.mode_label),
            theme::dim_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Layout: messages | separator | input | hints
    let chunks = Layout::vertical([
        Constraint::Min(3),    // messages area
        Constraint::Length(1), // separator
        Constraint::Length(1), // input
        Constraint::Length(1), // hints
    ])
    .split(inner);

    // ── Messages ─────────────────────────────────────────────────────────────
    draw_messages(f, chunks[0], state);

    // ── Separator ────────────────────────────────────────────────────────────
    let sep_line = "\u{2500}".repeat(chunks[1].width as usize);
    let sep = Paragraph::new(Line::from(vec![Span::styled(
        sep_line,
        Style::default().fg(theme::BORDER),
    )]));
    f.render_widget(sep, chunks[1]);

    // ── Input ────────────────────────────────────────────────────────────────
    let input_line = if state.is_streaming {
        let mut spans = vec![
            Span::styled(" > ", Style::default().fg(theme::YELLOW)),
            Span::raw(&state.input),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(theme::YELLOW)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ];
        if !state.staged_messages.is_empty() {
            spans.push(Span::styled(
                format!("  ({} staged)", state.staged_messages.len()),
                Style::default().fg(theme::PURPLE),
            ));
        }
        Line::from(spans)
    } else {
        Line::from(vec![
            Span::styled(" > ", theme::input_style()),
            Span::raw(&state.input),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    f.render_widget(Paragraph::new(input_line), chunks[2]);

    // ── Hints ────────────────────────────────────────────────────────────────
    let hints = if state.show_model_picker {
        "    [\u{2191}\u{2193}] Navigate  [Enter] Select  [Esc] Close  [type] Filter"
    } else if state.is_streaming {
        "    [Enter] Stage  [\u{2191}\u{2193}] Scroll  [Esc] Stop"
    } else {
        "    [Enter] Send  [Ctrl+M] Models  [\u{2191}\u{2193}] Scroll  [Esc] Back"
    };
    let hints = Paragraph::new(Line::from(vec![Span::styled(hints, theme::hint_style())]));
    f.render_widget(hints, chunks[3]);

    // ── Model picker overlay ────────────────────────────────────────────────
    if state.show_model_picker {
        draw_model_picker(f, inner, state);
    }
}

fn draw_model_picker(f: &mut Frame, area: Rect, state: &ChatState) {
    let filtered = state.filtered_models();

    // Center a popup — width ~50 cols, height capped at area
    if area.height < 6 || area.width < 20 {
        return; // Too small to show picker
    }
    let popup_w = area.width.clamp(30, 54);
    let popup_h = (filtered.len() as u16 + 4).clamp(5, area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    // Clear background
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![Span::styled(
            " Switch Model ",
            theme::title_style(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .padding(Padding::horizontal(1));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if inner.height < 2 || inner.width < 10 {
        return;
    }

    // Layout: search bar | model list
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);

    // Search bar
    let search_line = Line::from(vec![
        Span::styled("/ ", theme::dim_style()),
        Span::raw(&state.model_picker_filter),
        Span::styled(
            "\u{2588}",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    f.render_widget(Paragraph::new(search_line), chunks[0]);

    // Model list
    let visible_h = chunks[1].height as usize;
    let total = filtered.len();

    if total == 0 {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                " No models match",
                theme::dim_style(),
            )])),
            chunks[1],
        );
        return;
    }

    // Scroll window: keep selected item visible
    let scroll_start = if state.model_picker_idx >= visible_h {
        state.model_picker_idx - visible_h + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();
    let max_name = (chunks[1].width as usize).saturating_sub(14);
    for (i, entry) in filtered
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_h)
    {
        let selected = i == state.model_picker_idx;
        let indicator = if selected { "\u{25b6} " } else { "  " };

        let name = if entry.display_name.is_empty() {
            &entry.id
        } else {
            &entry.display_name
        };
        let name_display = if name.len() > max_name && max_name > 1 {
            let truncated = openfang_types::truncate_str(name, max_name.saturating_sub(1));
            format!("{truncated}\u{2026}")
        } else {
            name.to_string()
        };

        let tier_style = match entry.tier.to_lowercase().as_str() {
            "frontier" => Style::default().fg(theme::PURPLE),
            "smart" => Style::default().fg(theme::BLUE),
            "balanced" => Style::default().fg(theme::GREEN),
            "fast" => Style::default().fg(theme::YELLOW),
            _ => theme::dim_style(),
        };

        let bg = if selected {
            Style::default()
                .fg(theme::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };

        lines.push(Line::from(vec![
            Span::styled(indicator, Style::default().fg(theme::ACCENT)),
            Span::styled(name_display, bg),
            Span::raw(" "),
            Span::styled(entry.tier.to_lowercase(), tier_style),
        ]));
    }

    f.render_widget(Paragraph::new(lines), chunks[1]);
}

fn draw_messages(f: &mut Frame, area: Rect, state: &ChatState) {
    let width = area.width as usize;
    if width < 4 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Empty state: show welcome message when no messages yet
    if state.messages.is_empty() && state.streaming_text.is_empty() && !state.thinking {
        let blank_lines = area.height.saturating_sub(4) / 2;
        for _ in 0..blank_lines {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(vec![Span::styled(
            "  Send a message to start chatting.",
            theme::dim_style(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "  Type /help for available commands.",
            theme::dim_style(),
        )]));
        let para = Paragraph::new(lines);
        f.render_widget(para, area);
        return;
    }

    // Build lines from message history
    for msg in &state.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(""));
                let wrapped = wrap_text(&msg.text, width.saturating_sub(6));
                for (i, wline) in wrapped.into_iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{276f} ", theme::input_style()),
                            Span::styled(wline, Style::default().fg(theme::TEXT_PRIMARY)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(wline, Style::default().fg(theme::TEXT_PRIMARY)),
                        ]));
                    }
                }
            }
            Role::Agent => {
                lines.push(Line::from(""));
                let wrapped = wrap_text(&msg.text, width.saturating_sub(4));
                for wline in wrapped {
                    lines.push(Line::from(vec![Span::raw("  "), Span::raw(wline)]));
                }
            }
            Role::System => {
                for sline in msg.text.lines() {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {sline}"),
                        theme::dim_style(),
                    )]));
                }
            }
            Role::Tool => {
                if let Some(ref info) = msg.tool {
                    let max_val = width.saturating_sub(14);
                    let is_err = info.is_error;
                    let border_color = if is_err { theme::RED } else { theme::GREEN };
                    let icon = if info.result.is_empty() {
                        "\u{2026}" // … (running)
                    } else if is_err {
                        "\u{2718}" // ✘
                    } else {
                        "\u{2714}" // ✔
                    };
                    let icon_color = if is_err { theme::RED } else { theme::GREEN };

                    // Header: ┌─ ✔ tool_name ────────
                    let header_rest = width.saturating_sub(6 + info.name.len());
                    let fill = "\u{2500}".repeat(header_rest);
                    lines.push(Line::from(vec![
                        Span::styled("  \u{250c}\u{2500} ", Style::default().fg(border_color)),
                        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                        Span::styled(
                            info.name.clone(),
                            Style::default()
                                .fg(theme::YELLOW)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!(" {fill}"), Style::default().fg(border_color)),
                    ]));

                    // Input line (skip if empty)
                    if !info.input.is_empty() {
                        let val = truncate_line(&info.input, max_val);
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2502} ", Style::default().fg(border_color)),
                            Span::styled("input: ", theme::dim_style()),
                            Span::raw(val),
                        ]));
                    }

                    // Result / error / running line
                    if info.result.is_empty() {
                        let spinner = theme::SPINNER_FRAMES
                            [state.spinner_frame % theme::SPINNER_FRAMES.len()];
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2502} ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("{spinner} running\u{2026}"),
                                Style::default().fg(theme::CYAN),
                            ),
                        ]));
                    } else if is_err {
                        let val = truncate_line(&info.result, max_val);
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2502} ", Style::default().fg(border_color)),
                            Span::styled("error: ", Style::default().fg(theme::RED)),
                            Span::raw(val),
                        ]));
                    } else {
                        let val = truncate_line(&info.result, max_val);
                        lines.push(Line::from(vec![
                            Span::styled("  \u{2502} ", Style::default().fg(border_color)),
                            Span::styled("result: ", theme::dim_style()),
                            Span::raw(val),
                        ]));
                    }

                    // Footer: └───────────
                    let footer_fill = "\u{2500}".repeat(width.saturating_sub(4));
                    lines.push(Line::from(vec![Span::styled(
                        format!("  \u{2514}{footer_fill}"),
                        Style::default().fg(border_color),
                    )]));
                } else {
                    // Fallback for tool messages without ToolInfo
                    lines.push(Line::from(vec![Span::styled(
                        format!("  \u{2714} {}", msg.text),
                        Style::default().fg(theme::YELLOW),
                    )]));
                }
            }
        }
    }

    // Add streaming text if any
    if !state.streaming_text.is_empty() {
        lines.push(Line::from(""));
        let wrapped = wrap_text(&state.streaming_text, width.saturating_sub(4));
        for wline in wrapped {
            lines.push(Line::from(vec![Span::raw("  "), Span::raw(wline)]));
        }
    }

    // Add "thinking..." spinner while waiting for first token
    if state.thinking {
        let spinner = theme::SPINNER_FRAMES[state.spinner_frame];
        lines.push(Line::from(vec![
            Span::styled(format!("  {spinner} "), Style::default().fg(theme::CYAN)),
            Span::styled("thinking\u{2026}", Style::default().fg(theme::DIM)),
        ]));
    }

    // Add tool spinner if active
    if let Some(ref tool_name) = state.active_tool {
        let spinner = theme::SPINNER_FRAMES[state.spinner_frame];
        lines.push(Line::from(vec![
            Span::styled(format!("  {spinner} "), Style::default().fg(theme::RED)),
            Span::styled(tool_name.clone(), Style::default().fg(theme::YELLOW)),
        ]));
    }

    // Show estimated token count during streaming (~4 chars per token)
    if state.is_streaming && state.streaming_chars > 0 {
        let est_tokens = state.streaming_chars / 4;
        lines.push(Line::from(vec![Span::styled(
            format!("  ~{est_tokens} tokens"),
            theme::dim_style(),
        )]));
    }

    // Add token usage and cost if available
    if let Some((input, output)) = state.last_tokens {
        if input > 0 || output > 0 {
            let cost_str = match state.last_cost_usd {
                Some(c) if c > 0.0 => format!(" | ${:.4}", c),
                _ => String::new(),
            };
            lines.push(Line::from(vec![Span::styled(
                format!("  [tokens: {} in / {} out{}]", input, output, cost_str),
                theme::dim_style(),
            )]));
        }
    }

    // Add status message if any
    if let Some(ref msg) = state.status_msg {
        lines.push(Line::from(vec![Span::styled(
            format!("  {msg}"),
            Style::default().fg(theme::RED),
        )]));
    }

    // Compute scroll — we want to show the bottom of the chat by default
    let total_lines = lines.len() as u16;
    let visible_height = area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = max_scroll
        .saturating_sub(state.scroll_offset)
        .min(max_scroll);

    let para = Paragraph::new(lines).scroll((scroll, 0));
    f.render_widget(para, area);

    // Show scroll indicator if not at bottom
    if state.scroll_offset > 0 && total_lines > visible_height {
        let above = scroll;
        let below = total_lines.saturating_sub(scroll + visible_height);
        let indicator = format!("{}↑ {}↓", above, below);
        let ind_area = Rect {
            x: area.x + area.width.saturating_sub(indicator.len() as u16 + 1),
            y: area.y + area.height.saturating_sub(1),
            width: indicator.len() as u16,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(indicator, theme::dim_style())),
            ind_area,
        );
    }
}

/// Simple word-wrapping.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in line.split_whitespace() {
            if current.is_empty() {
                current = word.to_string();
            } else if current.len() + 1 + word.len() <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                result.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

/// Strip leaked `<function>...</function>` tags from streaming text.
fn sanitize_function_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<function>") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</function>") {
            rest = &rest[start + end + "</function>".len()..];
        } else {
            // Unclosed tag — drop from <function> to end
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Truncate a string to `max_len` chars, appending `…` if truncated.
fn truncate_line(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}\u{2026}",
            openfang_types::truncate_str(s, max_len.saturating_sub(1))
        )
    }
}
