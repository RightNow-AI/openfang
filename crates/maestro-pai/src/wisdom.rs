//! Wisdom frames — crystallized domain knowledge for future task retrieval.

use crate::{Pattern, WisdomFrame};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Manages wisdom frames — crystallized knowledge derived from patterns.
pub struct WisdomStore {
    frames: HashMap<Uuid, WisdomFrame>,
}

impl WisdomStore {
    pub fn new() -> Self {
        Self { frames: HashMap::new() }
    }

    /// Add a wisdom frame.
    pub fn add(&mut self, frame: WisdomFrame) {
        self.frames.insert(frame.id, frame);
    }

    /// Create a wisdom frame from a pattern.
    pub fn from_pattern(pattern: &Pattern, domain: &str) -> WisdomFrame {
        WisdomFrame {
            id: Uuid::new_v4(),
            domain: domain.to_string(),
            title: pattern.name.clone(),
            content: format!(
                "## Pattern: {}\n\n{}\n\n**Suggested Action:** {}",
                pattern.name, pattern.description, pattern.suggested_action
            ),
            source_pattern_ids: vec![pattern.id],
            created_at: Utc::now(),
            last_applied_at: None,
            application_count: 0,
        }
    }

    /// Search wisdom frames by domain.
    pub fn by_domain(&self, domain: &str) -> Vec<&WisdomFrame> {
        self.frames.values().filter(|f| f.domain == domain).collect()
    }

    /// Search wisdom frames by keyword.
    pub fn search(&self, query: &str) -> Vec<&WisdomFrame> {
        let q = query.to_lowercase();
        self.frames.values()
            .filter(|f| f.title.to_lowercase().contains(&q) || f.content.to_lowercase().contains(&q))
            .collect()
    }

    /// Mark a wisdom frame as applied.
    pub fn mark_applied(&mut self, id: Uuid) {
        if let Some(frame) = self.frames.get_mut(&id) {
            frame.last_applied_at = Some(Utc::now());
            frame.application_count += 1;
        }
    }

    /// Get the most frequently applied wisdom frames.
    pub fn top_applied(&self, n: usize) -> Vec<&WisdomFrame> {
        let mut frames: Vec<&WisdomFrame> = self.frames.values().collect();
        frames.sort_by(|a, b| b.application_count.cmp(&a.application_count));
        frames.into_iter().take(n).collect()
    }

    pub fn len(&self) -> usize { self.frames.len() }
    pub fn is_empty(&self) -> bool { self.frames.is_empty() }
}

impl Default for WisdomStore {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(name: &str) -> Pattern {
        Pattern {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: format!("Pattern about {}", name),
            source_learning_ids: vec![],
            occurrence_count: 3,
            confidence: 0.8,
            suggested_action: "Review and apply".to_string(),
            discovered_at: Utc::now(),
        }
    }

    #[test]
    fn test_add_and_search() {
        let mut store = WisdomStore::new();
        let p = make_pattern("parallel execution");
        let frame = WisdomStore::from_pattern(&p, "performance");
        let frame_id = frame.id;
        store.add(frame);

        let results = store.search("parallel");
        assert_eq!(results.len(), 1);

        let by_domain = store.by_domain("performance");
        assert_eq!(by_domain.len(), 1);
    }

    #[test]
    fn test_mark_applied() {
        let mut store = WisdomStore::new();
        let p = make_pattern("caching");
        let frame = WisdomStore::from_pattern(&p, "performance");
        let id = frame.id;
        store.add(frame);

        store.mark_applied(id);
        store.mark_applied(id);

        let frame = store.frames.get(&id).unwrap();
        assert_eq!(frame.application_count, 2);
        assert!(frame.last_applied_at.is_some());
    }

    #[test]
    fn test_top_applied() {
        let mut store = WisdomStore::new();
        for i in 0..5 {
            let p = make_pattern(&format!("pattern-{}", i));
            let frame = WisdomStore::from_pattern(&p, "domain");
            let id = frame.id;
            store.add(frame);
            for _ in 0..i {
                store.mark_applied(id);
            }
        }
        let top = store.top_applied(3);
        assert_eq!(top.len(), 3);
        assert!(top[0].application_count >= top[1].application_count);
    }
}
