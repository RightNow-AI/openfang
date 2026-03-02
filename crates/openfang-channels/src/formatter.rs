//! Channel-specific message formatting.
//!
//! Converts standard Markdown into platform-specific markup:
//! - Telegram HTML: `**bold**` → `<b>bold</b>`
//! - Slack mrkdwn: `**bold**` → `*bold*`, `[text](url)` → `<url|text>`
//! - Plain text: strips all formatting

use openfang_types::config::OutputFormat;

/// Format a message for a specific channel output format.
pub fn format_for_channel(text: &str, format: OutputFormat) -> String {
    match format {
        OutputFormat::Markdown => text.to_string(),
        OutputFormat::TelegramHtml => markdown_to_telegram_html(text),
        OutputFormat::SlackMrkdwn => markdown_to_slack_mrkdwn(text),
        OutputFormat::PlainText => markdown_to_plain(text),
    }
}

/// Convert Markdown to Telegram HTML subset (single-pass).
///
/// Supported tags: `<b>`, `<i>`, `<code>`, `<pre>`, `<a href="">`.
fn markdown_to_telegram_html(text: &str) -> String {
    let b = text.as_bytes();
    let len = b.len();
    // Generous pre-allocation: tags expand the output
    let mut out = String::with_capacity(len + len / 4);
    let mut i = 0;

    while i < len {
        // Bold: **text**
        if i + 1 < len && b[i] == b'*' && b[i + 1] == b'*' {
            if let Some(end) = find_closing(b, i + 2, b"**") {
                out.push_str("<b>");
                out.push_str(&text[i + 2..end]);
                out.push_str("</b>");
                i = end + 2;
                continue;
            }
        }
        // Italic: single * (not preceded/followed by *)
        if b[i] == b'*' && (i + 1 >= len || b[i + 1] != b'*') && (i == 0 || b[i - 1] != b'*') {
            if let Some(end) = find_closing_single_star(b, i + 1) {
                out.push_str("<i>");
                out.push_str(&text[i + 1..end]);
                out.push_str("</i>");
                i = end + 1;
                continue;
            }
        }
        // Inline code: `text`
        if b[i] == b'`' {
            if let Some(end) = memchr_byte(b'`', &b[i + 1..]) {
                let end = i + 1 + end;
                out.push_str("<code>");
                out.push_str(&text[i + 1..end]);
                out.push_str("</code>");
                i = end + 1;
                continue;
            }
        }
        // Link: [text](url)
        if b[i] == b'[' {
            if let Some((link_end, url_end)) = parse_md_link(b, i) {
                out.push_str("<a href=\"");
                out.push_str(&text[link_end + 2..url_end]);
                out.push_str("\">");
                out.push_str(&text[i + 1..link_end]);
                out.push_str("</a>");
                i = url_end + 1;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// Convert Markdown to Slack mrkdwn format (single-pass).
fn markdown_to_slack_mrkdwn(text: &str) -> String {
    let b = text.as_bytes();
    let len = b.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        // Bold: **text** → *text*
        if i + 1 < len && b[i] == b'*' && b[i + 1] == b'*' {
            if let Some(end) = find_closing(b, i + 2, b"**") {
                out.push('*');
                out.push_str(&text[i + 2..end]);
                out.push('*');
                i = end + 2;
                continue;
            }
        }
        // Link: [text](url) → <url|text>
        if b[i] == b'[' {
            if let Some((link_end, url_end)) = parse_md_link(b, i) {
                out.push('<');
                out.push_str(&text[link_end + 2..url_end]);
                out.push('|');
                out.push_str(&text[i + 1..link_end]);
                out.push('>');
                i = url_end + 1;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// Strip all Markdown formatting, producing plain text (single-pass).
fn markdown_to_plain(text: &str) -> String {
    let b = text.as_bytes();
    let len = b.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        // Bold: **text** → text
        if i + 1 < len && b[i] == b'*' && b[i + 1] == b'*' {
            if let Some(end) = find_closing(b, i + 2, b"**") {
                out.push_str(&text[i + 2..end]);
                i = end + 2;
                continue;
            }
        }
        // Italic: *text* → text
        if b[i] == b'*' && (i + 1 >= len || b[i + 1] != b'*') && (i == 0 || b[i - 1] != b'*') {
            if let Some(end) = find_closing_single_star(b, i + 1) {
                out.push_str(&text[i + 1..end]);
                i = end + 1;
                continue;
            }
        }
        // Inline code: `text` → text
        if b[i] == b'`' {
            if let Some(end) = memchr_byte(b'`', &b[i + 1..]) {
                let end = i + 1 + end;
                out.push_str(&text[i + 1..end]);
                i = end + 1;
                continue;
            }
        }
        // Link: [text](url) → text (url)
        if b[i] == b'[' {
            if let Some((link_end, url_end)) = parse_md_link(b, i) {
                out.push_str(&text[i + 1..link_end]);
                out.push_str(" (");
                out.push_str(&text[link_end + 2..url_end]);
                out.push(')');
                i = url_end + 1;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Shared helpers for single-pass markdown scanning
// ---------------------------------------------------------------------------

/// Find closing delimiter (e.g. `**`) starting from `start` in `b`.
fn find_closing(b: &[u8], start: usize, delim: &[u8]) -> Option<usize> {
    let dlen = delim.len();
    if dlen == 0 || start + dlen > b.len() {
        return None;
    }
    let mut i = start;
    while i + dlen <= b.len() {
        if &b[i..i + dlen] == delim {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find closing single `*` that is not part of `**`.
fn find_closing_single_star(b: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < b.len() {
        if b[i] == b'*' && (i + 1 >= b.len() || b[i + 1] != b'*') {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Simple byte search (like memchr but without the dep).
fn memchr_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&b| b == needle)
}

/// Parse `[text](url)` starting at position `start` (which points to `[`).
/// Returns `(bracket_end, paren_end)` — the positions of `]` and `)`.
fn parse_md_link(b: &[u8], start: usize) -> Option<(usize, usize)> {
    // Find `](` after `[`
    let mut i = start + 1;
    while i + 1 < b.len() {
        if b[i] == b']' && b[i + 1] == b'(' {
            let bracket_end = i;
            // Find closing `)`
            let mut j = i + 2;
            while j < b.len() {
                if b[j] == b')' {
                    return Some((bracket_end, j));
                }
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_markdown_passthrough() {
        let text = "**bold** and *italic*";
        assert_eq!(format_for_channel(text, OutputFormat::Markdown), text);
    }

    #[test]
    fn test_telegram_html_bold() {
        let result = markdown_to_telegram_html("Hello **world**!");
        assert_eq!(result, "Hello <b>world</b>!");
    }

    #[test]
    fn test_telegram_html_italic() {
        let result = markdown_to_telegram_html("Hello *world*!");
        assert_eq!(result, "Hello <i>world</i>!");
    }

    #[test]
    fn test_telegram_html_code() {
        let result = markdown_to_telegram_html("Use `println!`");
        assert_eq!(result, "Use <code>println!</code>");
    }

    #[test]
    fn test_telegram_html_link() {
        let result = markdown_to_telegram_html("[click here](https://example.com)");
        assert_eq!(result, "<a href=\"https://example.com\">click here</a>");
    }

    #[test]
    fn test_slack_mrkdwn_bold() {
        let result = markdown_to_slack_mrkdwn("Hello **world**!");
        assert_eq!(result, "Hello *world*!");
    }

    #[test]
    fn test_slack_mrkdwn_link() {
        let result = markdown_to_slack_mrkdwn("[click](https://example.com)");
        assert_eq!(result, "<https://example.com|click>");
    }

    #[test]
    fn test_plain_text_strips_formatting() {
        let result = markdown_to_plain("**bold** and `code` and *italic*");
        assert_eq!(result, "bold and code and italic");
    }

    #[test]
    fn test_plain_text_converts_links() {
        let result = markdown_to_plain("[click](https://example.com)");
        assert_eq!(result, "click (https://example.com)");
    }
}
