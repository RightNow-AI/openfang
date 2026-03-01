//! Shared UI primitives for non-TUI subcommands (doctor, status, etc.).
//!
//! Uses `colored` for terminal output. The interactive TUI uses ratatui instead.

use colored::Colorize;

// ---------------------------------------------------------------------------
// Existing helpers
// ---------------------------------------------------------------------------

/// Doctor-style check: passed (green checkmark).
pub fn check_ok(msg: &str) {
    println!("  {} {}", "\u{2714}".bright_green(), msg);
}

/// Doctor-style check: warning (yellow dash).
pub fn check_warn(msg: &str) {
    println!("  {} {}", "-".bright_yellow(), msg.yellow());
}

/// Doctor-style check: failed (red cross).
pub fn check_fail(msg: &str) {
    println!("  {} {}", "\u{2718}".bright_red(), msg.bright_red());
}

/// Print a step/section header.
pub fn step(msg: &str) {
    println!("  {} {}", "\u{25cf}".bright_red(), msg.bold());
}

/// Print a success message.
pub fn success(msg: &str) {
    println!("  {} {}", "\u{2714}".bright_green(), msg);
}

/// Print an error message.
pub fn error(msg: &str) {
    println!("  {} {}", "\u{2718}".bright_red(), msg.bright_red());
}

// ---------------------------------------------------------------------------
// New themed output helpers
// ---------------------------------------------------------------------------

/// ANSI pixel-art banner embedded at compile time.
const ANSI_BANNER: &str = include_str!("../../../public/assets/ascii/banner.txt");

/// Brand banner: ANSI pixel-art wolf logo with tagline.
///
/// Falls back to a simple text banner if the terminal is narrower than 80 columns
/// or if stdout is not a TTY.
pub fn banner() {
    use ratatui::crossterm::{terminal, tty::IsTty};

    let is_tty = std::io::stdout().is_tty();
    let wide_enough = terminal::size().is_ok_and(|(cols, _)| cols >= 80);

    if is_tty && wide_enough {
        for line in ANSI_BANNER.lines() {
            // Strip cursor hide/show control sequences
            let line = line
                .trim_start_matches("\x1b[?25l")
                .trim_start_matches("\x1b[?25h")
                .trim_end_matches("\x1b[?25l")
                .trim_end_matches("\x1b[?25h");
            println!("{line}");
        }
    } else {
        // Compact fallback for narrow terminals / piped output
        println!(
            "  {} {}",
            ">>".bright_cyan().bold(),
            "OpenFang Agent OS".bold()
        );
        println!("     {}", "The open-source agent operating system".dimmed());
    }
}

/// Section header: ">> Title" in cyan.
pub fn section(title: &str) {
    println!("  {} {}", ">>".bright_cyan().bold(), title.bold());
}

/// Key-value display: "  Label:       value".
pub fn kv(label: &str, value: &str) {
    println!("  {:<13}{}", format!("{label}:"), value);
}

/// Key-value with green value.
pub fn kv_ok(label: &str, value: &str) {
    println!("  {:<13}{}", format!("{label}:"), value.bright_green());
}

/// Key-value with yellow value.
pub fn kv_warn(label: &str, value: &str) {
    println!("  {:<13}{}", format!("{label}:"), value.bright_yellow());
}

/// Hint line: "  hint: message" in dimmed text.
pub fn hint(msg: &str) {
    println!("  {} {}", "hint:".dimmed(), msg.dimmed());
}

/// Numbered "Next steps:" list.
pub fn next_steps(steps: &[&str]) {
    println!("  {}:", "Next steps".bold());
    for (i, step) in steps.iter().enumerate() {
        println!("    {}. {step}", i + 1);
    }
}

/// Suggest a command: "    label  command" with command highlighted.
pub fn suggest_cmd(label: &str, cmd: &str) {
    println!("    {:<22}{}", label, cmd.bright_cyan());
}

/// Red error + yellow "fix:" suggestion.
pub fn error_with_fix(msg: &str, fix: &str) {
    println!("  {} {}", "\u{2718}".bright_red(), msg.bright_red());
    println!("    {} {}", "fix:".bright_yellow(), fix);
}

/// Yellow warning + "try:" suggestion.
pub fn warn_with_fix(msg: &str, fix: &str) {
    println!("  {} {}", "-".bright_yellow(), msg.yellow());
    println!("    {} {}", "try:".bright_yellow(), fix);
}

/// Provider status line: checkmark/circle + name + env var.
pub fn provider_status(name: &str, env_var: &str, configured: bool) {
    if configured {
        println!("  {} {:<14} ({})", "\u{2714}".bright_green(), name, env_var);
    } else {
        println!(
            "  {} {:<14} ({} not set)",
            "\u{25cb}".dimmed(),
            name.dimmed(),
            env_var.dimmed()
        );
    }
}

/// Empty line.
pub fn blank() {
    println!();
}
