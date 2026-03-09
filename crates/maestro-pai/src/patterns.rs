//! Pattern synthesis — mine accumulated learnings for recurring patterns.

use crate::{LearningEntry, Pattern};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Synthesizes patterns from a collection of learning entries.
///
/// This is a heuristic implementation. In production, the synthesis
/// step would call an LLM with the learnings as context.
pub struct PatternSynthesizer;

impl PatternSynthesizer {
    /// Group learnings by category and find recurring themes.
    pub fn synthesize(learnings: &[LearningEntry]) -> Vec<Pattern> {
        let mut by_category: HashMap<&str, Vec<&LearningEntry>> = HashMap::new();
        for l in learnings {
            by_category.entry(&l.category).or_default().push(l);
        }

        let mut patterns = Vec::new();
        for (category, entries) in &by_category {
            if entries.len() < 2 { continue; }

            // Simple heuristic: if multiple entries share keywords, that's a pattern
            let keyword_counts = Self::count_keywords(entries);
            for (keyword, count) in keyword_counts {
                if count >= 2 {
                    let confidence = (count as f64 / entries.len() as f64).min(1.0);
                    let source_ids: Vec<Uuid> = entries.iter()
                        .filter(|e| e.insight.to_lowercase().contains(&keyword.to_lowercase()))
                        .map(|e| e.id)
                        .collect();
                    patterns.push(Pattern {
                        id: Uuid::new_v4(),
                        name: format!("{}: {}", category, keyword),
                        description: format!(
                            "Recurring theme '{}' observed {} times in {} learnings",
                            keyword, count, entries.len()
                        ),
                        source_learning_ids: source_ids,
                        occurrence_count: count as u32,
                        confidence,
                        suggested_action: format!(
                            "Review {} learnings mentioning '{}' and consider updating the algorithm",
                            category, keyword
                        ),
                        discovered_at: Utc::now(),
                    });
                }
            }
        }
        patterns
    }

    fn count_keywords(entries: &[&LearningEntry]) -> HashMap<String, usize> {
        let stop_words = ["the", "a", "an", "is", "in", "on", "at", "to", "for", "of", "and", "or", "with", "this", "that"];
        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            let words: Vec<String> = entry.insight
                .split_whitespace()
                .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                .filter(|w| w.len() > 4 && !stop_words.contains(&w.as_str()))
                .collect();
            for word in words {
                *counts.entry(word).or_insert(0) += 1;
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_synthesis_empty() {
        let patterns = PatternSynthesizer::synthesize(&[]);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_pattern_synthesis_finds_recurring() {
        let entries = vec![
            LearningEntry {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                task_id: "t1".to_string(),
                category: "ALGORITHM".to_string(),
                insight: "parallel execution improves throughput significantly".to_string(),
                context: "ctx".to_string(),
                actionable: true,
                user_rating: None,
                tags: vec![],
            },
            LearningEntry {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                task_id: "t2".to_string(),
                category: "ALGORITHM".to_string(),
                insight: "parallel execution reduces latency for independent tasks".to_string(),
                context: "ctx".to_string(),
                actionable: true,
                user_rating: None,
                tags: vec![],
            },
        ];
        let patterns = PatternSynthesizer::synthesize(&entries);
        // Should find "parallel" and "execution" as recurring keywords
        assert!(!patterns.is_empty());
        let has_parallel = patterns.iter().any(|p| p.name.contains("parallel") || p.name.contains("execution"));
        assert!(has_parallel);
    }
}
