//! Core types and traits for the OpenFang Agent Operating System.
//!
//! This crate defines all shared data structures used across the OpenFang kernel,
//! runtime, memory substrate, and wire protocol. It contains no business logic.

pub mod agent;
pub mod approval;
pub mod capability;
pub mod comms;
pub mod config;
pub mod error;
pub mod event;
pub mod manifest_signing;
pub mod media;
pub mod memory;
pub mod message;
pub mod model_catalog;
pub mod scheduler;
pub mod serde_compat;
pub mod taint;
pub mod tool;
pub mod tool_compat;
pub mod webhook;

/// Safely truncate a string to at most `max_bytes`, never splitting a UTF-8 char.
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn truncate_str_chinese() {
        // Each Chinese character is 3 bytes
        let s = "\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 你好世界
        assert_eq!(truncate_str(s, 6), "\u{4F60}\u{597D}"); // 你好
        assert_eq!(truncate_str(s, 7), "\u{4F60}\u{597D}"); // still 你好 (7 is mid-char)
        assert_eq!(truncate_str(s, 9), "\u{4F60}\u{597D}\u{4E16}"); // 你好世
    }

    #[test]
    fn truncate_str_emoji() {
        let s = "hi\u{1F600}there"; // hi😀there — emoji is 4 bytes
        assert_eq!(truncate_str(s, 3), "hi"); // 3 is mid-emoji
        assert_eq!(truncate_str(s, 6), "hi\u{1F600}"); // after emoji
    }

    #[test]
    fn truncate_str_no_truncation() {
        assert_eq!(truncate_str("short", 100), "short");
    }

    #[test]
    fn truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }
}
