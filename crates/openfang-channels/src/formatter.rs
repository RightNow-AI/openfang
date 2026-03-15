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
/// Supported tags: `<b>`, `<i>`, `<code>`, `<pre>`, `<a href="">`, `<blockquote>`.
fn markdown_to_telegram_html(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks = Vec::new();
    let lines: Vec<&str> = normalized.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Fenced code block
        if let Some(fence) = fence_delimiter(trimmed) {
            i += 1;
            let mut code_lines = Vec::new();
            while i < lines.len() {
                let candidate = lines[i].trim();
                if candidate.starts_with(fence) {
                    i += 1;
                    break;
                }
                code_lines.push(lines[i]);
                i += 1;
            }
            let code = escape_html(&code_lines.join("\n"));
            blocks.push(format!("<pre><code>{}</code></pre>", code));
            continue;
        }

        // ATX heading (#, ##, ...)
        if let Some(content) = heading_text(trimmed) {
            blocks.push(format!("<b>{}</b>", render_inline_markdown(content.trim())));
            i += 1;
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            let mut quote_lines = Vec::new();
            while i < lines.len() {
                let current = lines[i].trim();
                if current.is_empty() || !current.starts_with('>') {
                    break;
                }
                let content = current
                    .strip_prefix('>')
                    .unwrap_or(current)
                    .trim_start();
                quote_lines.push(render_inline_markdown(content));
                i += 1;
            }
            blocks.push(format!("<blockquote>{}</blockquote>", quote_lines.join("\n")));
            continue;
        }

        // Unordered list
        if let Some(item) = unordered_list_item(trimmed) {
            let mut items = vec![format!("• {}", render_inline_markdown(item.trim()))];
            i += 1;
            while i < lines.len() {
                let current = lines[i].trim();
                if let Some(next_item) = unordered_list_item(current) {
                    items.push(format!("• {}", render_inline_markdown(next_item.trim())));
                    i += 1;
                } else if current.is_empty() {
                    i += 1;
                    break;
                } else {
                    break;
                }
            }
            blocks.push(items.join("\n"));
            continue;
        }

        // Ordered list
        if let Some(item) = ordered_list_item(trimmed) {
            let mut items = vec![format!("1. {}", render_inline_markdown(item.trim()))];
            let mut counter = 2;
            i += 1;
            while i < lines.len() {
                let current = lines[i].trim();
                if let Some(next_item) = ordered_list_item(current) {
                    items.push(format!("{}. {}", counter, render_inline_markdown(next_item.trim())));
                    counter += 1;
                    i += 1;
                } else if current.is_empty() {
                    i += 1;
                    break;
                } else {
                    break;
                }
            }
            blocks.push(items.join("\n"));
            continue;
        }

        // Paragraph
        let mut paragraph_lines = vec![trimmed];
        i += 1;
        while i < lines.len() {
            let current = lines[i].trim();
            if current.is_empty()
                || fence_delimiter(current).is_some()
                || heading_text(current).is_some()
                || current.starts_with('>')
                || unordered_list_item(current).is_some()
                || ordered_list_item(current).is_some()
            {
                break;
            }
            paragraph_lines.push(current);
            i += 1;
        }
        let joined = paragraph_lines.join("\n");
        blocks.push(render_inline_markdown(&joined));
    }

    blocks.join("\n\n")
}

fn render_inline_markdown(text: &str) -> String {
    let mut result = escape_html(text);

    // Links: [text](url) → <a href="url">text</a>
    while let Some(bracket_start) = result.find('[') {
        if let Some(bracket_end_rel) = result[bracket_start..].find("](") {
            let bracket_end = bracket_start + bracket_end_rel;
            if let Some(paren_end_rel) = result[bracket_end + 2..].find(')') {
                let paren_end = bracket_end + 2 + paren_end_rel;
                let link_text = result[bracket_start + 1..bracket_end].to_string();
                let url = result[bracket_end + 2..paren_end].to_string();
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

    // Bold: **text** → <b>text</b>
    while let Some(start) = result.find("**") {
        if let Some(end_rel) = result[start + 2..].find("**") {
            let end = start + 2 + end_rel;
            let inner = result[start + 2..end].to_string();
            result = format!("{}<b>{}</b>{}", &result[..start], inner, &result[end + 2..]);
        } else {
            break;
        }
    }

    // Inline code: `text` → <code>text</code>
    while let Some(start) = result.find('`') {
        if let Some(end_rel) = result[start + 1..].find('`') {
            let end = start + 1 + end_rel;
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

    // Italic: *text* → <i>text</i> (single star only)
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

    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fence_delimiter(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn heading_text(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ') {
        Some(&line[hashes + 1..])
    } else {
        None
    }
}

fn unordered_list_item(line: &str) -> Option<&str> {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

fn ordered_list_item(line: &str) -> Option<&str> {
    let digit_count = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let rest = &line[digit_count..];
    if let Some(item) = rest.strip_prefix(". ") {
        Some(item)
    } else if let Some(item) = rest.strip_prefix(") ") {
        Some(item)
    } else {
        None
    }
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
        assert_eq!(result, "<a href=\"https://example.com\">click here</a>");
    }

    #[test]
    fn test_telegram_html_heading() {
        let result = markdown_to_telegram_html("## Result");
        assert_eq!(result, "<b>Result</b>");
    }

    #[test]
    fn test_telegram_html_unordered_list() {
        let result = markdown_to_telegram_html("- alpha\n- beta");
        assert_eq!(result, "• alpha\n• beta");
    }

    #[test]
    fn test_telegram_html_ordered_list() {
        let result = markdown_to_telegram_html("1. alpha\n2. beta");
        assert_eq!(result, "1. alpha\n2. beta");
    }

    #[test]
    fn test_telegram_html_fenced_code_block() {
        let result = markdown_to_telegram_html("```rust\nfn main() {}\n```");
        assert_eq!(result, "<pre><code>fn main() {}</code></pre>");
    }

    #[test]
    fn test_telegram_html_blockquote() {
        let result = markdown_to_telegram_html("> note\n> second line");
        assert_eq!(result, "<blockquote>note\nsecond line</blockquote>");
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
