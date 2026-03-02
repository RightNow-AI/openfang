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

/// Convert Markdown to Telegram HTML subset.
///
/// Supported tags: `<b>`, `<i>`, `<s>`, `<code>`, `<pre>`, `<a href="">`.
///
/// Processing order matters:
/// 1. HTML entity escaping (before any tags are injected)
/// 2. Headings (line-level, before inline processing)
/// 3. Code blocks (triple backtick, before inline backtick)
/// 4. Bold, Strikethrough, Italic, Inline code, Links
fn markdown_to_telegram_html(text: &str) -> String {
    let mut result = text.to_string();

    // 1. Escape HTML entities first — user text with <, >, & must not break our tags.
    //    & must be replaced first to avoid double-encoding.
    result = result
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    // 2. Headings: # / ## / ### text → <b>text</b> (Telegram has no heading tags)
    let lines: Vec<&str> = result.lines().collect();
    let mut processed_lines = Vec::with_capacity(lines.len());
    for line in &lines {
        let trimmed = line.trim_start();
        if let Some(h) = trimmed.strip_prefix("### ") {
            processed_lines.push(format!("<b>{h}</b>"));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            processed_lines.push(format!("<b>{h}</b>"));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            processed_lines.push(format!("<b>{h}</b>"));
        } else {
            processed_lines.push(line.to_string());
        }
    }
    result = processed_lines.join("\n");

    // 3. Code blocks: ```lang\ncode``` → <pre>code</pre>
    //    Must happen before inline backtick to avoid consuming ``` as three ` pairs.
    while let Some(start) = result.find("```") {
        if let Some(end) = result[start + 3..].find("```") {
            let end = start + 3 + end;
            let mut inner = result[start + 3..end].to_string();
            // Strip optional language identifier on first line
            if let Some(newline_pos) = inner.find('\n') {
                let first_line = inner[..newline_pos].trim();
                if !first_line.is_empty() && !first_line.contains(' ') {
                    inner = inner[newline_pos + 1..].to_string();
                }
            }
            result = format!(
                "{}<pre>{}</pre>{}",
                &result[..start],
                inner.trim(),
                &result[end + 3..]
            );
        } else {
            break;
        }
    }

    // 4. Bold: **text** → <b>text</b>
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let end = start + 2 + end;
            let inner = result[start + 2..end].to_string();
            result = format!("{}<b>{}</b>{}", &result[..start], inner, &result[end + 2..]);
        } else {
            break;
        }
    }

    // 5. Strikethrough: ~~text~~ → <s>text</s>
    while let Some(start) = result.find("~~") {
        if let Some(end) = result[start + 2..].find("~~") {
            let end = start + 2 + end;
            let inner = result[start + 2..end].to_string();
            result = format!("{}<s>{}</s>{}", &result[..start], inner, &result[end + 2..]);
        } else {
            break;
        }
    }

    // 6. Italic: *text* → <i>text</i> (single * not preceded/followed by *)
    let mut out = String::with_capacity(result.len());
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;
    let mut in_italic = false;
    while i < chars.len() {
        if chars[i] == '*'
            && (i == 0 || chars[i - 1] != '*')
            && (i + 1 >= chars.len() || chars[i + 1] != '*')
        {
            if in_italic {
                out.push_str("</i>");
            } else {
                out.push_str("<i>");
            }
            in_italic = !in_italic;
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    result = out;

    // 7. Inline code: `text` → <code>text</code>
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let end = start + 1 + end;
            let inner = result[start + 1..end].to_string();
            result = format!(
                "{}<code>{}</code>{}",
                &result[..start],
                inner,
                &result[end + 1..]
            );
        } else {
            break;
        }
    }

    // 8. Links: [text](url) → <a href="url">text</a>
    while let Some(bracket_start) = result.find('[') {
        if let Some(bracket_end) = result[bracket_start..].find("](") {
            let bracket_end = bracket_start + bracket_end;
            if let Some(paren_end) = result[bracket_end + 2..].find(')') {
                let paren_end = bracket_end + 2 + paren_end;
                let link_text = &result[bracket_start + 1..bracket_end];
                let url = &result[bracket_end + 2..paren_end];
                result = format!(
                    "{}<a href=\"{}\">{}</a>{}",
                    &result[..bracket_start],
                    url,
                    link_text,
                    &result[paren_end + 1..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

/// Convert Markdown to Slack mrkdwn format.
fn markdown_to_slack_mrkdwn(text: &str) -> String {
    let mut result = text.to_string();

    // Bold: **text** → *text*
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let end = start + 2 + end;
            let inner = result[start + 2..end].to_string();
            result = format!("{}*{}*{}", &result[..start], inner, &result[end + 2..]);
        } else {
            break;
        }
    }

    // Links: [text](url) → <url|text>
    while let Some(bracket_start) = result.find('[') {
        if let Some(bracket_end) = result[bracket_start..].find("](") {
            let bracket_end = bracket_start + bracket_end;
            if let Some(paren_end) = result[bracket_end + 2..].find(')') {
                let paren_end = bracket_end + 2 + paren_end;
                let link_text = &result[bracket_start + 1..bracket_end];
                let url = &result[bracket_end + 2..paren_end];
                result = format!(
                    "{}<{}|{}>{}",
                    &result[..bracket_start],
                    url,
                    link_text,
                    &result[paren_end + 1..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

/// Strip all Markdown formatting, producing plain text.
fn markdown_to_plain(text: &str) -> String {
    let mut result = text.to_string();

    // Remove bold markers
    result = result.replace("**", "");

    // Remove italic markers (single *)
    // Simple approach: remove isolated *
    let mut out = String::with_capacity(result.len());
    let chars: Vec<char> = result.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '*'
            && (i == 0 || chars[i - 1] != '*')
            && (i + 1 >= chars.len() || chars[i + 1] != '*')
        {
            continue;
        }
        out.push(ch);
    }
    result = out;

    // Remove inline code markers
    result = result.replace('`', "");

    // Convert links: [text](url) → text (url)
    while let Some(bracket_start) = result.find('[') {
        if let Some(bracket_end) = result[bracket_start..].find("](") {
            let bracket_end = bracket_start + bracket_end;
            if let Some(paren_end) = result[bracket_end + 2..].find(')') {
                let paren_end = bracket_end + 2 + paren_end;
                let link_text = &result[bracket_start + 1..bracket_end];
                let url = &result[bracket_end + 2..paren_end];
                result = format!(
                    "{}{} ({}){}",
                    &result[..bracket_start],
                    link_text,
                    url,
                    &result[paren_end + 1..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
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
        // Note: URL contains &amp; because of entity escaping, but simple URLs are fine
        assert!(result.contains("<a href="));
        assert!(result.contains("click here</a>"));
    }

    #[test]
    fn test_telegram_html_escaping() {
        let result = markdown_to_telegram_html("Use <div> & check");
        assert!(result.contains("&lt;div&gt;"));
        assert!(result.contains("&amp;"));
        assert!(!result.contains("<div>"));
    }

    #[test]
    fn test_telegram_html_code_block() {
        let result = markdown_to_telegram_html("Example:\n```rust\nfn main() {}\n```");
        assert!(result.contains("<pre>"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_telegram_html_heading() {
        assert_eq!(
            markdown_to_telegram_html("## Section Title"),
            "<b>Section Title</b>"
        );
        assert_eq!(
            markdown_to_telegram_html("# H1\n## H2\n### H3"),
            "<b>H1</b>\n<b>H2</b>\n<b>H3</b>"
        );
    }

    #[test]
    fn test_telegram_html_strikethrough() {
        let result = markdown_to_telegram_html("This is ~~deleted~~ text");
        assert_eq!(result, "This is <s>deleted</s> text");
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
