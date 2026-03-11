//! Channel-specific message formatting.
//!
//! Converts standard Markdown into platform-specific markup:
//! - Telegram HTML: `**bold**` → `<b>bold</b>`
//! - Slack mrkdwn: `**bold**` → `*bold*`, `[text](url)` → `<url|text>`
//! - Plain text: strips all formatting

use openfang_types::config::OutputFormat;
use unicode_width::UnicodeWidthStr;

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
/// Supported tags: `<b>`, `<i>`, `<u>`, `<s>`, `<tg-spoiler>`,
/// `<code>`, `<pre>`, `<blockquote>`, `<a href="">`.
fn markdown_to_telegram_html(text: &str) -> String {
    let mut placeholders = Vec::new();
    let mut result = replace_fenced_code_blocks(text, &mut placeholders);
    result = replace_markdown_tables(&result, &mut placeholders);
    result = replace_markdown_blockquotes(&result, &mut placeholders);
    result = format_telegram_inline(&result, &mut placeholders);

    restore_placeholders(result, &placeholders)
}

fn escape_telegram_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn stash_placeholder(placeholders: &mut Vec<(String, String)>, rendered: String) -> String {
    let token = format!("@@TG_PLACEHOLDER_{}@@", placeholders.len());
    placeholders.push((token.clone(), rendered));
    token
}

fn restore_placeholders(mut text: String, placeholders: &[(String, String)]) -> String {
    for (token, rendered) in placeholders.iter().rev() {
        text = text.replace(token, rendered);
    }
    text
}

fn format_telegram_inline(text: &str, placeholders: &mut Vec<(String, String)>) -> String {
    let mut result = escape_telegram_html(text);
    result = replace_inline_code(&result, placeholders);
    result = replace_markdown_links(result);
    result = replace_markdown_spoiler(result);
    result = replace_markdown_strikethrough(result);
    result = replace_markdown_underline(result);
    result = replace_markdown_bold(result);
    replace_markdown_italic(&result)
}

fn replace_fenced_code_blocks(text: &str, placeholders: &mut Vec<(String, String)>) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut rendered = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            if j < lines.len() {
                let code = lines[i + 1..j].join("\n");
                rendered.push(stash_placeholder(
                    placeholders,
                    format!("<pre>{}</pre>", escape_telegram_html(&code)),
                ));
                i = j + 1;
                continue;
            }
        }

        rendered.push(lines[i].to_string());
        i += 1;
    }

    rendered.join("\n")
}

fn replace_markdown_tables(text: &str, placeholders: &mut Vec<(String, String)>) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut rendered = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if let Some((table_html, consumed)) = parse_markdown_table(&lines[i..]) {
            rendered.push(stash_placeholder(placeholders, table_html));
            i += consumed;
            continue;
        }

        rendered.push(lines[i].to_string());
        i += 1;
    }

    rendered.join("\n")
}

fn replace_markdown_blockquotes(text: &str, placeholders: &mut Vec<(String, String)>) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut rendered = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if let Some(first_line) = parse_blockquote_line(lines[i]) {
            let mut block = vec![first_line];
            i += 1;
            while i < lines.len() {
                match parse_blockquote_line(lines[i]) {
                    Some(line) => {
                        block.push(line);
                        i += 1;
                    }
                    None => break,
                }
            }
            let inner = format_telegram_inline(&block.join("\n"), placeholders);
            rendered.push(stash_placeholder(
                placeholders,
                format!("<blockquote>{inner}</blockquote>"),
            ));
            continue;
        }

        rendered.push(lines[i].to_string());
        i += 1;
    }

    rendered.join("\n")
}

fn parse_blockquote_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let stripped = trimmed.strip_prefix('>')?;
    Some(stripped.strip_prefix(' ').unwrap_or(stripped).to_string())
}

fn parse_markdown_table(lines: &[&str]) -> Option<(String, usize)> {
    if lines.len() < 2 {
        return None;
    }

    let header = parse_table_row(lines[0])?;
    if header.len() < 2 || !is_table_separator(lines[1]) {
        return None;
    }

    let mut rows = Vec::new();
    let mut consumed = 2;
    while consumed < lines.len() {
        match parse_table_row(lines[consumed]) {
            Some(row) => {
                rows.push(row);
                consumed += 1;
            }
            None => break,
        }
    }

    let table = render_markdown_table(&header, &rows);
    let rendered = if should_render_table_as_records(&header, &rows) {
        escape_telegram_html(&table)
    } else {
        format!("<pre>{}</pre>", escape_telegram_html(&table))
    };
    Some((rendered, consumed))
}

fn parse_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("```") || !trimmed.contains('|') {
        return None;
    }

    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let cells: Vec<String> = trimmed.split('|').map(|cell| cell.trim().to_string()).collect();
    (cells.len() >= 2).then_some(cells)
}

fn is_table_separator(line: &str) -> bool {
    parse_table_row(line).is_some_and(|cells| {
        cells.len() >= 2
            && cells
                .iter()
                .all(|cell| !cell.is_empty() && cell.contains('-') && cell.chars().all(|ch| ch == '-' || ch == ':'))
    })
}

fn render_markdown_table(header: &[String], rows: &[Vec<String>]) -> String {
    if should_render_table_as_records(header, rows) {
        return render_record_table(header, rows);
    }

    let col_count = std::iter::once(header.len())
        .chain(rows.iter().map(|row| row.len()))
        .max()
        .unwrap_or(0);

    let mut normalized = Vec::with_capacity(rows.len() + 1);
    let mut header_cells: Vec<String> = header
        .iter()
        .map(|cell| markdown_to_plain(cell).trim().to_string())
        .collect();
    header_cells.resize(col_count, String::new());
    normalized.push(header_cells);
    for row in rows {
        let mut cells: Vec<String> = row
            .iter()
            .map(|cell| markdown_to_plain(cell).trim().to_string())
            .collect();
        cells.resize(col_count, String::new());
        normalized.push(cells);
    }

    let mut widths = vec![0; col_count];
    for row in &normalized {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(display_width(cell)).max(1);
        }
    }

    let header_line = render_table_row(&normalized[0], &widths);
    let separator_line = widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>()
        .join("-+-");
    let body_lines = normalized
        .iter()
        .skip(1)
        .map(|row| render_table_row(row, &widths))
        .collect::<Vec<_>>();

    std::iter::once(header_line)
        .chain(std::iter::once(separator_line))
        .chain(body_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_record_table(header: &[String], rows: &[Vec<String>]) -> String {
    let headers: Vec<String> = header
        .iter()
        .map(|cell| markdown_to_plain(cell).trim().to_string())
        .collect();

    rows.iter()
        .map(|row| {
            headers
                .iter()
                .enumerate()
                .filter_map(|(idx, label)| {
                    let value = row
                        .get(idx)
                        .map(|cell| markdown_to_plain(cell).trim().to_string())
                        .unwrap_or_default();
                    if label.is_empty() || value.is_empty() {
                        None
                    } else {
                        Some(format!("{label}: {value}"))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_table_row(row: &[String], widths: &[usize]) -> String {
    row.iter()
        .zip(widths.iter())
        .map(|(cell, width)| {
            let padding = width.saturating_sub(display_width(cell));
            format!("{cell}{}", " ".repeat(padding))
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width_cjk(text)
}

fn contains_wide_chars(text: &str) -> bool {
    display_width(text) > text.chars().count()
}

fn should_render_table_as_records(header: &[String], rows: &[Vec<String>]) -> bool {
    header
        .iter()
        .chain(rows.iter().flatten())
        .any(|cell| contains_wide_chars(cell))
}

fn replace_inline_code(text: &str, placeholders: &mut Vec<(String, String)>) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let end = start + 1 + end;
            let inner = result[start + 1..end].to_string();
            let replacement = stash_placeholder(placeholders, format!("<code>{inner}</code>"));
            result = format!("{}{}{}", &result[..start], replacement, &result[end + 1..]);
        } else {
            break;
        }
    }
    result
}

fn replace_delimited_markup(
    mut result: String,
    delimiter: &str,
    open_tag: &str,
    close_tag: &str,
) -> String {
    while let Some(start) = result.find(delimiter) {
        if let Some(end) = result[start + delimiter.len()..].find(delimiter) {
            let end = start + delimiter.len() + end;
            let inner = result[start + delimiter.len()..end].to_string();
            result = format!(
                "{}{}{}{}{}",
                &result[..start],
                open_tag,
                inner,
                close_tag,
                &result[end + delimiter.len()..]
            );
        } else {
            break;
        }
    }
    result
}

fn replace_markdown_links(mut result: String) -> String {
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

fn replace_markdown_spoiler(result: String) -> String {
    replace_delimited_markup(result, "||", "<tg-spoiler>", "</tg-spoiler>")
}

fn replace_markdown_strikethrough(result: String) -> String {
    replace_delimited_markup(result, "~~", "<s>", "</s>")
}

fn replace_markdown_underline(result: String) -> String {
    replace_delimited_markup(result, "__", "<u>", "</u>")
}

fn replace_markdown_bold(result: String) -> String {
    replace_delimited_markup(result, "**", "<b>", "</b>")
}

fn replace_markdown_italic(result: &str) -> String {
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

    result = result
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(stripped) = trimmed.strip_prefix('>') {
                stripped.strip_prefix(' ').unwrap_or(stripped).to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Remove bold markers
    result = result.replace("**", "");
    result = result.replace("__", "");
    result = result.replace("~~", "");
    result = result.replace("||", "");

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
    fn test_telegram_html_extended_entities() {
        let result = markdown_to_telegram_html("__under__ ~~gone~~ ||secret||");
        assert_eq!(
            result,
            "<u>under</u> <s>gone</s> <tg-spoiler>secret</tg-spoiler>"
        );
    }

    #[test]
    fn test_telegram_html_blockquote_with_nested_formatting() {
        let result = markdown_to_telegram_html(
            "> quoted **bold**\n> second __line__ with ||spoiler||\n\noutside",
        );
        assert_eq!(
            result,
            "<blockquote>quoted <b>bold</b>\nsecond <u>line</u> with <tg-spoiler>spoiler</tg-spoiler></blockquote>\n\noutside"
        );
    }

    #[test]
    fn test_telegram_html_table() {
        let result = markdown_to_telegram_html(
            "| Name | Notes |\n| --- | --- |\n| **Alice** | [Docs](https://example.com) |\n| Bob | `ready` |",
        );
        assert!(result.starts_with("<pre>"));
        assert!(result.ends_with("</pre>"));
        assert!(result.contains("Name"));
        assert!(result.contains("Alice"));
        assert!(result.contains("Docs (https://example.com)"));
        assert!(result.contains("ready"));
        assert!(!result.contains("**Alice**"));
    }

    #[test]
    fn test_telegram_html_cjk_table_falls_back_to_records() {
        let result =
            markdown_to_telegram_html("| 名称 | 状态 |\n| --- | --- |\n| 中文 | ok |\n| 派遣周报 | 已同步 |");
        assert!(!result.contains("<pre>"));
        assert!(result.contains("名称: 中文"));
        assert!(result.contains("状态: ok"));
        assert!(result.contains("名称: 派遣周报"));
    }

    #[test]
    fn test_telegram_html_mixed_text_and_table() {
        let result = markdown_to_telegram_html(
            "Summary\n\n| Name | Value |\n| --- | --- |\n| Foo | 42 |\n\n**done**",
        );
        assert!(result.contains("Summary"));
        assert!(result.contains("<pre>Name"));
        assert!(result.contains("Foo"));
        assert!(result.contains("<b>done</b>"));
    }

    #[test]
    fn test_render_markdown_table_uses_display_width() {
        let header = vec!["Name".to_string(), "State".to_string()];
        let rows = vec![
            vec!["Alice".to_string(), "ok".to_string()],
            vec!["Bob".to_string(), "ready".to_string()],
        ];
        let table = render_markdown_table(&header, &rows);
        let widths: Vec<usize> = table.lines().map(display_width).collect();
        assert_eq!(widths, vec![13, 13, 13, 13]);
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
        let result = markdown_to_plain("**bold** and `code` and *italic* and ~~gone~~ and __under__");
        assert_eq!(result, "bold and code and italic and gone and under");
    }

    #[test]
    fn test_plain_text_converts_links() {
        let result = markdown_to_plain("[click](https://example.com)");
        assert_eq!(result, "click (https://example.com)");
    }

    #[test]
    fn test_plain_text_strips_blockquote_and_spoiler() {
        let result = markdown_to_plain("> quoted ||secret||");
        assert_eq!(result, "quoted secret");
    }
}
