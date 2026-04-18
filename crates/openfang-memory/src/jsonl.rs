//! JSONL session mirror writer.
//! Pure filesystem utility — no database dependency.

use crate::session::Session;
use chrono::Utc;
use openfang_types::message::{ContentBlock, MessageContent, Role};
use std::io::Write;
use std::path::Path;

/// A single JSONL line in the session mirror file.
#[derive(serde::Serialize)]
struct JsonlLine {
    timestamp: String,
    role: String,
    content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_use: Option<serde_json::Value>,
}

/// Write a human-readable JSONL mirror of a session to disk.
///
/// Best-effort: errors are returned but should be logged and never
/// affect the primary SQLite store.
pub fn write_session_mirror(session: &Session, sessions_dir: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(sessions_dir)?;
    // SessionId wraps a uuid::Uuid (see openfang_types::agent::SessionId), so the filename is path-traversal safe.
    let path = sessions_dir.join(format!("{}.jsonl", session.id.0));
    let mut file = std::fs::File::create(&path)?;
    let now = Utc::now().to_rfc3339();

    for msg in &session.messages {
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        };

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_parts: Vec<serde_json::Value> = Vec::new();

        match &msg.content {
            MessageContent::Text(t) => {
                text_parts.push(t.clone());
            }
            MessageContent::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        ContentBlock::Text { text, .. } => {
                            text_parts.push(text.clone());
                        }
                        ContentBlock::ToolUse {
                            id, name, input, ..
                        } => {
                            tool_parts.push(serde_json::json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            tool_name: _,
                            content,
                            is_error,
                        } => {
                            tool_parts.push(serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content,
                                "is_error": is_error,
                            }));
                        }
                        ContentBlock::Image { media_type, .. } => {
                            text_parts.push(format!("[image: {media_type}]"));
                        }
                        ContentBlock::Thinking { thinking } => {
                            text_parts.push(format!(
                                "[thinking: {}]",
                                openfang_types::truncate_str(thinking, 200)
                            ));
                        }
                        ContentBlock::Unknown => {}
                    }
                }
            }
        }

        let line = JsonlLine {
            timestamp: now.clone(),
            role: role_str.to_string(),
            content: serde_json::Value::String(text_parts.join("\n")),
            tool_use: if tool_parts.is_empty() {
                None
            } else {
                Some(serde_json::Value::Array(tool_parts))
            },
        };

        serde_json::to_writer(&mut file, &line).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
    }

    Ok(())
}
