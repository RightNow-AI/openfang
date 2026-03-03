//! Color palette matching the OpenFang landing page design system.
//!
//! Core palette from globals.css + code syntax from constants.ts.

#![allow(dead_code)] // Full palette — some colors reserved for future screens.

use ratatui::style::{Color, Modifier, Style};

// ── Core Palette (dark mode for terminal) ───────────────────────────────────

pub const ACCENT: Color = Color::Rgb(255, 92, 0); // #FF5C00 — OpenFang orange
pub const ACCENT_DIM: Color = Color::Rgb(224, 82, 0); // #E05200

pub const BG_PRIMARY: Color = Color::Rgb(15, 14, 14); // #0F0E0E — dark background
pub const BG_CARD: Color = Color::Rgb(31, 29, 28); // #1F1D1C — dark surface
pub const BG_HOVER: Color = Color::Rgb(42, 39, 37); // #2A2725 — dark hover
pub const BG_CODE: Color = Color::Rgb(24, 22, 21); // #181615 — dark code block

pub const TEXT_PRIMARY: Color = Color::Rgb(240, 239, 238); // #F0EFEE — light text on dark bg
pub const TEXT_SECONDARY: Color = Color::Rgb(168, 162, 158); // #A8A29E — muted text
pub const TEXT_TERTIARY: Color = Color::Rgb(120, 113, 108); // #78716C — dim text

pub const BORDER: Color = Color::Rgb(63, 59, 56); // #3F3B38 — dark border

// ── Semantic Colors (brighter variants for dark background contrast) ────────

pub const GREEN: Color = Color::Rgb(34, 197, 94); // #22C55E — success
pub const BLUE: Color = Color::Rgb(59, 130, 246); // #3B82F6 — info
pub const YELLOW: Color = Color::Rgb(234, 179, 8); // #EAB308 — warning
pub const RED: Color = Color::Rgb(239, 68, 68); // #EF4444 — error
pub const PURPLE: Color = Color::Rgb(168, 85, 247); // #A855F7 — decorators

// ── Backward-compat aliases ─────────────────────────────────────────────────

pub const CYAN: Color = BLUE;
pub const DIM: Color = TEXT_SECONDARY;
pub const TEXT: Color = TEXT_PRIMARY;

// ── Reusable styles ─────────────────────────────────────────────────────────

pub fn title_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().fg(ACCENT).bg(BG_HOVER)
}

pub fn dim_style() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

pub fn input_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn hint_style() -> Style {
    Style::default().fg(TEXT_TERTIARY)
}

// ── Tab bar styles ──────────────────────────────────────────────────────────

pub fn tab_active() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn tab_inactive() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

// ── State badge styles ──────────────────────────────────────────────────────

pub fn badge_running() -> Style {
    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
}

pub fn badge_created() -> Style {
    Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
}

pub fn badge_suspended() -> Style {
    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
}

pub fn badge_terminated() -> Style {
    Style::default().fg(TEXT_TERTIARY)
}

pub fn badge_crashed() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

/// Return badge text + style for an agent state string.
pub fn state_badge(state: &str) -> (&'static str, Style) {
    let lower = state.to_lowercase();
    if lower.contains("run") {
        ("[RUN]", badge_running())
    } else if lower.contains("creat") || lower.contains("new") || lower.contains("idle") {
        ("[NEW]", badge_created())
    } else if lower.contains("sus") || lower.contains("paus") {
        ("[SUS]", badge_suspended())
    } else if lower.contains("term") || lower.contains("stop") || lower.contains("end") {
        ("[END]", badge_terminated())
    } else if lower.contains("err") || lower.contains("crash") || lower.contains("fail") {
        ("[ERR]", badge_crashed())
    } else {
        ("[---]", dim_style())
    }
}

// ── Table / channel styles ──────────────────────────────────────────────────

pub fn table_header() -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub fn channel_ready() -> Style {
    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
}

pub fn channel_missing() -> Style {
    Style::default().fg(YELLOW)
}

pub fn channel_off() -> Style {
    dim_style()
}

// ── Spinner ─────────────────────────────────────────────────────────────────

pub const SPINNER_FRAMES: &[&str] = &[
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280f}",
];
