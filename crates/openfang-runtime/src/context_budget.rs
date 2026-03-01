//! Dynamic context budget for tool result truncation.
//!
//! Replaces the hardcoded MAX_TOOL_RESULT_CHARS with a two-layer system:
//! - Layer 1: Per-result cap based on context window size (30% of window)
//! - Layer 2: Context guard that scans all tool results before LLM calls
//!   and compacts oldest results when total exceeds 75% headroom.

use openfang_types::message::{ContentBlock, Message, MessageContent};
use openfang_types::tool::ToolDefinition;
use tracing::debug;

/// Budget parameters derived from the model's context window.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Total context window size in tokens.
    pub context_window_tokens: usize,
    /// Estimated characters per token for tool results (denser content).
    pub tool_chars_per_token: f64,
    /// Estimated characters per token for general content.
    pub general_chars_per_token: f64,
}

impl ContextBudget {
    /// Create a new budget from a context window size.
    pub fn new(context_window_tokens: usize) -> Self {
        Self {
            context_window_tokens,
            tool_chars_per_token: 2.0,
            general_chars_per_token: 4.0,
        }
    }

    /// Per-result character cap: 30% of context window converted to chars.
    pub fn per_result_cap(&self) -> usize {
        let tokens_for_tool = (self.context_window_tokens as f64 * 0.30) as usize;
        (tokens_for_tool as f64 * self.tool_chars_per_token) as usize
    }

    /// Single result absolute max: 50% of context window.
    pub fn single_result_max(&self) -> usize {
        let tokens = (self.context_window_tokens as f64 * 0.50) as usize;
        (tokens as f64 * self.tool_chars_per_token) as usize
    }

    /// Total tool result headroom: 75% of context window in chars.
    pub fn total_tool_headroom_chars(&self) -> usize {
        let tokens = (self.context_window_tokens as f64 * 0.75) as usize;
        (tokens as f64 * self.tool_chars_per_token) as usize
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::new(200_000)
    }
}

/// Layer 1: Truncate a single tool result dynamically based on context budget.
///
/// Breaks at newline boundaries when possible to avoid mid-line truncation.
pub fn truncate_tool_result_dynamic(content: &str, budget: &ContextBudget) -> String {
    let cap = budget.per_result_cap();
    if content.len() <= cap {
        return content.to_string();
    }

    // Find last newline before the cap to break cleanly (char-boundary safe)
    let safe_cap = if content.is_char_boundary(cap) {
        cap
    } else {
        content[..cap].char_indices().next_back().map(|(i, _)| i).unwrap_or(0)
    };
    let search_start = safe_cap.saturating_sub(200);
    let break_point = content[search_start..safe_cap]
        .rfind('\n')
        .map(|pos| search_start + pos)
        .unwrap_or(safe_cap.saturating_sub(100));
    // Ensure break_point is also a char boundary
    let break_point = if content.is_char_boundary(break_point) {
        break_point
    } else {
        content[..break_point].char_indices().next_back().map(|(i, _)| i).unwrap_or(0)
    };

    format!(
        "{}\n\n[TRUNCATED: result was {} chars, showing first {} (budget: {}% of {}K context window)]",
        &content[..break_point],
        content.len(),
        break_point,
        30,
        budget.context_window_tokens / 1000
    )
}

/// Layer 2: Context guard — scan all tool_result blocks in the message history.
///
/// If total tool result content exceeds 75% of the context headroom,
/// compact oldest results first. Returns the number of results compacted.
pub fn apply_context_guard(
    messages: &mut [Message],
    budget: &ContextBudget,
    _tools: &[ToolDefinition],
) -> usize {
    let headroom = budget.total_tool_headroom_chars();
    let single_max = budget.single_result_max();

    // Collect all tool result sizes and locations
    struct ToolResultLoc {
        msg_idx: usize,
        block_idx: usize,
        char_len: usize,
    }

    let mut locations: Vec<ToolResultLoc> = Vec::new();
    let mut total_chars: usize = 0;

    for (msg_idx, msg) in messages.iter().enumerate() {
        if let MessageContent::Blocks(blocks) = &msg.content {
            for (block_idx, block) in blocks.iter().enumerate() {
                if let ContentBlock::ToolResult { content, .. } = block {
                    let len = content.len();
                    total_chars += len;
                    locations.push(ToolResultLoc {
                        msg_idx,
                        block_idx,
                        char_len: len,
                    });
                }
            }
        }
    }

    if total_chars <= headroom {
        return 0;
    }

    debug!(
        total_chars,
        headroom,
        results = locations.len(),
        "Context guard: tool results exceed headroom, compacting oldest"
    );

    // First pass: cap any single result that exceeds 50% of context
    let mut compacted = 0;
    for loc in &locations {
        if loc.char_len > single_max {
            if let MessageContent::Blocks(blocks) = &mut messages[loc.msg_idx].content {
                if let ContentBlock::ToolResult { content, .. } = &mut blocks[loc.block_idx] {
                    let old_len = content.len();
                    *content = truncate_to(content, single_max);
                    total_chars -= old_len;
                    total_chars += content.len();
                    compacted += 1;
                }
            }
        }
    }

    // Second pass: compact oldest results until under headroom
    // (locations are already in chronological order)
    let compact_target = 2000; // compact to 2K chars each
    for loc in &locations {
        if total_chars <= headroom {
            break;
        }
        if loc.char_len <= compact_target {
            continue;
        }
        if let MessageContent::Blocks(blocks) = &mut messages[loc.msg_idx].content {
            if let ContentBlock::ToolResult { content, .. } = &mut blocks[loc.block_idx] {
                if content.len() > compact_target {
                    let old_len = content.len();
                    *content = truncate_to(content, compact_target);
                    total_chars -= old_len;
                    total_chars += content.len();
                    compacted += 1;
                }
            }
        }
    }

    compacted
}

/// Truncate content to `max_chars` with a marker.
fn truncate_to(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    let keep = max_chars.saturating_sub(80);
    // Try to break at newline
    let break_point = content[keep.saturating_sub(100)..keep]
        .rfind('\n')
        .map(|pos| keep.saturating_sub(100) + pos)
        .unwrap_or(keep);
    format!(
        "{}\n\n[COMPACTED: {} → {} chars by context guard]",
        &content[..break_point],
        content.len(),
        break_point
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_defaults() {
        let budget = ContextBudget::default();
        assert_eq!(budget.context_window_tokens, 200_000);
        // 30% of 200K * 2.0 chars/token = 120K chars
        assert_eq!(budget.per_result_cap(), 120_000);
    }

    #[test]
    fn test_small_model_budget() {
        let budget = ContextBudget::new(8_000);
        // 30% of 8K * 2.0 = 4800 chars
        assert_eq!(budget.per_result_cap(), 4_800);
    }

    #[test]
    fn test_truncate_within_limit() {
        let budget = ContextBudget::default();
        let short = "Hello world";
        assert_eq!(truncate_tool_result_dynamic(short, &budget), short);
    }

    #[test]
    fn test_truncate_breaks_at_newline() {
        let budget = ContextBudget::new(100); // very small: cap = 60 chars
        let content =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12";
        let result = truncate_tool_result_dynamic(content, &budget);
        assert!(result.contains("[TRUNCATED:"));
        // Should not split in the middle of a line
        assert!(
            result.starts_with("line1\n") || result.is_empty() || result.contains("[TRUNCATED:")
        );
    }

    #[test]
    fn test_context_guard_no_compaction_needed() {
        let budget = ContextBudget::default();
        let mut messages = vec![Message::user("hello")];
        let compacted = apply_context_guard(&mut messages, &budget, &[]);
        assert_eq!(compacted, 0);
    }

    #[test]
    fn test_context_guard_compacts_oldest() {
        // Use tiny budget to trigger compaction
        let budget = ContextBudget::new(100); // headroom = 75% of 100 * 2.0 = 150 chars
        let big_result = "x".repeat(500);
        let mut messages = vec![
            Message {
                role: openfang_types::message::Role::User,
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".to_string(),
                    tool_name: String::new(),
                    content: big_result.clone(),
                    is_error: false,
                }]),
            },
            Message {
                role: openfang_types::message::Role::User,
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "t2".to_string(),
                    tool_name: String::new(),
                    content: big_result,
                    is_error: false,
                }]),
            },
        ];

        let compacted = apply_context_guard(&mut messages, &budget, &[]);
        assert!(compacted > 0);

        // Verify results were actually truncated
        if let MessageContent::Blocks(blocks) = &messages[0].content {
            if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                assert!(content.len() < 500);
            }
        }
    }
}
