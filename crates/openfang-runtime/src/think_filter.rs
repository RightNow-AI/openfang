//! Streaming think-tag filter.
//!
//! Some LLMs (DeepSeek-R1, Qwen3, local models via Ollama/vLLM) embed
//! `<think>...</think>` reasoning blocks in their streamed content deltas.
//! In non-streaming mode, the full text is assembled and then
//! [`extract_think_tags`](crate::drivers::openai) strips them out.
//!
//! In streaming mode, text deltas are forwarded to the client as they arrive.
//! This module provides [`StreamingThinkFilter`] — a stateful filter that sits
//! between the SSE parser and the `StreamEvent` sender. It buffers text while
//! inside a `<think>` block, emitting only visible text as `TextDelta` and
//! routing think content as `ThinkingDelta`.

/// Actions the filter can produce for each incoming text delta.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterAction {
    /// Emit visible text to the client.
    EmitText(String),
    /// Emit thinking/reasoning text (not shown to user by default).
    EmitThinking(String),
}

/// A stateful streaming filter for `<think>...</think>` tags.
///
/// Feed each text delta through [`process`] and collect the resulting
/// [`FilterAction`]s. The filter handles partial tag boundaries that may
/// be split across multiple deltas.
pub struct StreamingThinkFilter {
    /// Whether we are currently inside a `<think>` block.
    inside_think: bool,
    /// Buffer that accumulates text when we might be at a tag boundary.
    /// This holds characters that *could* be the start of `<think>` or
    /// `</think>` but we haven't seen enough to decide yet.
    pending: String,
}

impl StreamingThinkFilter {
    /// Create a new filter in the default (outside-think) state.
    pub fn new() -> Self {
        Self {
            inside_think: false,
            pending: String::new(),
        }
    }

    /// Returns `true` if we are currently inside a `<think>` block.
    pub fn is_inside_think(&self) -> bool {
        self.inside_think
    }

    /// Process an incoming text delta and return zero or more actions.
    pub fn process(&mut self, delta: &str) -> Vec<FilterAction> {
        self.pending.push_str(delta);
        let mut actions = Vec::new();

        loop {
            if self.inside_think {
                // Look for `</think>` in the pending buffer
                if let Some(end_pos) = self.pending.find("</think>") {
                    // Everything before the closing tag is thinking content
                    let thinking = self.pending[..end_pos].to_string();
                    if !thinking.is_empty() {
                        actions.push(FilterAction::EmitThinking(thinking));
                    }
                    // Consume the tag
                    self.pending = self.pending[end_pos + "</think>".len()..].to_string();
                    self.inside_think = false;
                    // Continue processing — there may be more tags
                    continue;
                }

                // No complete `</think>` found. Check if the tail of pending
                // could be the start of `</think>` (partial match).
                let keep = partial_suffix_match(&self.pending, "</think>");
                let emit_len = self.pending.len() - keep;
                if emit_len > 0 {
                    let thinking = self.pending[..emit_len].to_string();
                    if !thinking.is_empty() {
                        actions.push(FilterAction::EmitThinking(thinking));
                    }
                    self.pending = self.pending[emit_len..].to_string();
                }
                // We either emitted what we could or everything is a partial match — wait for more data.
                break;
            } else {
                // Outside a think block — look for `<think>`
                if let Some(start_pos) = self.pending.find("<think>") {
                    // Everything before the opening tag is visible text
                    let visible = self.pending[..start_pos].to_string();
                    if !visible.is_empty() {
                        actions.push(FilterAction::EmitText(visible));
                    }
                    // Consume the tag
                    self.pending = self.pending[start_pos + "<think>".len()..].to_string();
                    self.inside_think = true;
                    // Continue processing — there may be `</think>` in the same delta
                    continue;
                }

                // No complete `<think>` found. Check if the tail could be
                // a partial `<think>` tag.
                let keep = partial_suffix_match(&self.pending, "<think>");
                let emit_len = self.pending.len() - keep;
                if emit_len > 0 {
                    let visible = self.pending[..emit_len].to_string();
                    if !visible.is_empty() {
                        actions.push(FilterAction::EmitText(visible));
                    }
                    self.pending = self.pending[emit_len..].to_string();
                }
                break;
            }
        }

        actions
    }

    /// Flush any remaining buffered content.
    ///
    /// Call this when the stream ends. If we're inside a `<think>` block,
    /// the pending text is emitted as thinking. If outside, it's emitted as
    /// visible text (it was only held back due to a potential partial tag).
    pub fn flush(&mut self) -> Vec<FilterAction> {
        let mut actions = Vec::new();
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            if self.inside_think {
                actions.push(FilterAction::EmitThinking(text));
            } else {
                actions.push(FilterAction::EmitText(text));
            }
        }
        actions
    }
}

impl Default for StreamingThinkFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute the length of the longest suffix of `haystack` that is a prefix of `needle`.
///
/// This tells us how many trailing bytes of `haystack` we must keep buffered
/// because they could be the beginning of a tag we haven't fully received yet.
fn partial_suffix_match(haystack: &str, needle: &str) -> usize {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    // Try longest possible suffix first (up to needle length - 1)
    let max_len = h.len().min(n.len() - 1);
    for len in (1..=max_len).rev() {
        if h.ends_with(&n[..len]) {
            return len;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_think_tags() {
        let mut filter = StreamingThinkFilter::new();
        let actions = filter.process("Hello world");
        assert_eq!(actions, vec![FilterAction::EmitText("Hello world".into())]);
        assert!(!filter.is_inside_think());
    }

    #[test]
    fn test_complete_think_block_single_delta() {
        let mut filter = StreamingThinkFilter::new();
        let actions = filter.process("<think>reasoning here</think>The answer is 42.");
        assert_eq!(
            actions,
            vec![
                FilterAction::EmitThinking("reasoning here".into()),
                FilterAction::EmitText("The answer is 42.".into()),
            ]
        );
        assert!(!filter.is_inside_think());
    }

    #[test]
    fn test_think_block_across_deltas() {
        let mut filter = StreamingThinkFilter::new();

        // Delta 1: opening tag starts
        let a1 = filter.process("<think>Let me ");
        assert_eq!(a1, vec![FilterAction::EmitThinking("Let me ".into())]);
        assert!(filter.is_inside_think());

        // Delta 2: more thinking
        let a2 = filter.process("reason about this...");
        assert_eq!(
            a2,
            vec![FilterAction::EmitThinking("reason about this...".into())]
        );
        assert!(filter.is_inside_think());

        // Delta 3: closing tag + visible text
        let a3 = filter.process("</think>The answer is 42.");
        assert_eq!(a3, vec![FilterAction::EmitText("The answer is 42.".into())]);
        assert!(!filter.is_inside_think());
    }

    #[test]
    fn test_partial_opening_tag() {
        let mut filter = StreamingThinkFilter::new();

        // Delta 1: ends with partial `<think>`
        let a1 = filter.process("Hello <thi");
        // "Hello " is safe to emit, "<thi" is buffered
        assert_eq!(a1, vec![FilterAction::EmitText("Hello ".into())]);

        // Delta 2: completes the tag
        let a2 = filter.process("nk>deep thought");
        // The tag is complete. "deep thought" is thinking.
        assert_eq!(a2, vec![FilterAction::EmitThinking("deep thought".into())]);
        assert!(filter.is_inside_think());
    }

    #[test]
    fn test_partial_closing_tag() {
        let mut filter = StreamingThinkFilter::new();

        // Enter think block
        let a1 = filter.process("<think>thinking here</thi");
        assert_eq!(a1, vec![FilterAction::EmitThinking("thinking here".into())]);
        assert!(filter.is_inside_think());

        // Complete the closing tag
        let a2 = filter.process("nk>visible text");
        assert_eq!(a2, vec![FilterAction::EmitText("visible text".into())]);
        assert!(!filter.is_inside_think());
    }

    #[test]
    fn test_false_partial_tag() {
        let mut filter = StreamingThinkFilter::new();

        // Delta 1: text that looks like start of a tag but isn't
        let a1 = filter.process("Hello <this is not a tag>");
        // After processing, "Hello " is emitted immediately. "<" is held.
        // Then "<this..." doesn't match <think>, so eventually emitted.
        // The partial_suffix_match checks suffix of pending vs prefix of "<think>"
        assert!(!filter.is_inside_think());

        // Flush to get everything
        let flush = filter.flush();
        // All text should have been emitted as visible
        let mut all_text = String::new();
        for action in a1.iter().chain(flush.iter()) {
            if let FilterAction::EmitText(t) = action {
                all_text.push_str(t);
            }
        }
        assert_eq!(all_text, "Hello <this is not a tag>");
    }

    #[test]
    fn test_multiple_think_blocks() {
        let mut filter = StreamingThinkFilter::new();
        let actions = filter.process("<think>first</think>middle<think>second</think>end");
        assert_eq!(
            actions,
            vec![
                FilterAction::EmitThinking("first".into()),
                FilterAction::EmitText("middle".into()),
                FilterAction::EmitThinking("second".into()),
                FilterAction::EmitText("end".into()),
            ]
        );
    }

    #[test]
    fn test_flush_outside_think() {
        let mut filter = StreamingThinkFilter::new();
        // Buffer a partial tag start
        let a1 = filter.process("text<th");
        assert_eq!(a1, vec![FilterAction::EmitText("text".into())]);

        // Flush — the partial tag is just text, not a real tag
        let flush = filter.flush();
        assert_eq!(flush, vec![FilterAction::EmitText("<th".into())]);
    }

    #[test]
    fn test_flush_inside_think() {
        let mut filter = StreamingThinkFilter::new();
        let a1 = filter.process("<think>unclosed thinking");
        assert_eq!(
            a1,
            vec![FilterAction::EmitThinking("unclosed thinking".into())]
        );

        // Stream ends without closing tag
        let flush = filter.flush();
        assert!(flush.is_empty()); // nothing left in pending
    }

    #[test]
    fn test_flush_inside_think_with_pending() {
        let mut filter = StreamingThinkFilter::new();
        let a1 = filter.process("<think>thinking</thi");
        assert_eq!(a1, vec![FilterAction::EmitThinking("thinking".into())]);
        assert!(filter.is_inside_think());

        // Stream ends with partial close tag buffered
        let flush = filter.flush();
        assert_eq!(flush, vec![FilterAction::EmitThinking("</thi".into())]);
    }

    #[test]
    fn test_empty_think_block() {
        let mut filter = StreamingThinkFilter::new();
        let actions = filter.process("<think></think>The answer.");
        assert_eq!(actions, vec![FilterAction::EmitText("The answer.".into())]);
    }

    #[test]
    fn test_only_think_block_no_visible_text() {
        let mut filter = StreamingThinkFilter::new();
        let actions = filter.process("<think>I need to reason carefully.</think>");
        assert_eq!(
            actions,
            vec![FilterAction::EmitThinking(
                "I need to reason carefully.".into()
            )]
        );
    }

    #[test]
    fn test_tag_split_across_many_deltas() {
        let mut filter = StreamingThinkFilter::new();

        // Split "<think>" across character-by-character deltas
        let a1 = filter.process("Hello ");
        assert_eq!(a1, vec![FilterAction::EmitText("Hello ".into())]);

        let a2 = filter.process("<");
        assert!(a2.is_empty()); // buffered

        let a3 = filter.process("t");
        assert!(a3.is_empty()); // still buffered

        let a4 = filter.process("h");
        assert!(a4.is_empty());

        let a5 = filter.process("i");
        assert!(a5.is_empty());

        let a6 = filter.process("n");
        assert!(a6.is_empty());

        let a7 = filter.process("k");
        assert!(a7.is_empty());

        let a8 = filter.process(">");
        // Now "<think>" is complete — we enter think mode, nothing to emit
        assert!(a8.is_empty());
        assert!(filter.is_inside_think());

        let a9 = filter.process("deep thought");
        assert_eq!(a9, vec![FilterAction::EmitThinking("deep thought".into())]);

        let a10 = filter.process("</think>done");
        assert_eq!(a10, vec![FilterAction::EmitText("done".into())]);
    }

    #[test]
    fn test_angle_bracket_not_tag() {
        let mut filter = StreamingThinkFilter::new();
        // Text with < that isn't a think tag
        let a1 = filter.process("a < b and c > d");
        let flush = filter.flush();

        let mut all_text = String::new();
        for action in a1.iter().chain(flush.iter()) {
            if let FilterAction::EmitText(t) = action {
                all_text.push_str(t);
            }
        }
        assert_eq!(all_text, "a < b and c > d");
    }

    #[test]
    fn test_partial_suffix_match_fn() {
        assert_eq!(partial_suffix_match("hello<", "<think>"), 1);
        assert_eq!(partial_suffix_match("hello<t", "<think>"), 2);
        assert_eq!(partial_suffix_match("hello<th", "<think>"), 3);
        assert_eq!(partial_suffix_match("hello<thi", "<think>"), 4);
        assert_eq!(partial_suffix_match("hello<thin", "<think>"), 5);
        assert_eq!(partial_suffix_match("hello<think", "<think>"), 6);
        // Full match is NOT a partial — the caller should use .find() for that
        assert_eq!(partial_suffix_match("hello<think>", "<think>"), 0);
        assert_eq!(partial_suffix_match("hello", "<think>"), 0);
        assert_eq!(partial_suffix_match("", "<think>"), 0);
    }

    #[test]
    fn test_close_tag_partial_suffix_match() {
        assert_eq!(partial_suffix_match("thinking</", "</think>"), 2);
        assert_eq!(partial_suffix_match("thinking</t", "</think>"), 3);
        assert_eq!(partial_suffix_match("thinking</th", "</think>"), 4);
    }

    #[test]
    fn test_interleaved_text_and_think() {
        let mut filter = StreamingThinkFilter::new();

        // Simulate realistic streaming: model sends text, then thinks, then more text
        let mut all_visible = String::new();
        let mut all_thinking = String::new();

        for delta in &[
            "The capital of France is ",
            "<think>The user is asking about geography. France",
            "'s capital is Paris, which I know for certain.</think>",
            "Paris. It is known as the City of Light.",
        ] {
            for action in filter.process(delta) {
                match action {
                    FilterAction::EmitText(t) => all_visible.push_str(&t),
                    FilterAction::EmitThinking(t) => all_thinking.push_str(&t),
                }
            }
        }
        for action in filter.flush() {
            match action {
                FilterAction::EmitText(t) => all_visible.push_str(&t),
                FilterAction::EmitThinking(t) => all_thinking.push_str(&t),
            }
        }

        assert_eq!(
            all_visible,
            "The capital of France is Paris. It is known as the City of Light."
        );
        assert!(all_thinking.contains("user is asking about geography"));
        assert!(all_thinking.contains("Paris, which I know for certain"));
    }
}
