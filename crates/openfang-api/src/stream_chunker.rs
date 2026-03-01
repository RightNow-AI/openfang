//! Markdown-aware stream chunking.
//!
//! Replaces naive 200-char text buffer flushing with smart chunking that
//! never splits inside fenced code blocks and respects Markdown structure.

/// Markdown-aware stream chunker.
///
/// Buffers incoming text and flushes at natural break points:
/// paragraph boundaries > newlines > sentence endings.
/// Never splits inside fenced code blocks.
pub struct StreamChunker {
    buffer: String,
    in_code_fence: bool,
    fence_marker: String,
    min_chunk_chars: usize,
    max_chunk_chars: usize,
}

impl StreamChunker {
    /// Create a new chunker with custom min/max thresholds.
    pub fn new(min_chunk_chars: usize, max_chunk_chars: usize) -> Self {
        Self {
            buffer: String::new(),
            in_code_fence: false,
            fence_marker: String::new(),
            min_chunk_chars,
            max_chunk_chars,
        }
    }

    /// Push new text into the buffer. Updates code fence tracking.
    pub fn push(&mut self, text: &str) {
        for line in text.split_inclusive('\n') {
            self.buffer.push_str(line);
            // Track code fence state
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if self.in_code_fence {
                    // Check if this closes the current fence
                    if trimmed == "```" || trimmed.starts_with(&self.fence_marker) {
                        self.in_code_fence = false;
                        self.fence_marker.clear();
                    }
                } else {
                    self.in_code_fence = true;
                    self.fence_marker = "```".to_string();
                }
            }
        }
    }

    /// Try to flush a chunk from the buffer.
    ///
    /// Returns `Some(chunk)` if enough content has accumulated,
    /// `None` if we should wait for more input.
    pub fn try_flush(&mut self) -> Option<String> {
        if self.buffer.len() < self.min_chunk_chars {
            return None;
        }

        // If inside a code fence and under max, wait for fence to close
        if self.in_code_fence && self.buffer.len() < self.max_chunk_chars {
            return None;
        }

        // If at max inside a fence, force-close and flush
        if self.in_code_fence && self.buffer.len() >= self.max_chunk_chars {
            // Close the fence, flush everything, reopen on next push
            let mut chunk = std::mem::take(&mut self.buffer);
            chunk.push_str("\n```\n");
            // Mark that we need to reopen the fence
            self.buffer = format!("```{}\n", self.fence_marker.trim_start_matches('`'));
            return Some(chunk);
        }

        // Find best break point
        let search_range = self.min_chunk_chars..self.buffer.len().min(self.max_chunk_chars);

        // Priority 1: Paragraph break (double newline)
        if let Some(pos) = find_last_in_range(&self.buffer, "\n\n", &search_range) {
            let break_at = pos + 2;
            let chunk = self.buffer[..break_at].to_string();
            self.buffer = self.buffer[break_at..].to_string();
            return Some(chunk);
        }

        // Priority 2: Single newline
        if let Some(pos) = find_last_in_range(&self.buffer, "\n", &search_range) {
            let break_at = pos + 1;
            let chunk = self.buffer[..break_at].to_string();
            self.buffer = self.buffer[break_at..].to_string();
            return Some(chunk);
        }

        // Priority 3: Sentence ending (". ", "! ", "? ")
        for ending in &[". ", "! ", "? "] {
            if let Some(pos) = find_last_in_range(&self.buffer, ending, &search_range) {
                let break_at = pos + ending.len();
                let chunk = self.buffer[..break_at].to_string();
                self.buffer = self.buffer[break_at..].to_string();
                return Some(chunk);
            }
        }

        // Priority 4: Forced break at max_chunk_chars (char-boundary safe)
        if self.buffer.len() >= self.max_chunk_chars {
            let mut break_at = self.max_chunk_chars;
            while break_at > 0 && !self.buffer.is_char_boundary(break_at) {
                break_at -= 1;
            }
            if break_at == 0 {
                break_at = self.buffer.len();
            }
            let chunk = self.buffer[..break_at].to_string();
            self.buffer = self.buffer[break_at..].to_string();
            return Some(chunk);
        }

        None
    }

    /// Force-flush all remaining text.
    pub fn flush_remaining(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }

    /// Current buffer length.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether currently inside a code fence.
    pub fn is_in_code_fence(&self) -> bool {
        self.in_code_fence
    }
}

/// Find the last occurrence of a pattern within a byte range.
fn find_last_in_range(text: &str, pattern: &str, range: &std::ops::Range<usize>) -> Option<usize> {
    let search_text = &text[range.start..range.end.min(text.len())];
    search_text.rfind(pattern).map(|pos| range.start + pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_chunking() {
        let mut chunker = StreamChunker::new(10, 50);
        chunker.push("Hello world.\nThis is a test.\nAnother line.\n");

        let chunk = chunker.try_flush();
        assert!(chunk.is_some());
        let text = chunk.unwrap();
        // Should break at a newline
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn test_code_fence_not_split() {
        let mut chunker = StreamChunker::new(5, 200);
        chunker.push("Before\n```python\ndef foo():\n    pass\n```\nAfter\n");

        // Should not flush mid-fence
        // Since buffer is >5 chars and fence is now closed, should flush
        let chunk = chunker.try_flush();
        assert!(chunk.is_some());
        let text = chunk.unwrap();
        // If it includes the code block, the fence should be complete
        if text.contains("```python") {
            assert!(text.contains("```\n") || text.ends_with("```"));
        }
    }

    #[test]
    fn test_code_fence_force_close_at_max() {
        let mut chunker = StreamChunker::new(5, 30);
        chunker.push("```python\nline1\nline2\nline3\nline4\nline5\nline6\n");

        // Buffer exceeds max while in fence — should force close
        let chunk = chunker.try_flush();
        assert!(chunk.is_some());
        let text = chunk.unwrap();
        assert!(text.contains("```\n")); // force-closed fence
    }

    #[test]
    fn test_paragraph_break_priority() {
        let mut chunker = StreamChunker::new(10, 200);
        chunker.push("First paragraph text.\n\nSecond paragraph text.\n");

        let chunk = chunker.try_flush();
        assert!(chunk.is_some());
        let text = chunk.unwrap();
        assert!(text.ends_with("\n\n"));
    }

    #[test]
    fn test_flush_remaining() {
        let mut chunker = StreamChunker::new(100, 200);
        chunker.push("short");

        // try_flush should return None (under min)
        assert!(chunker.try_flush().is_none());

        // flush_remaining should return everything
        let remaining = chunker.flush_remaining();
        assert_eq!(remaining, Some("short".to_string()));

        // Second flush should be None
        assert!(chunker.flush_remaining().is_none());
    }

    #[test]
    fn test_sentence_break() {
        let mut chunker = StreamChunker::new(10, 200);
        chunker.push("This is the first sentence. This is the second sentence. More text here.");

        let chunk = chunker.try_flush();
        assert!(chunk.is_some());
        let text = chunk.unwrap();
        // Should break at a sentence ending
        assert!(text.ends_with(". ") || text.ends_with(".\n"));
    }
}
