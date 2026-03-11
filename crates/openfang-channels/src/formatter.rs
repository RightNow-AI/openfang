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

/// Format a message for WeCom, using a stronger plain-text conversion to avoid
/// leaking Markdown syntax into enterprise chat replies.
pub fn format_for_wecom(text: &str, format: OutputFormat) -> String {
    match format {
        OutputFormat::PlainText => markdown_to_wecom_plain(text),
        _ => format_for_channel(text, format),
    }
}

/// Convert Markdown to Telegram HTML subset.
///
/// Supported tags: `<b>`, `<i>`, `<code>`, `<pre>`, `<a href="">`.
fn markdown_to_telegram_html(text: &str) -> String {
    // Escape HTML special characters first so agent names and other text
    // don't get interpreted as HTML tags by Telegram's parser.
    let mut result = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    // Bold: **text** → <b>text</b>
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let end = start + 2 + end;
            let inner = result[start + 2..end].to_string();
            result = format!("{}<b>{}</b>{}", &result[..start], inner, &result[end + 2..]);
        } else {
            break;
        }
    }

    // Italic: *text* → <i>text</i> (but not inside bold tags)
    // Simple heuristic: match single * not preceded/followed by *
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

    // Inline code: `text` → <code>text</code>
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

    // Links: [text](url) → <a href="url">text</a>
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

fn strip_atx_heading(line: &str) -> String {
    let trimmed = line.trim_start();
    let heading_level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&heading_level) {
        return line.to_string();
    }

    if trimmed.chars().nth(heading_level) != Some(' ') {
        return line.to_string();
    }

    trimmed[heading_level..]
        .trim()
        .trim_end_matches('#')
        .trim_end()
        .to_string()
}

fn strip_blockquote_prefix(line: &str) -> String {
    let mut trimmed = line.trim_start();
    while let Some(rest) = trimmed.strip_prefix('>') {
        trimmed = rest.trim_start();
    }
    trimmed.to_string()
}

fn strip_task_list_prefix(line: &str) -> String {
    let trimmed = line.trim_start();
    for prefix in [
        "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    line.to_string()
}

fn is_fenced_code_marker(line: &str) -> bool {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let Some(marker) = chars.next() else {
        return false;
    };
    if marker != '`' && marker != '~' {
        return false;
    }
    chars.all(|c| c == marker || c.is_ascii_alphanumeric())
}

fn is_setext_heading_underline(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    trimmed.chars().all(|c| c == '=' || c == '-') && trimmed.contains(['=', '-'])
}

fn is_table_divider(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| matches!(c, '|' | ':' | '-' | ' '))
}

fn strip_inline_markdown(mut text: String) -> String {
    while let Some(start) = text.find("![") {
        if let Some(mid) = text[start..].find("](") {
            let mid = start + mid;
            if let Some(end) = text[mid + 2..].find(')') {
                let end = mid + 2 + end;
                let alt = &text[start + 2..mid];
                let url = &text[mid + 2..end];
                let replacement = if alt.is_empty() {
                    url.to_string()
                } else {
                    format!("{alt} ({url})")
                };
                text = format!("{}{}{}", &text[..start], replacement, &text[end + 1..]);
                continue;
            }
        }
        break;
    }

    while let Some(start) = text.find('[') {
        if let Some(mid) = text[start..].find("](") {
            let mid = start + mid;
            if let Some(end) = text[mid + 2..].find(')') {
                let end = mid + 2 + end;
                let label = &text[start + 1..mid];
                let url = &text[mid + 2..end];
                text = format!("{}{} ({}){}", &text[..start], label, url, &text[end + 1..]);
                continue;
            }
        }
        break;
    }

    while let Some(start) = text.find('<') {
        if let Some(end) = text[start + 1..].find('>') {
            let end = start + 1 + end;
            let inner = &text[start + 1..end];
            if inner.starts_with("http://")
                || inner.starts_with("https://")
                || inner.starts_with("mailto:")
            {
                text = format!("{}{}{}", &text[..start], inner, &text[end + 1..]);
                continue;
            }
        }
        break;
    }

    text = text.replace("**", "");
    text = text.replace("__", "");
    text = text.replace("~~", "");
    text = text.replace('`', "");

    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '*'
            && (i == 0 || chars[i - 1] != '*')
            && (i + 1 >= chars.len() || chars[i + 1] != '*')
        {
            continue;
        }
        out.push(ch);
    }
    out
}

/// Strip common Markdown blocks for WeCom plain-text replies.
fn markdown_to_wecom_plain(text: &str) -> String {
    let mut result_lines = Vec::new();
    let mut in_fenced_code = false;

    for raw_line in text.replace("\r\n", "\n").lines() {
        let trimmed = raw_line.trim();

        if is_fenced_code_marker(trimmed) {
            in_fenced_code = !in_fenced_code;
            continue;
        }

        if in_fenced_code {
            result_lines.push(raw_line.trim_end().to_string());
            continue;
        }

        if is_setext_heading_underline(trimmed) || is_table_divider(trimmed) {
            continue;
        }

        let mut line = strip_atx_heading(raw_line);
        line = strip_blockquote_prefix(&line);
        line = strip_task_list_prefix(&line);

        let trimmed_line = line.trim();
        if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') && trimmed_line.len() > 2 {
            line = trimmed_line
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim())
                .collect::<Vec<_>>()
                .join("    ");
        }

        line = strip_inline_markdown(line);
        result_lines.push(line.trim().to_string());
    }

    let mut collapsed = Vec::new();
    for line in result_lines {
        if line.is_empty() && collapsed.last().is_some_and(|prev: &String| prev.is_empty()) {
            continue;
        }
        collapsed.push(line);
    }

    collapsed.join("\n").trim().to_string()
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

    #[test]
    fn test_wecom_plain_text_strips_common_markdown_blocks() {
        let result = markdown_to_wecom_plain(
            "# Title\n\
             \n\
             > quoted text\n\
             \n\
             - [x] done item\n\
             - [ ] todo item\n\
             \n\
             ```rust\n\
             let value = 1;\n\
             ```\n\
             \n\
             [docs](https://example.com)\n",
        );
        assert_eq!(
            result,
            "Title\n\nquoted text\n\ndone item\ntodo item\n\nlet value = 1;\n\ndocs (https://example.com)"
        );
    }
}
