//! Dream: semantic consolidation of a conversation session into user memory.
//!
//! Runs at session end (triggered by inactivity timeout). Takes all structured
//! extractions accumulated during compaction passes, plus the recent message tail,
//! and consolidates them into topic-organized user memory.
//!
//! Runs as a background task — does not block the next message.
//! Context-injection messages (calendar/email summaries) are excluded.
//!
//! ## Conflict resolution
//! The dream prompt includes the full content of existing memory topics so the LLM
//! can detect contradictions. It returns a `supersedes` list of topic names that
//! should be deleted before the new topics are written.
//!
//! ## Expiry
//! Time-sensitive topics (travel plans, temporary preferences) are tagged with an
//! optional `expires_at` ISO timestamp. Durable facts carry `null`.

use crate::compactor::{build_conversation_text, CompactionConfig, SessionExtraction};
use crate::llm_driver::{CompletionRequest, LlmDriver};
use openfang_types::agent::UserId;
use openfang_types::message::{ContentBlock, Message, MessageContent, MessageSource, Role};
use std::sync::Arc;
use tracing::{info, warn};

/// A topic-organized memory entry produced by dream.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DreamTopic {
    pub topic: String,
    pub summary: String, // one-liner for the index
    pub content: String, // full content for retrieval
    /// ISO 8601 timestamp after which this topic expires and is invisible at read time.
    /// Null for durable facts.
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Result of a dream pass.
#[derive(Debug, Clone)]
pub struct DreamResult {
    pub topics: Vec<DreamTopic>,
    pub user_id: UserId,
    /// Topic names from existing memory that are superseded by this dream pass
    /// and should be deleted before the new topics are written.
    pub superseded_topics: Vec<String>,
}

/// Run the dream pass: consolidate extractions + recent messages into topic memory.
///
/// - `extractions`: all SessionExtraction records from compaction during this session
/// - `recent_messages`: recent tail messages kept verbatim (already filtered)
/// - `user_id`: whose memory this belongs to
/// - `existing_topics`: full existing user memory `(topic_name, content)` pairs for
///   conflict detection. Dream may supersede stale entries.
pub async fn dream(
    driver: Arc<dyn LlmDriver>,
    model: &str,
    extractions: &[SessionExtraction],
    recent_messages: &[Message],
    user_id: UserId,
    existing_topics: &[(String, String)],
    config: &CompactionConfig,
) -> Result<DreamResult, String> {
    // Filter recent_messages to exclude ContextInjection
    let filtered_recent: Vec<Message> = recent_messages
        .iter()
        .filter(|m| m.source != Some(MessageSource::ContextInjection))
        .cloned()
        .collect();

    let recent_conversation = build_conversation_text(&filtered_recent, config);

    // Merge all extractions into consolidated lists
    let all_facts: Vec<String> = extractions.iter().flat_map(|e| e.facts.clone()).collect();
    let all_preferences: Vec<String> = extractions
        .iter()
        .flat_map(|e| e.preferences.clone())
        .collect();
    let all_decisions: Vec<String> = extractions
        .iter()
        .flat_map(|e| e.decisions.clone())
        .collect();
    let all_tasks: Vec<String> = extractions.iter().flat_map(|e| e.tasks.clone()).collect();
    let all_open_items: Vec<String> = extractions
        .iter()
        .flat_map(|e| e.open_items.clone())
        .collect();

    let fmt_list = |items: &[String]| -> String {
        if items.is_empty() {
            "(none)".to_string()
        } else {
            items.join("; ")
        }
    };

    // Build existing topics section with full content for conflict detection
    let existing_topics_str = if existing_topics.is_empty() {
        "(none)".to_string()
    } else {
        existing_topics
            .iter()
            .map(|(name, content)| format!("  [{name}]: {content}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let prompt = format!(
        "You are consolidating a conversation into persistent user memory organized by topic.\n\n\
         Accumulated insights from this session:\n\
         Facts: {facts}\n\
         Preferences: {prefs}\n\
         Decisions: {decisions}\n\
         Tasks completed: {tasks}\n\
         Open items: {open}\n\n\
         Recent conversation:\n\
         {recent}\n\n\
         Existing memory topics (full content — check for contradictions):\n\
         {existing}\n\n\
         Instructions:\n\
         1. Produce 3-7 named memory topics that reflect the complete, current state of the user's memory.\n\
         2. If new information contradicts an existing topic, the new information wins. List the old \
            topic name in `supersedes`.\n\
         3. For time-sensitive facts (travel plans, temporary preferences, deadlines), set `expires_at` \
            to an ISO 8601 timestamp (e.g. \"2026-04-10T00:00:00Z\"). For durable facts set `expires_at` to null.\n\
         4. Topic names must be snake_case. Good examples: work_context, preferences, open_items, \
            family_context, upcoming_travel.\n\n\
         Return ONLY valid JSON:\n\
         {{\n\
           \"supersedes\": [\"old_topic_name_if_any\"],\n\
           \"topics\": [\n\
             {{\n\
               \"topic\": \"snake_case_name\",\n\
               \"summary\": \"one line description of what this topic contains\",\n\
               \"content\": \"full content for this topic, 2-10 sentences\",\n\
               \"expires_at\": null\n\
             }}\n\
           ]\n\
         }}",
        facts = fmt_list(&all_facts),
        prefs = fmt_list(&all_preferences),
        decisions = fmt_list(&all_decisions),
        tasks = fmt_list(&all_tasks),
        open = fmt_list(&all_open_items),
        recent = recent_conversation,
        existing = existing_topics_str,
    );

    let request = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Blocks(vec![ContentBlock::Text {
                text: prompt,
                provider_metadata: None,
            }]),
            source: None,
            ..Default::default()
        }],
        tools: vec![],
        max_tokens: config.max_summary_tokens * 2, // dreamer can produce more content
        temperature: 0.3,
        system: Some(
            "You are a memory consolidation assistant. Organize conversation insights into \
             structured topics with conflict resolution and expiry tagging. \
             Return ONLY valid JSON. Do not include any text outside the JSON object."
                .to_string(),
        ),
        thinking: None,
    };

    for attempt in 0..config.max_retries {
        match driver.complete(request.clone()).await {
            Ok(response) => {
                let text = response.text();
                if text.is_empty() {
                    warn!(attempt, "Empty response from LLM for dream pass");
                    continue;
                }
                // Strip markdown code fences if present
                let json_str = text
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                match parse_dream_response(json_str) {
                    Ok((topics, superseded)) => {
                        info!(
                            topics = topics.len(),
                            superseded = superseded.len(),
                            "Dream pass complete"
                        );
                        return Ok(DreamResult {
                            topics,
                            user_id,
                            superseded_topics: superseded,
                        });
                    }
                    Err(e) => {
                        warn!(attempt, error = %e, "Failed to parse dream response JSON, retrying");
                    }
                }
            }
            Err(e) => {
                warn!(attempt, error = %e, "LLM call failed during dream pass");
            }
        }
    }

    // Fallback: produce a single "general" topic from concatenated extractions
    warn!("Dream pass failed after all retries, falling back to general topic");
    let general_content = build_fallback_content(
        &all_facts,
        &all_preferences,
        &all_decisions,
        &all_tasks,
        &all_open_items,
    );
    let fallback_topic = DreamTopic {
        topic: "general".to_string(),
        summary: "General session memory (dream consolidation unavailable)".to_string(),
        content: general_content,
        expires_at: None,
    };
    Ok(DreamResult {
        topics: vec![fallback_topic],
        user_id,
        superseded_topics: vec![],
    })
}

/// Parse the LLM response JSON into (topics, superseded_topics).
fn parse_dream_response(json_str: &str) -> Result<(Vec<DreamTopic>, Vec<String>), String> {
    #[derive(serde::Deserialize)]
    struct DreamResponse {
        #[serde(default)]
        supersedes: Vec<String>,
        topics: Vec<DreamTopic>,
    }
    serde_json::from_str::<DreamResponse>(json_str)
        .map(|r| (r.topics, r.supersedes))
        .map_err(|e| e.to_string())
}

/// Build a fallback "general" topic content from all extraction lists.
fn build_fallback_content(
    facts: &[String],
    preferences: &[String],
    decisions: &[String],
    tasks: &[String],
    open_items: &[String],
) -> String {
    let mut parts = Vec::new();
    if !facts.is_empty() {
        parts.push(format!("Facts: {}", facts.join("; ")));
    }
    if !preferences.is_empty() {
        parts.push(format!("Preferences: {}", preferences.join("; ")));
    }
    if !decisions.is_empty() {
        parts.push(format!("Decisions: {}", decisions.join("; ")));
    }
    if !tasks.is_empty() {
        parts.push(format!("Tasks: {}", tasks.join("; ")));
    }
    if !open_items.is_empty() {
        parts.push(format!("Open items: {}", open_items.join("; ")));
    }
    if parts.is_empty() {
        "No structured information extracted from this session.".to_string()
    } else {
        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compactor::CompactionConfig;
    use crate::llm_driver::{CompletionResponse, LlmError};
    use async_trait::async_trait;
    use openfang_types::message::{StopReason, TokenUsage};
    use std::sync::Arc;

    struct MockDreamDriver {
        response: String,
    }

    #[async_trait]
    impl LlmDriver for MockDreamDriver {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: self.response.clone(),
                    provider_metadata: None,
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: vec![],
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                },
            })
        }
    }

    #[test]
    fn test_dream_filters_context_injections() {
        let messages = [
            Message::user("What is 2+2?"),
            Message::context_injection("Calendar: meeting at 3pm"),
            Message::assistant("It's 4."),
            Message::context_injection("Email: 3 unread"),
        ];

        let filtered: Vec<Message> = messages
            .iter()
            .filter(|m| m.source != Some(MessageSource::ContextInjection))
            .cloned()
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered[0].content.text_content().contains("2+2"));
        assert!(filtered[1].content.text_content().contains("It's 4"));
    }

    #[tokio::test]
    async fn test_dream_result_has_topics() {
        let valid_response = r#"{
            "supersedes": [],
            "topics": [
                {
                    "topic": "preferences",
                    "summary": "User interface and work preferences",
                    "content": "The user prefers dark mode. They like concise responses.",
                    "expires_at": null
                },
                {
                    "topic": "open_items",
                    "summary": "Pending tasks and questions",
                    "content": "There is a pending code review. The user wants to set up CI/CD.",
                    "expires_at": null
                }
            ]
        }"#;

        let driver = Arc::new(MockDreamDriver {
            response: valid_response.to_string(),
        });

        let extractions = vec![SessionExtraction {
            preferences: vec!["dark mode".to_string()],
            open_items: vec!["code review pending".to_string()],
            ..SessionExtraction::default()
        }];

        let recent = vec![Message::user("Let's set up CI/CD next.")];
        let user_id = UserId::new();
        let config = CompactionConfig::default();

        let result = dream(
            driver,
            "test-model",
            &extractions,
            &recent,
            user_id,
            &[("preferences".to_string(), "User prefers tea".to_string())],
            &config,
        )
        .await
        .unwrap();

        assert_eq!(result.topics.len(), 2);
        assert_eq!(result.topics[0].topic, "preferences");
        assert_eq!(result.topics[1].topic, "open_items");
        assert_eq!(result.user_id, user_id);
        assert!(result.superseded_topics.is_empty());
    }

    #[tokio::test]
    async fn test_dream_conflict_resolution() {
        let response_with_supersedes = r#"{
            "supersedes": ["old_preferences"],
            "topics": [
                {
                    "topic": "preferences",
                    "summary": "Updated preferences",
                    "content": "User now prefers coffee (changed from tea).",
                    "expires_at": null
                }
            ]
        }"#;

        let driver = Arc::new(MockDreamDriver {
            response: response_with_supersedes.to_string(),
        });

        let user_id = UserId::new();
        let result = dream(
            driver,
            "test-model",
            &[],
            &[],
            user_id,
            &[(
                "old_preferences".to_string(),
                "User prefers tea".to_string(),
            )],
            &CompactionConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(
            result.superseded_topics,
            vec!["old_preferences".to_string()]
        );
        assert_eq!(result.topics.len(), 1);
        assert_eq!(result.topics[0].topic, "preferences");
    }

    #[tokio::test]
    async fn test_dream_expiry_tagging() {
        let response_with_expiry = r#"{
            "supersedes": [],
            "topics": [
                {
                    "topic": "upcoming_travel",
                    "summary": "Paris trip",
                    "content": "Flying to Paris on April 10.",
                    "expires_at": "2026-04-11T00:00:00Z"
                },
                {
                    "topic": "preferences",
                    "summary": "Durable preferences",
                    "content": "Prefers concise answers.",
                    "expires_at": null
                }
            ]
        }"#;

        let driver = Arc::new(MockDreamDriver {
            response: response_with_expiry.to_string(),
        });

        let user_id = UserId::new();
        let result = dream(
            driver,
            "test-model",
            &[],
            &[],
            user_id,
            &[],
            &CompactionConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(result.topics.len(), 2);
        assert!(result.topics[0].expires_at.is_some());
        assert!(result.topics[1].expires_at.is_none());
    }

    #[tokio::test]
    async fn test_dream_fallback_on_parse_error() {
        let driver = Arc::new(MockDreamDriver {
            response: "this is not valid json at all!!!".to_string(),
        });

        let extractions = vec![SessionExtraction {
            facts: vec!["User uses Rust".to_string()],
            ..SessionExtraction::default()
        }];

        let config = CompactionConfig {
            max_retries: 2,
            ..CompactionConfig::default()
        };

        let user_id = UserId::new();
        let result = dream(
            driver,
            "test-model",
            &extractions,
            &[],
            user_id,
            &[],
            &config,
        )
        .await
        .unwrap();

        assert_eq!(result.topics.len(), 1);
        assert_eq!(result.topics[0].topic, "general");
        assert!(result.topics[0].content.contains("User uses Rust"));
        assert!(result.superseded_topics.is_empty());
    }
}
