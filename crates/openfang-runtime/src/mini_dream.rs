//! Mini-dream: extract and persist facts from messages being trimmed during overflow recovery.
//!
//! When the context window overflows and old messages must be dropped, this runs a
//! dream pass on the messages about to be removed so their content is preserved in
//! user memory before they are discarded — replacing lossy truncation with semantic
//! preservation.
//!
//! Called by the agent loop immediately before `recover_from_overflow()` trims the
//! context. Non-fatal: errors are logged but never propagated.

use crate::compactor::{extract_structured, CompactionConfig};
use crate::dreamer::dream;
use crate::embedding::EmbeddingDriver;
use crate::llm_driver::LlmDriver;
use openfang_memory::user_memory::MemoryTopic;
use openfang_memory::MemorySubstrate;
use openfang_types::agent::UserId;
use openfang_types::message::Message;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Extract facts from `messages` (which are about to be trimmed) and persist them
/// to user memory. Non-fatal: errors are logged, never propagated.
///
/// `embedding_driver` is optional — when provided, topic embeddings are stored for
/// semantic retrieval.
///
/// Returns the number of memory topics written.
pub async fn run_mini_dream(
    messages: &[Message],
    user_id: UserId,
    driver: Arc<dyn LlmDriver>,
    model: &str,
    memory: &MemorySubstrate,
    config: &CompactionConfig,
    embedding_driver: Option<&(dyn EmbeddingDriver + Send + Sync)>,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    debug!(
        user_id = %user_id,
        messages = messages.len(),
        "Mini-dream: extracting facts from messages about to be trimmed"
    );

    // Step 1: Structured extraction from the messages being trimmed.
    // `extract_structured` is infallible — internal LLM/parse failures fall back
    // to an empty extraction, which the emptiness check below skips.
    let extraction = extract_structured(driver.clone(), model, messages, None, config).await;

    // Nothing worth persisting — skip the dream LLM call
    if extraction.facts.is_empty()
        && extraction.preferences.is_empty()
        && extraction.decisions.is_empty()
        && extraction.tasks.is_empty()
        && extraction.open_items.is_empty()
    {
        debug!(user_id = %user_id, "Mini-dream: no facts extracted, skipping dream pass");
        return 0;
    }

    // Step 2: Load existing topics for conflict detection / supersedes
    let existing_topics: Vec<(String, String)> = memory
        .user_topic_index(user_id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            memory
                .user_topic(user_id, &entry.topic)
                .ok()
                .flatten()
                .map(|t| (entry.topic, t.content))
        })
        .collect();

    // Step 3: Dream pass — consolidate extraction into topic-organized memory
    let result = match dream(
        driver,
        model,
        &[extraction],
        messages,
        user_id,
        &existing_topics,
        config,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Mini-dream: dream pass failed: {e}");
            return 0;
        }
    };

    // Step 4: Delete superseded topics, then write new ones
    for superseded in &result.superseded_topics {
        if let Err(e) = memory.delete_user_topic(user_id, superseded) {
            warn!("Mini-dream: failed to delete superseded topic '{superseded}': {e}");
        }
    }

    let now = chrono::Utc::now();
    let mut persisted = 0usize;
    for dt in &result.topics {
        let expires_at = dt
            .expires_at
            .as_deref()
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
        let topic = MemoryTopic {
            user_id,
            topic: dt.topic.clone(),
            summary: dt.summary.clone(),
            content: dt.content.clone(),
            updated_at: now,
            expires_at,
        };
        match memory.upsert_user_topic(&topic) {
            Ok(()) => {
                persisted += 1;
                // Embed the topic content for semantic retrieval.
                if let Some(emb) = embedding_driver {
                    match emb.embed_one(&dt.content).await {
                        Ok(vec) => {
                            if let Err(e) =
                                memory.store_user_topic_embedding(user_id, &dt.topic, &vec)
                            {
                                warn!(
                                    "Mini-dream: failed to store embedding for '{}': {e}",
                                    dt.topic
                                );
                            }
                        }
                        Err(e) => warn!("Mini-dream: embedding failed for '{}': {e}", dt.topic),
                    }
                }
            }
            Err(e) => warn!("Mini-dream: failed to persist topic '{}': {e}", dt.topic),
        }
    }

    info!(
        user_id = %user_id,
        topics = persisted,
        superseded = result.superseded_topics.len(),
        "Mini-dream: facts preserved from trimmed messages"
    );
    persisted
}

#[cfg(test)]
mod tests {
    use openfang_types::agent::{AgentManifest, MemoryConfig, MemorySystem};

    /// Behavioural gate: the agent loop must only call into `run_mini_dream`
    /// when the agent's manifest opts in to structured memory.
    ///
    /// This test asserts the gate predicate directly — the call sites in
    /// `agent_loop.rs` are `if manifest.memory.is_structured() { ... }`,
    /// so a `false` here proves the extraction path is skipped, and a
    /// `true` here proves it runs.
    #[test]
    fn test_structured_memory_gate_skips_default_agents() {
        // Default agent — no [memory] block in TOML.
        let default_agent = AgentManifest::default();
        assert!(
            !default_agent.memory.is_structured(),
            "default agents must NOT trigger structured extraction / dreamer"
        );
        assert_eq!(default_agent.memory.system, MemorySystem::Summarization);
    }

    #[test]
    fn test_structured_memory_gate_fires_for_opted_in_agents() {
        // Agent that opts in to structured memory.
        let opted_in = AgentManifest {
            memory: MemoryConfig {
                system: MemorySystem::Structured,
            },
            ..AgentManifest::default()
        };
        assert!(
            opted_in.memory.is_structured(),
            "agents with memory.system = structured MUST trigger structured extraction / dreamer"
        );
    }
}
