//! A2A (Agent-to-Agent) Protocol — cross-framework agent interoperability.
//!
//! Google's A2A protocol enables cross-framework agent interoperability via
//! **Agent Cards** (JSON capability manifests) and **Task-based coordination**.
//!
//! This module provides:
//! - `AgentCard` — describes an agent's capabilities to external systems
//! - `A2aTask` — unit of work exchanged between agents
//! - `build_agent_card` — expose OpenFang agents via A2A
//! - `A2aClient` — discover and interact with external A2A agents

use futures::StreamExt;
use openfang_types::agent::AgentManifest;
use reqwest::Response;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Wall-clock deadline applied to the synchronous SSE streaming path
/// ([`A2aClient::send_task_streaming`]). Must match the contract advertised
/// in the `a2a_send` tool description ("blocks until complete, up to 300 s").
/// The async dispatch path ([`A2aClient::send_task_streaming_with_progress`])
/// intentionally does NOT apply this deadline — see its doc comment.
pub(crate) const SYNC_STREAMING_DEADLINE: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// A2A Agent Card
// ---------------------------------------------------------------------------

/// A2A Agent Card — describes an agent's capabilities to external systems.
///
/// Served at `/.well-known/agent.json` per the A2A specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    /// Agent display name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Agent endpoint URL.
    pub url: String,
    /// Protocol version.
    pub version: String,
    /// Agent capabilities.
    pub capabilities: AgentCapabilities,
    /// Skills this agent can perform (A2A skill descriptors, not OpenFang skills).
    pub skills: Vec<AgentSkill>,
    /// Supported input content types.
    #[serde(default)]
    pub default_input_modes: Vec<String>,
    /// Supported output content types.
    #[serde(default)]
    pub default_output_modes: Vec<String>,
}

/// A2A agent capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Whether this agent supports streaming responses.
    pub streaming: bool,
    /// Whether this agent supports push notifications.
    pub push_notifications: bool,
    /// Whether task status history is available.
    pub state_transition_history: bool,
}

/// A2A skill descriptor (not an OpenFang skill — describes a capability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    /// Unique skill identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description of what this skill does.
    pub description: String,
    /// Tags for discovery.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Example prompts that trigger this skill.
    #[serde(default)]
    pub examples: Vec<String>,
}

// ---------------------------------------------------------------------------
// A2A Task
// ---------------------------------------------------------------------------

/// A2A Task — unit of work exchanged between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aTask {
    /// Unique task identifier.
    pub id: String,
    /// Optional session identifier for conversation continuity.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Current task status (accepts both string and object forms).
    pub status: A2aTaskStatusWrapper,
    /// Messages exchanged during the task.
    #[serde(default)]
    pub messages: Vec<A2aMessage>,
    /// Artifacts produced by the task.
    #[serde(default)]
    pub artifacts: Vec<A2aArtifact>,
}

/// A2A task status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum A2aTaskStatus {
    /// Task has been received but not started.
    Submitted,
    /// Task is being processed.
    Working,
    /// Agent needs more input from the caller.
    InputRequired,
    /// Task completed successfully.
    Completed,
    /// Task was cancelled.
    Cancelled,
    /// Task failed.
    Failed,
}

/// Wrapper that accepts either a bare status string (`"completed"`)
/// or the object form (`{"state": "completed", "message": null}`)
/// used by some A2A implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2aTaskStatusWrapper {
    /// Object form: `{"state": "completed", "message": ...}`.
    Object {
        state: A2aTaskStatus,
        #[serde(default)]
        message: Option<serde_json::Value>,
    },
    /// Bare enum form: `"completed"`.
    Enum(A2aTaskStatus),
}

impl A2aTaskStatusWrapper {
    /// Extract the underlying `A2aTaskStatus` regardless of encoding form.
    pub fn state(&self) -> &A2aTaskStatus {
        match self {
            Self::Object { state, .. } => state,
            Self::Enum(s) => s,
        }
    }
}

impl From<A2aTaskStatus> for A2aTaskStatusWrapper {
    fn from(status: A2aTaskStatus) -> Self {
        Self::Enum(status)
    }
}

impl PartialEq<A2aTaskStatus> for A2aTaskStatusWrapper {
    fn eq(&self, other: &A2aTaskStatus) -> bool {
        self.state() == other
    }
}

/// A2A message in a task conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aMessage {
    /// Message role ("user" or "agent").
    pub role: String,
    /// Message content parts.
    pub parts: Vec<A2aPart>,
}

/// A2A message content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum A2aPart {
    /// Text content.
    Text { text: String },
    /// File content (base64-encoded).
    File {
        name: String,
        mime_type: String,
        data: String,
    },
    /// Structured data.
    Data {
        mime_type: String,
        data: serde_json::Value,
    },
}

/// A2A artifact produced by a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aArtifact {
    /// Artifact name (optional per spec).
    #[serde(default)]
    pub name: Option<String>,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Artifact index in the sequence.
    #[serde(default)]
    pub index: Option<u32>,
    /// Whether this is the last chunk of a streamed artifact.
    #[serde(default)]
    pub last_chunk: Option<bool>,
    /// Artifact content parts.
    pub parts: Vec<A2aPart>,
}

// ---------------------------------------------------------------------------
// A2A Task Store — tracks task lifecycle
// ---------------------------------------------------------------------------

/// In-memory store for tracking A2A task lifecycle.
///
/// Tasks are created by `tasks/send`, polled by `tasks/get`, and cancelled
/// by `tasks/cancel`. The store is bounded to prevent memory exhaustion.
#[derive(Debug)]
pub struct A2aTaskStore {
    tasks: Mutex<HashMap<String, A2aTask>>,
    /// Maximum number of tasks to retain (FIFO eviction).
    max_tasks: usize,
}

impl A2aTaskStore {
    /// Create a new task store with a capacity limit.
    pub fn new(max_tasks: usize) -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            max_tasks,
        }
    }

    /// Insert a task. If the store is at capacity, the oldest task is evicted.
    pub fn insert(&self, task: A2aTask) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        // Evict oldest completed/failed/cancelled tasks if at capacity
        if tasks.len() >= self.max_tasks {
            let evict_key = tasks
                .iter()
                .filter(|(_, t)| {
                    matches!(
                        t.status.state(),
                        A2aTaskStatus::Completed | A2aTaskStatus::Failed | A2aTaskStatus::Cancelled
                    )
                })
                .map(|(k, _)| k.clone())
                .next();
            if let Some(key) = evict_key {
                tasks.remove(&key);
            }
        }
        tasks.insert(task.id.clone(), task);
    }

    /// Get a task by ID.
    pub fn get(&self, task_id: &str) -> Option<A2aTask> {
        self.tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(task_id)
            .cloned()
    }

    /// Update a task's status and optionally add messages/artifacts.
    pub fn update_status(&self, task_id: &str, status: A2aTaskStatus) -> bool {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = status.into();
            true
        } else {
            false
        }
    }

    /// Complete a task with a response message and optional artifacts.
    pub fn complete(&self, task_id: &str, response: A2aMessage, artifacts: Vec<A2aArtifact>) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(task_id) {
            task.messages.push(response);
            task.artifacts.extend(artifacts);
            task.status = A2aTaskStatus::Completed.into();
        }
    }

    /// Fail a task with an error message.
    pub fn fail(&self, task_id: &str, error_message: A2aMessage) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(task_id) {
            task.messages.push(error_message);
            task.status = A2aTaskStatus::Failed.into();
        }
    }

    /// Cancel a task.
    pub fn cancel(&self, task_id: &str) -> bool {
        self.update_status(task_id, A2aTaskStatus::Cancelled)
    }

    /// Count of tracked tasks.
    pub fn len(&self) -> usize {
        self.tasks.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for A2aTaskStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

// ---------------------------------------------------------------------------
// A2A Discovery — auto-discover external agents at boot
// ---------------------------------------------------------------------------

/// Discover all configured external A2A agents and return their cards.
///
/// Called during kernel boot to populate the list of known external agents.
pub async fn discover_external_agents(
    agents: &[openfang_types::config::ExternalAgent],
) -> Vec<(String, AgentCard)> {
    let client = A2aClient::new();
    let mut discovered = Vec::new();

    for agent in agents {
        match client.discover(&agent.url).await {
            Ok(card) => {
                info!(
                    name = %agent.name,
                    url = %agent.url,
                    skills = card.skills.len(),
                    "Discovered external A2A agent"
                );
                discovered.push((agent.name.clone(), card));
            }
            Err(e) => {
                warn!(
                    name = %agent.name,
                    url = %agent.url,
                    error = %e,
                    "Failed to discover external A2A agent"
                );
            }
        }
    }

    if !discovered.is_empty() {
        info!("A2A: discovered {} external agent(s)", discovered.len());
    }

    discovered
}

// ---------------------------------------------------------------------------
// A2A Server — expose OpenFang agents via A2A
// ---------------------------------------------------------------------------

/// Build an A2A Agent Card from an OpenFang agent manifest.
pub fn build_agent_card(manifest: &AgentManifest, base_url: &str) -> AgentCard {
    let tools: Vec<String> = manifest.capabilities.tools.clone();

    // Convert tool names to A2A skill descriptors
    let skills: Vec<AgentSkill> = tools
        .iter()
        .map(|tool| AgentSkill {
            id: tool.clone(),
            name: tool.replace('_', " "),
            description: format!("Can use the {tool} tool"),
            tags: vec!["tool".to_string()],
            examples: vec![],
        })
        .collect();

    AgentCard {
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        url: format!("{base_url}/a2a"),
        version: "0.1.0".to_string(),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
        },
        skills,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
    }
}

// ---------------------------------------------------------------------------
// A2A Client — discover and interact with external A2A agents
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// SSE parsing helpers
// ---------------------------------------------------------------------------

/// Outcome of processing a single `data:` SSE line.
pub(crate) enum SseLineOutcome {
    /// The line yielded a task update. `bool` is `true` when this is the final event.
    Task(A2aTask, bool),
    /// The line contained an error object — the stream should be aborted.
    Error(String),
    /// The line was empty, non-`data:`, or contained non-JSON payload (e.g. `[DONE]`).
    Skip,
}

/// Process a single raw SSE line and return what was found.
///
/// Accepts the full line (including the `data: ` prefix, if present).
/// All the JSON-parsing and task-extraction logic lives here — both
/// `parse_sse_content` and the streaming methods delegate to this function
/// so the logic is defined in exactly one place.
pub(crate) fn process_sse_line(line: &str) -> SseLineOutcome {
    let data = match line.strip_prefix("data: ") {
        Some(d) => d.trim(),
        None => return SseLineOutcome::Skip,
    };
    if data.is_empty() {
        return SseLineOutcome::Skip;
    }
    // Malformed / non-JSON payload (e.g. `data: [DONE]`): skip gracefully.
    let parsed: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            debug!(data, error = %e, "SSE data line is not valid JSON — skipping");
            return SseLineOutcome::Skip;
        }
    };

    if let Some(result) = parsed.get("result") {
        if let Ok(task) = serde_json::from_value::<A2aTask>(result.clone()) {
            let is_final = result
                .get("final")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            return SseLineOutcome::Task(task, is_final);
        }
    } else if let Some(error) = parsed.get("error") {
        return SseLineOutcome::Error(format!("A2A SSE error: {error}"));
    }
    SseLineOutcome::Skip
}

/// Consume a streaming SSE response and return the final `A2aTask`.
///
/// Shared implementation for both the basic and progress-reporting streaming
/// variants. Handles incremental byte accumulation (so multi-byte UTF-8
/// codepoints split across TCP chunk boundaries do not abort the stream),
/// splits the buffer into newline-terminated lines, and dispatches each one
/// through [`process_sse_line`]. The supplied `on_task` callback is invoked
/// for every task observed — final or not — so callers can stream progress
/// snapshots without re-implementing the parsing loop.
pub(crate) async fn consume_sse_stream<F, Fut>(
    response: Response,
    mut on_task: F,
) -> Result<A2aTask, String>
where
    F: FnMut(&A2aTask) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    // Incremental SSE processing: as each chunk arrives, append to a small
    // byte accumulator and dispatch any complete (newline-terminated) lines
    // through `process_sse_line`. We never hold the full response body in
    // memory at once.
    //
    // We accumulate raw *bytes* (not a `String`) because a single TCP chunk
    // may end in the middle of a multi-byte UTF-8 codepoint (very real with
    // CJK / emoji / accented text on small MTUs). Trying to convert each
    // chunk to `str` directly would abort the stream with a UTF-8 error.
    // Instead we only attempt UTF-8 decoding on prefixes that end at a
    // newline, where the boundary is guaranteed to fall between codepoints.
    let mut stream = response.bytes_stream();
    let mut bytes: Vec<u8> = Vec::new();
    let mut last_task: Option<A2aTask> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("SSE stream error: {e}"))?;
        bytes.extend_from_slice(&chunk);

        // Drain every complete (newline-terminated) line out of the byte
        // buffer. Any trailing partial-codepoint bytes stay in `bytes` for
        // the next iteration.
        while let Some(newline_pos) = bytes.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = bytes.drain(..=newline_pos).collect();
            // Drop the terminating '\n' before UTF-8 decoding — and strip
            // a trailing '\r' if present.
            let end = if line_bytes.len() >= 2 && line_bytes[line_bytes.len() - 2] == b'\r' {
                line_bytes.len() - 2
            } else {
                line_bytes.len() - 1
            };
            // Use lossy decoding: a stray invalid byte on a single line
            // shouldn't abort the entire stream. `process_sse_line` will
            // skip lines whose payload isn't valid JSON anyway.
            let line = String::from_utf8_lossy(&line_bytes[..end]);

            match process_sse_line(&line) {
                SseLineOutcome::Task(task, is_final) => {
                    on_task(&task).await;
                    last_task = Some(task);
                    if is_final {
                        return last_task.ok_or_else(|| "No task in final SSE event".to_string());
                    }
                }
                SseLineOutcome::Error(e) => return Err(e),
                SseLineOutcome::Skip => {}
            }
        }
    }

    // Stream closed without a final event — return whatever we have.
    last_task.ok_or_else(|| "SSE stream ended without a final event".to_string())
}

/// Parse a complete SSE stream body (already collected as a `&str`) and
/// return the final `A2aTask`.
///
/// Used by unit tests so they exercise the same line-processing code path
/// as the streaming production methods.
///
/// Behaviour:
/// - Lines beginning with `data: ` are JSON-parsed via `process_sse_line`.
/// - A line with `"error"` in the JSON object returns `Err`.
/// - A line with `"result"` that has `"final": true` returns immediately.
/// - Malformed JSON lines are skipped (logged at debug level).
/// - If the stream ends without a `"final": true` event, the last observed task
///   is returned (partial result) or an `Err` if no task was seen at all.
#[cfg(test)]
pub(crate) fn parse_sse_content(content: &str) -> Result<A2aTask, String> {
    let mut buf = content.to_string();
    let mut last_task: Option<A2aTask> = None;

    while let Some(newline_pos) = buf.find('\n') {
        let line = buf[..newline_pos].trim_end_matches('\r').to_string();
        buf = buf[newline_pos + 1..].to_string();

        match process_sse_line(&line) {
            SseLineOutcome::Task(task, is_final) => {
                last_task = Some(task);
                if is_final {
                    return last_task.ok_or_else(|| "No task in final SSE event".to_string());
                }
            }
            SseLineOutcome::Error(e) => return Err(e),
            SseLineOutcome::Skip => {}
        }
    }

    // Stream ended without a final event — return whatever we have.
    last_task.ok_or_else(|| "SSE stream ended without a final event".to_string())
}

/// Client for discovering and interacting with external A2A agents.
pub struct A2aClient {
    client: reqwest::Client,
}

impl A2aClient {
    /// Create a new A2A client.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Discover an external agent by fetching its Agent Card.
    pub async fn discover(&self, url: &str) -> Result<AgentCard, String> {
        let agent_json_url = format!("{}/.well-known/agent.json", url.trim_end_matches('/'));

        debug!(url = %agent_json_url, "Discovering A2A agent");

        let response = self
            .client
            .get(&agent_json_url)
            .header("User-Agent", "OpenFang/0.1 A2A")
            .send()
            .await
            .map_err(|e| format!("A2A discovery failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("A2A discovery returned {}", response.status()));
        }

        let card: AgentCard = response
            .json()
            .await
            .map_err(|e| format!("Invalid Agent Card: {e}"))?;

        info!(agent = %card.name, skills = card.skills.len(), "Discovered A2A agent");
        Ok(card)
    }

    /// Send a task to an external A2A agent.
    pub async fn send_task(
        &self,
        url: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> Result<A2aTask, String> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tasks/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": message}]
                },
                "sessionId": session_id,
            }
        });

        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("A2A send_task failed: {e}"))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Invalid A2A response: {e}"))?;

        if let Some(result) = body.get("result") {
            serde_json::from_value(result.clone())
                .map_err(|e| format!("Invalid A2A task response: {e}"))
        } else if let Some(error) = body.get("error") {
            Err(format!("A2A error: {}", error))
        } else {
            Err("Empty A2A response".to_string())
        }
    }

    /// Send a task to an external A2A agent using SSE streaming (`tasks/sendSubscribe`).
    ///
    /// Synchronous in the sense that it blocks until the server emits its final event
    /// (or the underlying connection closes), but it processes SSE lines incrementally
    /// as they arrive from the wire — the full response body is **not** buffered in
    /// memory. Each `data:` line is fed through [`process_sse_line`] the moment a
    /// newline is observed, preserving backpressure to the remote.
    ///
    /// # Deadline
    /// The total wall-clock time spent consuming the SSE stream is capped at
    /// [`SYNC_STREAMING_DEADLINE`] (300 s). This matches the `a2a_send` tool
    /// description's "blocks until complete, up to 300 s" contract. The deadline
    /// is enforced via [`tokio::time::timeout`] wrapping the entire SSE consumption,
    /// **not** by `reqwest`'s `.timeout()` — that one only governs the gap between
    /// successive read events, not total elapsed time. Wrapping `consume_sse_stream`
    /// gives us a true wall-clock deadline while still letting us distinguish
    /// connect-level failures (surfaced by `reqwest`) from "ran too long" timeouts
    /// (surfaced here as an explicit 300 s error).
    pub async fn send_task_streaming(
        &self,
        url: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> Result<A2aTask, String> {
        // Build a dedicated client for streaming. We deliberately do NOT set
        // `.timeout()` on the client itself: reqwest's request timeout fires
        // when a single read takes too long, which is the wrong semantic for a
        // long-lived SSE stream that may legitimately have idle gaps between
        // server-sent events. The total wall-clock deadline is enforced below
        // via `tokio::time::timeout` around the SSE consumption loop.
        let streaming_client = reqwest::Client::builder()
            .build()
            .map_err(|e| format!("Failed to build streaming client: {e}"))?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tasks/sendSubscribe",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": message}]
                },
                "sessionId": session_id,
            }
        });

        let response = streaming_client
            .post(url)
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("A2A send_task_streaming failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "A2A send_task_streaming returned {}",
                response.status()
            ));
        }

        // The actual SSE loop lives in `consume_sse_stream` so the streaming
        // and progress-reporting variants stay in lock-step. This caller
        // doesn't care about intermediate task snapshots — pass a no-op
        // callback. Wrap the consumption in a 300 s deadline so the tool
        // description's "up to 300 s" claim is enforced in behaviour, not
        // merely documentation.
        tokio::time::timeout(
            SYNC_STREAMING_DEADLINE,
            consume_sse_stream(response, |_task| async {}),
        )
        .await
        .map_err(|_| {
            format!(
                "A2A request exceeded {}s timeout",
                SYNC_STREAMING_DEADLINE.as_secs()
            )
        })?
    }

    /// Get the status of a task from an external A2A agent.
    pub async fn get_task(&self, url: &str, task_id: &str) -> Result<A2aTask, String> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tasks/get",
            "params": {
                "id": task_id,
            }
        });

        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("A2A get_task failed: {e}"))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Invalid A2A response: {e}"))?;

        if let Some(result) = body.get("result") {
            serde_json::from_value(result.clone()).map_err(|e| format!("Invalid A2A task: {e}"))
        } else {
            Err("Empty A2A response".to_string())
        }
    }
}

impl Default for A2aClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_from_manifest() {
        let manifest = AgentManifest {
            name: "test-agent".to_string(),
            description: "A test agent".to_string(),
            ..Default::default()
        };

        let card = build_agent_card(&manifest, "https://example.com");
        assert_eq!(card.name, "test-agent");
        assert_eq!(card.description, "A test agent");
        assert!(card.url.contains("/a2a"));
        assert!(card.capabilities.streaming);
        assert_eq!(card.default_input_modes, vec!["text"]);
    }

    #[test]
    fn test_a2a_task_status_transitions() {
        let task = A2aTask {
            id: "task-1".to_string(),
            session_id: None,
            status: A2aTaskStatus::Submitted.into(),
            messages: vec![],
            artifacts: vec![],
        };
        assert_eq!(task.status, A2aTaskStatus::Submitted);

        // Simulate progression
        let working = A2aTask {
            status: A2aTaskStatus::Working.into(),
            ..task.clone()
        };
        assert_eq!(working.status, A2aTaskStatus::Working);

        let completed = A2aTask {
            status: A2aTaskStatus::Completed.into(),
            ..task.clone()
        };
        assert_eq!(completed.status, A2aTaskStatus::Completed);

        let cancelled = A2aTask {
            status: A2aTaskStatus::Cancelled.into(),
            ..task.clone()
        };
        assert_eq!(cancelled.status, A2aTaskStatus::Cancelled);

        let failed = A2aTask {
            status: A2aTaskStatus::Failed.into(),
            ..task
        };
        assert_eq!(failed.status, A2aTaskStatus::Failed);
    }

    #[test]
    fn test_a2a_task_status_wrapper_object_form() {
        // Test deserialization of the object form: {"state": "completed", "message": null}
        let json = r#"{"state":"completed","message":null}"#;
        let wrapper: A2aTaskStatusWrapper = serde_json::from_str(json).unwrap();
        assert_eq!(wrapper, A2aTaskStatus::Completed);
        assert_eq!(wrapper.state(), &A2aTaskStatus::Completed);

        // Test with a message payload
        let json_with_msg = r#"{"state":"working","message":{"text":"Processing..."}}"#;
        let wrapper2: A2aTaskStatusWrapper = serde_json::from_str(json_with_msg).unwrap();
        assert_eq!(wrapper2, A2aTaskStatus::Working);

        // Test bare string form
        let json_bare = r#""completed""#;
        let wrapper3: A2aTaskStatusWrapper = serde_json::from_str(json_bare).unwrap();
        assert_eq!(wrapper3, A2aTaskStatus::Completed);
    }

    #[test]
    fn test_a2a_artifact_optional_fields() {
        // name is now optional — artifact with no name should deserialize
        let json = r#"{"parts":[{"type":"text","text":"hello"}]}"#;
        let artifact: A2aArtifact = serde_json::from_str(json).unwrap();
        assert!(artifact.name.is_none());
        assert!(artifact.description.is_none());
        assert!(artifact.metadata.is_none());
        assert!(artifact.index.is_none());
        assert!(artifact.last_chunk.is_none());
        assert_eq!(artifact.parts.len(), 1);

        // Full artifact with all optional fields
        let json_full = r#"{"name":"output.txt","description":"The result","metadata":{"key":"val"},"index":0,"lastChunk":true,"parts":[]}"#;
        let full: A2aArtifact = serde_json::from_str(json_full).unwrap();
        assert_eq!(full.name.as_deref(), Some("output.txt"));
        assert_eq!(full.description.as_deref(), Some("The result"));
        assert_eq!(full.index, Some(0));
        assert_eq!(full.last_chunk, Some(true));
    }

    #[test]
    fn test_a2a_message_serde() {
        let msg = A2aMessage {
            role: "user".to_string(),
            parts: vec![
                A2aPart::Text {
                    text: "Hello".to_string(),
                },
                A2aPart::Data {
                    mime_type: "application/json".to_string(),
                    data: serde_json::json!({"key": "value"}),
                },
            ],
        };

        let json = serde_json::to_string(&msg).unwrap();
        let back: A2aMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, "user");
        assert_eq!(back.parts.len(), 2);

        match &back.parts[0] {
            A2aPart::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_task_store_insert_and_get() {
        let store = A2aTaskStore::new(10);
        let task = A2aTask {
            id: "t-1".to_string(),
            session_id: None,
            status: A2aTaskStatus::Working.into(),
            messages: vec![],
            artifacts: vec![],
        };
        store.insert(task);
        assert_eq!(store.len(), 1);

        let got = store.get("t-1").unwrap();
        assert_eq!(got.status, A2aTaskStatus::Working);
    }

    #[test]
    fn test_task_store_complete_and_fail() {
        let store = A2aTaskStore::new(10);
        let task = A2aTask {
            id: "t-2".to_string(),
            session_id: None,
            status: A2aTaskStatus::Working.into(),
            messages: vec![],
            artifacts: vec![],
        };
        store.insert(task);

        store.complete(
            "t-2",
            A2aMessage {
                role: "agent".to_string(),
                parts: vec![A2aPart::Text {
                    text: "Done".to_string(),
                }],
            },
            vec![],
        );

        let completed = store.get("t-2").unwrap();
        assert_eq!(completed.status, A2aTaskStatus::Completed);
        assert_eq!(completed.messages.len(), 1);
    }

    #[test]
    fn test_task_store_cancel() {
        let store = A2aTaskStore::new(10);
        let task = A2aTask {
            id: "t-3".to_string(),
            session_id: None,
            status: A2aTaskStatus::Working.into(),
            messages: vec![],
            artifacts: vec![],
        };
        store.insert(task);
        assert!(store.cancel("t-3"));
        assert_eq!(store.get("t-3").unwrap().status, A2aTaskStatus::Cancelled);
        // Cancel a nonexistent task returns false
        assert!(!store.cancel("t-999"));
    }

    #[test]
    fn test_task_store_eviction() {
        let store = A2aTaskStore::new(2);
        // Insert 2 tasks
        for i in 0..2 {
            let task = A2aTask {
                id: format!("t-{i}"),
                session_id: None,
                status: A2aTaskStatus::Completed.into(),
                messages: vec![],
                artifacts: vec![],
            };
            store.insert(task);
        }
        assert_eq!(store.len(), 2);

        // Insert a 3rd — one completed task should be evicted
        let task = A2aTask {
            id: "t-2".to_string(),
            session_id: None,
            status: A2aTaskStatus::Working.into(),
            messages: vec![],
            artifacts: vec![],
        };
        store.insert(task);
        // One was evicted, plus the new one
        assert!(store.len() <= 2);
    }

    #[test]
    fn test_a2a_config_serde() {
        use openfang_types::config::{A2aConfig, ExternalAgent};

        let config = A2aConfig {
            enabled: true,
            listen_path: "/a2a".to_string(),
            external_agents: vec![ExternalAgent {
                name: "other-agent".to_string(),
                url: "https://other.example.com".to_string(),
            }],
        };

        let json = serde_json::to_string(&config).unwrap();
        let back: A2aConfig = serde_json::from_str(&json).unwrap();
        assert!(back.enabled);
        assert_eq!(back.listen_path, "/a2a");
        assert_eq!(back.external_agents.len(), 1);
        assert_eq!(back.external_agents[0].name, "other-agent");
    }

    // -----------------------------------------------------------------------
    // SSE parser edge-case tests (via parse_sse_content)
    // -----------------------------------------------------------------------

    /// Build a minimal A2A JSON-RPC result payload for use in SSE lines.
    fn sse_result_line(task_id: &str, status: &str, is_final: bool) -> String {
        let payload = serde_json::json!({
            "result": {
                "id": task_id,
                "status": status,
                "messages": [],
                "artifacts": [],
                "final": is_final,
            }
        });
        format!("data: {}\n", payload)
    }

    /// 1. Normal completion: a valid final-event stream returns the task.
    #[test]
    fn test_sse_parse_normal_completion() {
        let mut stream = String::new();
        stream.push_str(&sse_result_line("t-ok", "working", false));
        stream.push_str(&sse_result_line("t-ok", "completed", true));

        let result = parse_sse_content(&stream).expect("Should succeed on normal completion");
        assert_eq!(result.id, "t-ok");
        assert_eq!(result.status, A2aTaskStatus::Completed);
    }

    /// 2. Disconnect mid-stream (no final event): returns the last seen task
    ///    rather than panicking or hanging.
    #[test]
    fn test_sse_parse_disconnect_mid_stream() {
        let mut stream = String::new();
        stream.push_str(&sse_result_line("t-partial", "working", false));
        // Stream ends here — no final event.

        let result =
            parse_sse_content(&stream).expect("Should return partial result on disconnect");
        assert_eq!(result.id, "t-partial");
        assert_eq!(result.status, A2aTaskStatus::Working);
    }

    /// 3. Malformed JSON in a data field: the bad line is skipped; subsequent
    ///    valid lines are still processed.
    #[test]
    fn test_sse_parse_malformed_json_skipped() {
        let mut stream = String::new();
        stream.push_str("data: {not json at all}\n");
        stream.push_str(&sse_result_line("t-after-bad", "completed", true));

        // Must not panic; the malformed line is skipped and the valid final
        // event is returned.
        let result = parse_sse_content(&stream)
            .expect("Malformed JSON line must be skipped, valid final event returned");
        assert_eq!(result.id, "t-after-bad");
    }

    /// 4. Valid data lines with no `"final"` field: parse_sse_content returns
    ///    the accumulated last task rather than hanging or returning an error.
    #[test]
    fn test_sse_parse_no_final_event_returns_last_task() {
        let mut stream = String::new();
        // Three working events, none marked final.
        for _ in 0..3 {
            stream.push_str(&sse_result_line("t-nofinal", "working", false));
        }

        let result = parse_sse_content(&stream)
            .expect("Should return last task when no final event is present");
        assert_eq!(result.id, "t-nofinal");
    }

    /// 5. Parser correctly handles a single concatenated input that contains
    ///    a complete `data:` line. This is *not* a wire-level chunk-reassembly
    ///    test (the parser only ever sees a single `&str`); real chunk-boundary
    ///    coverage lives in the integration tests that drive
    ///    `consume_sse_stream` with a mocked TCP source.
    #[test]
    fn test_sse_parse_concatenated_lines() {
        // Build the full final-event line.
        let full_line = sse_result_line("t-chunked", "completed", true);

        // Split the line at an arbitrary point in the middle of the JSON.
        let split_at = full_line.len() / 2;
        let chunk_a = &full_line[..split_at];
        let chunk_b = &full_line[split_at..];

        // parse_sse_content processes the accumulated buffer — concatenate both
        // chunks to simulate what the production streaming loop does.
        let reassembled = format!("{chunk_a}{chunk_b}");
        let result =
            parse_sse_content(&reassembled).expect("Chunked event must be reassembled correctly");
        assert_eq!(result.id, "t-chunked");
        assert_eq!(result.status, A2aTaskStatus::Completed);
    }

    /// 6. Error event in the stream: parse_sse_content surfaces the error
    ///    rather than silently dropping it.
    #[test]
    fn test_sse_parse_error_event() {
        let stream = "data: {\"error\":{\"code\":-32600,\"message\":\"Bad request\"}}\n";
        let result = parse_sse_content(stream);
        assert!(result.is_err(), "Error event should return Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("A2A SSE error") || msg.contains("Bad request"),
            "Error message should mention the error: {msg}"
        );
    }

    /// 7. Completely empty stream: returns a clear error (no task seen).
    #[test]
    fn test_sse_parse_empty_stream() {
        let result = parse_sse_content("");
        assert!(result.is_err(), "Empty stream should return Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("without a final event") || msg.contains("no task"),
            "Error should indicate missing final event: {msg}"
        );
    }
}
