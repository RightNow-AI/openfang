//! Tool loop detection for the agent execution loop.
//!
//! Tracks tool calls within a single agent loop execution using SHA-256
//! hashes of `(tool_name, serialized_params)`. Detects when the agent is
//! stuck calling the same tool repeatedly and provides graduated responses:
//! warn, block, or circuit-break the entire loop.
//!
//! Enhanced features beyond basic hash-counting:
//! - **Outcome-aware detection**: tracks result hashes so identical call+result
//!   pairs escalate faster than just repeated calls.
//! - **Ping-pong detection**: identifies A-B-A-B or A-B-C-A-B-C alternating
//!   patterns that evade single-hash counting.
//! - **Poll tool handling**: relaxed thresholds for tools expected to be called
//!   repeatedly (e.g. `shell_exec` status checks).
//! - **Backoff suggestions**: recommends increasing wait times for polling.
//! - **Warning bucket**: prevents spam by upgrading to Block after repeated
//!   warnings for the same call.
//! - **Statistics snapshot**: exposes internal state for debugging and API.

use omtae_types::message::{ContentBlock, Message, MessageContent, Role};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Injected when the agent repeats the same planning text without tool calls.
pub const PLANNING_LOOP_NUDGE: &str = "[System: STOP PLANNING — call tools now. You repeated the same planning text without executing tools. Use shell_exec, file_list, memory_recall, or web_search immediately. Do not reply with more planning prose.]";

/// Injected on first planning-only response (no tools yet).
pub const PLANNING_WITHOUT_TOOLS_NUDGE: &str = "[System: You wrote planning text without calling tools. Execute tools NOW (shell_exec, file_list, memory_recall). Max one planning line, then a tool call. Never claim workspace/memory status without tool output.]";

/// Injected when the agent states factual claims without any tool evidence.
pub const TOOL_REQUIRED_NUDGE: &str = "[System: TOOL_REQUIRED — You stated factual claims (paths, URLs, status, lists, or counts) without tool evidence in this conversation. Call shell_exec, file_list, web_search, memory_recall, or curl /api/brain/* NOW. Do not guess workspace, brain vault, peer agents, or service status.]";

/// Outcome when the agent loops on planning text without tool calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningLoopAction {
    Reprompt(&'static str),
    ForceEnd(String),
}

/// Extract visible text from an assistant message for repetition checks.
pub fn assistant_message_text(msg: &Message) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Normalize assistant text to a prefix for repetition detection.
pub fn assistant_text_prefix(text: &str) -> String {
    let t = text.trim().to_lowercase();
    let end = t.find('\n').unwrap_or(t.len()).min(80);
    t[..end].trim().to_string()
}

/// Topic stem for fuzzy planning-loop detection (same intent, wording drifts).
pub fn assistant_planning_stem(text: &str) -> String {
    let t = text.trim().to_lowercase();
    if t.contains("obsidian") && t.contains("brain") {
        return "topic:obsidian-brain".to_string();
    }
    if t.contains("obsidian") {
        return "topic:obsidian".to_string();
    }
    for marker in [
        " - let me ",
        " let me ",
        " i'll ",
        " i will ",
        " looking at ",
        " let me also ",
    ] {
        if let Some(idx) = t.find(marker) {
            let stem = t[..idx].trim();
            if stem.len() >= 12 {
                return stem.to_string();
            }
        }
    }
    assistant_text_prefix(text)
}

/// Count prior assistant turns whose opening prefix matches `prefix`.
pub fn count_matching_assistant_prefix(messages: &[Message], prefix: &str) -> u32 {
    if prefix.len() < 15 {
        return 0;
    }
    messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .map(|m| assistant_text_prefix(&assistant_message_text(m)))
        .filter(|p| p == prefix)
        .count() as u32
}

/// Count prior assistant turns with the same planning stem (fuzzy).
pub fn count_matching_planning_stem(messages: &[Message], stem: &str) -> u32 {
    if stem.len() < 8 && !stem.starts_with("topic:") {
        return 0;
    }
    messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .map(|m| assistant_planning_stem(&assistant_message_text(m)))
        .filter(|s| s == stem)
        .count() as u32
}

/// Count prior assistant turns that are planning prose without tools.
pub fn count_prior_planning_prose(messages: &[Message]) -> u32 {
    messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .filter(|m| planning_without_tools_detected(&assistant_message_text(m)))
        .count() as u32
}

/// True when the assistant turn includes ToolResult blocks from prior tool calls.
pub fn has_tool_results_in_context(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        matches!(
            &m.content,
            MessageContent::Blocks(blocks)
                if blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        )
    })
}

/// Detect meta-narration about the user's intent (planning without action).
pub fn meta_narration_detected(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "the user is asking about",
        "the user wants",
        "the user is asking",
        "based on the user's question",
        "the user mentioned",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

/// Count assistant turns with meta-narration in this loop (including current text).
pub fn count_meta_narration(messages: &[Message], current: &str) -> u32 {
    let prior = messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .filter(|m| meta_narration_detected(&assistant_message_text(m)))
        .count() as u32;
    prior + u32::from(meta_narration_detected(current))
}

/// Detect factual claims that require tool backing (URLs, paths, status, lists, counts).
pub fn contains_factual_claims(text: &str) -> bool {
    let lower = text.to_lowercase();

    if text.contains("https://") || text.contains("http://") {
        return true;
    }

    if text.contains("/home/")
        || text.contains("~/.")
        || text.contains("/etc/")
        || text.contains("/api/brain/")
        || lower.contains("vault at ")
    {
        return true;
    }

    let status_patterns = [
        "is running",
        "is healthy",
        "status: ok",
        "status: warning",
        "status: critical",
        "peer agent",
        "agents listed",
        "connected peers",
        "daemon is",
        "service is",
        "vault contains",
        "brain status",
        "obsidian vault",
        "found ",
        "there are ",
        "currently running",
        "currently active",
    ];
    if status_patterns.iter().any(|p| lower.contains(p)) {
        return true;
    }

    let numbered = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
                && (trimmed.contains(". ") || trimmed.contains(") "))
        })
        .count();
    if numbered >= 3 {
        return true;
    }

    let words: Vec<&str> = lower.split_whitespace().collect();
    for window in words.windows(2) {
        if let [a, b] = window {
            if a.chars().all(|c| c.is_ascii_digit())
                && matches!(
                    *b,
                    "agents" | "files" | "peers" | "items" | "results" | "skills" | "agents."
                )
            {
                return true;
            }
        }
    }

    false
}

/// ECC-inspired verification gate: block factual claims without tool evidence.
pub fn check_verification_gate(
    messages: &[Message],
    text: &str,
    any_tools_executed: bool,
    verification_reprompts: u32,
) -> Option<PlanningLoopAction> {
    if text.trim().is_empty() {
        return None;
    }
    if any_tools_executed || has_tool_results_in_context(messages) {
        return None;
    }
    if !contains_factual_claims(text) {
        return None;
    }
    if verification_reprompts < 2 {
        return Some(PlanningLoopAction::Reprompt(TOOL_REQUIRED_NUDGE));
    }
    Some(PlanningLoopAction::ForceEnd(
        "TOOL_REQUIRED: Stopped — factual claims without tool evidence. Use shell_exec, file_list, web_search, or curl /api/brain/status.".to_string(),
    ))
}

/// Detect planning prose without tool execution (first-offense nudge).
pub fn planning_without_tools_detected(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "let me check",
        "let me also check",
        "i'll check",
        "i will check",
        "checking the workspace",
        "checking workspace",
        "check workspace and memory",
        "let me look",
        "let me look around",
        "i need to check",
        "first i'll",
        "first i will",
        "looking at the peer",
        "peer agents listed",
        "obsidian vault",
        "obsidian skill",
        "related setup",
        "related files",
        "the user is asking about",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

/// Detect repeated planning text (same prefix 3x) or planning without tools.
pub fn check_planning_loop(
    messages: &[Message],
    text: &str,
    any_tools_executed: bool,
    planning_reprompts: u32,
) -> Option<PlanningLoopAction> {
    if any_tools_executed || text.trim().is_empty() {
        return None;
    }
    let prefix = assistant_text_prefix(text);
    let stem = assistant_planning_stem(text);
    let exact_repeat = count_matching_assistant_prefix(messages, &prefix) + 1;
    let stem_repeat = count_matching_planning_stem(messages, &stem) + 1;
    let planning_prose_repeat = count_prior_planning_prose(messages) + 1;
    let meta_repeat = count_meta_narration(messages, text);
    let repeat_count = exact_repeat
        .max(stem_repeat)
        .max(planning_prose_repeat)
        .max(if meta_repeat >= 2 { meta_repeat } else { 0 });

    if repeat_count >= 2 {
        if planning_reprompts < 1 {
            return Some(PlanningLoopAction::Reprompt(PLANNING_LOOP_NUDGE));
        }
        return Some(PlanningLoopAction::ForceEnd(format!(
            "Stopped after repeating the same planning text {repeat_count} times without tool calls. \
             Run shell_exec or file_list on your next message."
        )));
    }
    if planning_without_tools_detected(text) {
        if planning_reprompts < 2 {
            return Some(PlanningLoopAction::Reprompt(PLANNING_WITHOUT_TOOLS_NUDGE));
        }
        return Some(PlanningLoopAction::ForceEnd(
            "Stopped after planning prose without tool calls. Use shell_exec (curl /api/brain/status, \
             file_list) or web_search — do not repeat planning."
                .to_string(),
        ));
    }
    None
}

/// Tools that are expected to be polled repeatedly.
const POLL_TOOLS: &[&str] = &[
    "shell_exec", // checking command output
];

/// Maximum recent call history size for ping-pong detection.
const HISTORY_SIZE: usize = 30;

/// Backoff schedule in milliseconds for polling tools.
const BACKOFF_SCHEDULE_MS: &[u64] = &[5000, 10000, 30000, 60000];

/// Configuration for the loop guard.
#[derive(Debug, Clone)]
pub struct LoopGuardConfig {
    /// Number of identical calls before a warning is appended.
    pub warn_threshold: u32,
    /// Number of identical calls before the call is blocked.
    pub block_threshold: u32,
    /// Total tool calls across all tools before circuit-breaking.
    pub global_circuit_breaker: u32,
    /// Multiplier for poll tool thresholds (poll tools get thresholds * this).
    pub poll_multiplier: u32,
    /// Number of identical outcome pairs before a warning.
    pub outcome_warn_threshold: u32,
    /// Number of identical outcome pairs before the next call is auto-blocked.
    pub outcome_block_threshold: u32,
    /// Minimum repeats of a ping-pong pattern before blocking.
    pub ping_pong_min_repeats: u32,
    /// Max warnings per unique tool call hash before upgrading to Block.
    pub max_warnings_per_call: u32,
    /// Cap `agent_send` calls per agent loop (orchestrator / meta-agents).
    pub max_agent_send_per_loop: Option<u32>,
}

impl Default for LoopGuardConfig {
    fn default() -> Self {
        Self {
            warn_threshold: 3,
            block_threshold: 5,
            global_circuit_breaker: 30,
            poll_multiplier: 3,
            outcome_warn_threshold: 2,
            outcome_block_threshold: 3,
            ping_pong_min_repeats: 3,
            max_warnings_per_call: 3,
            max_agent_send_per_loop: None,
        }
    }
}

/// Verdict from the loop guard on whether a tool call should proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopGuardVerdict {
    /// Proceed normally.
    Allow,
    /// Proceed, but append a warning to the tool result.
    Warn(String),
    /// Block this specific tool call (skip execution).
    Block(String),
    /// Circuit-break the entire agent loop.
    CircuitBreak(String),
}

/// Snapshot of the loop guard state (for debugging/API).
#[derive(Debug, Clone, Serialize)]
pub struct LoopGuardStats {
    /// Total tool calls made in this loop execution.
    pub total_calls: u32,
    /// Number of unique (tool_name + params) combinations seen.
    pub unique_calls: u32,
    /// Number of calls that were blocked.
    pub blocked_calls: u32,
    /// Whether a ping-pong pattern has been detected.
    pub ping_pong_detected: bool,
    /// The tool name that has been repeated the most (if any).
    pub most_repeated_tool: Option<String>,
    /// The count of the most repeated tool call.
    pub most_repeated_count: u32,
}

/// Tracks tool calls within a single agent loop to detect loops.
pub struct LoopGuard {
    config: LoopGuardConfig,
    /// Count of identical (tool_name + params) calls, keyed by SHA-256 hex hash.
    call_counts: HashMap<String, u32>,
    /// Total tool calls in this loop execution.
    total_calls: u32,
    /// Count of identical (tool_call_hash + result_hash) pairs.
    outcome_counts: HashMap<String, u32>,
    /// Call hashes that are blocked due to repeated identical outcomes.
    blocked_outcomes: HashSet<String>,
    /// Recent tool call hashes (ring buffer of last HISTORY_SIZE).
    recent_calls: Vec<String>,
    /// Warnings already emitted (to prevent spam). Key = call hash, value = count emitted.
    warnings_emitted: HashMap<String, u32>,
    /// Tracks poll counts per command hash for backoff suggestions.
    poll_counts: HashMap<String, u32>,
    /// Total calls that were blocked.
    blocked_calls: u32,
    /// Map from call hash to tool name (for stats reporting).
    hash_to_tool: HashMap<String, String>,
    /// `agent_send` invocations in this loop (for delegation caps).
    agent_send_calls: u32,
}

impl LoopGuard {
    /// Create a new loop guard with the given configuration.
    pub fn new(config: LoopGuardConfig) -> Self {
        Self {
            config,
            call_counts: HashMap::new(),
            total_calls: 0,
            outcome_counts: HashMap::new(),
            blocked_outcomes: HashSet::new(),
            recent_calls: Vec::with_capacity(HISTORY_SIZE),
            warnings_emitted: HashMap::new(),
            poll_counts: HashMap::new(),
            blocked_calls: 0,
            hash_to_tool: HashMap::new(),
            agent_send_calls: 0,
        }
    }

    /// Check whether a tool call should proceed.
    ///
    /// Returns a verdict indicating whether to allow, warn, block, or
    /// circuit-break. The caller should act on the verdict before executing
    /// the tool.
    pub fn check(&mut self, tool_name: &str, params: &serde_json::Value) -> LoopGuardVerdict {
        self.total_calls += 1;

        // Global circuit breaker
        if self.total_calls > self.config.global_circuit_breaker {
            self.blocked_calls += 1;
            return LoopGuardVerdict::CircuitBreak(format!(
                "Circuit breaker: exceeded {} total tool calls in this loop. \
                 The agent appears to be stuck.",
                self.config.global_circuit_breaker
            ));
        }

        // Per-turn delegation cap (orchestrator agent_send storms)
        if tool_name == "agent_send" {
            self.agent_send_calls += 1;
            if let Some(max) = self.config.max_agent_send_per_loop {
                if self.agent_send_calls > max {
                    self.blocked_calls += 1;
                    return LoopGuardVerdict::CircuitBreak(format!(
                        "Delegation limit: agent_send called {} times (max {max} per turn). \
                         Synthesize results from specialists already contacted and reply to the user.",
                        self.agent_send_calls
                    ));
                }
            }
        }

        let hash = Self::compute_hash(tool_name, params);
        self.hash_to_tool
            .entry(hash.clone())
            .or_insert_with(|| tool_name.to_string());

        // Track recent calls for ping-pong detection
        if self.recent_calls.len() >= HISTORY_SIZE {
            self.recent_calls.remove(0);
        }
        self.recent_calls.push(hash.clone());

        // Check if this call hash was blocked by outcome detection
        if self.blocked_outcomes.contains(&hash) {
            self.blocked_calls += 1;
            return LoopGuardVerdict::Block(format!(
                "Blocked: tool '{}' is returning identical results repeatedly. \
                 The current approach is not working — try something different.",
                tool_name
            ));
        }

        let count = self.call_counts.entry(hash.clone()).or_insert(0);
        *count += 1;
        let count_val = *count;

        // Determine effective thresholds (poll tools get relaxed thresholds)
        let is_poll = Self::is_poll_call(tool_name, params);
        let is_delegate = matches!(tool_name, "agent_send" | "agent_list" | "agent_spawn");
        let (effective_warn, effective_block) = if is_delegate {
            (2, 4)
        } else if is_poll {
            let m = self.config.poll_multiplier;
            (
                self.config.warn_threshold * m,
                self.config.block_threshold * m,
            )
        } else {
            (self.config.warn_threshold, self.config.block_threshold)
        };

        // Check per-hash thresholds
        if count_val >= effective_block {
            self.blocked_calls += 1;
            return LoopGuardVerdict::Block(format!(
                "Blocked: tool '{}' called {} times with identical parameters. \
                 Try a different approach or different parameters.",
                tool_name, count_val
            ));
        }

        if count_val >= effective_warn {
            // Warning bucket: check if we've already warned too many times
            let warning_count = self.warnings_emitted.entry(hash.clone()).or_insert(0);
            *warning_count += 1;
            if *warning_count > self.config.max_warnings_per_call {
                // Upgrade to block after too many warnings
                self.blocked_calls += 1;
                return LoopGuardVerdict::Block(format!(
                    "Blocked: tool '{}' called {} times with identical parameters \
                     (warnings exhausted). Try a different approach.",
                    tool_name, count_val
                ));
            }
            return LoopGuardVerdict::Warn(format!(
                "Warning: tool '{}' has been called {} times with identical parameters. \
                 Consider a different approach.",
                tool_name, count_val
            ));
        }

        // Ping-pong detection (runs even if individual hash counts are low)
        if let Some(ping_pong_msg) = self.detect_ping_pong() {
            // Count how many full pattern repeats we have
            let repeats = self.count_ping_pong_repeats();
            if repeats >= self.config.ping_pong_min_repeats {
                self.blocked_calls += 1;
                return LoopGuardVerdict::Block(ping_pong_msg);
            }
            // Below min_repeats, just warn
            let warning_count = self
                .warnings_emitted
                .entry(format!("pingpong_{}", hash))
                .or_insert(0);
            *warning_count += 1;
            if *warning_count <= self.config.max_warnings_per_call {
                return LoopGuardVerdict::Warn(ping_pong_msg);
            }
        }

        LoopGuardVerdict::Allow
    }

    /// Record the outcome of a tool call. Call this AFTER tool execution.
    ///
    /// Hashes `(tool_name | params_json | result_truncated)` and tracks how
    /// many times an identical call produces an identical result. Returns a
    /// warning string if outcome repetition is detected.
    pub fn record_outcome(
        &mut self,
        tool_name: &str,
        params: &serde_json::Value,
        result: &str,
    ) -> Option<String> {
        let outcome_hash = Self::compute_outcome_hash(tool_name, params, result);
        let call_hash = Self::compute_hash(tool_name, params);

        let count = self.outcome_counts.entry(outcome_hash).or_insert(0);
        *count += 1;
        let count_val = *count;

        if count_val >= self.config.outcome_block_threshold {
            // Mark the call hash so the NEXT check() auto-blocks it
            self.blocked_outcomes.insert(call_hash);
            return Some(format!(
                "Tool '{}' is returning identical results — the approach isn't working.",
                tool_name
            ));
        }

        if count_val >= self.config.outcome_warn_threshold {
            return Some(format!(
                "Tool '{}' is returning identical results — the approach isn't working.",
                tool_name
            ));
        }

        None
    }

    /// Get the suggested backoff delay (in milliseconds) for a polling tool call.
    ///
    /// Returns `None` if this is not a poll call. Returns `Some(ms)` with a
    /// suggested delay from the backoff schedule, capping at the last entry.
    pub fn get_poll_backoff(&mut self, tool_name: &str, params: &serde_json::Value) -> Option<u64> {
        if !Self::is_poll_call(tool_name, params) {
            return None;
        }
        let hash = Self::compute_hash(tool_name, params);
        let count = self.poll_counts.entry(hash).or_insert(0);
        *count += 1;
        // count is 1-indexed; backoff starts on the second call
        if *count <= 1 {
            return None;
        }
        let idx = (*count as usize).saturating_sub(2);
        let delay = BACKOFF_SCHEDULE_MS
            .get(idx)
            .copied()
            .unwrap_or(*BACKOFF_SCHEDULE_MS.last().unwrap_or(&60000));
        Some(delay)
    }

    /// Get a snapshot of current loop guard statistics.
    pub fn stats(&self) -> LoopGuardStats {
        let unique_calls = self.call_counts.len() as u32;

        // Find the most repeated tool call
        let mut most_repeated_tool: Option<String> = None;
        let mut most_repeated_count: u32 = 0;
        for (hash, &count) in &self.call_counts {
            if count > most_repeated_count {
                most_repeated_count = count;
                most_repeated_tool = self.hash_to_tool.get(hash).cloned();
            }
        }

        LoopGuardStats {
            total_calls: self.total_calls,
            unique_calls,
            blocked_calls: self.blocked_calls,
            ping_pong_detected: self.detect_ping_pong_pure(),
            most_repeated_tool,
            most_repeated_count,
        }
    }

    /// Check if a tool call looks like a polling operation.
    ///
    /// Poll tools (like `shell_exec` for status checks) are expected to be
    /// called repeatedly and get relaxed loop detection thresholds.
    fn is_poll_call(tool_name: &str, params: &serde_json::Value) -> bool {
        // Known poll tools with short commands that look like status checks
        if POLL_TOOLS.contains(&tool_name) {
            if let Some(cmd) = params.get("command").and_then(|v| v.as_str()) {
                let cmd_lower = cmd.to_lowercase();
                // Commands that explicitly check status/wait/poll
                if cmd_lower.contains("status")
                    || cmd_lower.contains("poll")
                    || cmd_lower.contains("wait")
                    || cmd_lower.contains("watch")
                    || cmd_lower.contains("tail")
                    || cmd_lower.contains("ps ")
                    || cmd_lower.contains("jobs")
                    || cmd_lower.contains("pgrep")
                    || cmd_lower.contains("docker ps")
                    || cmd_lower.contains("kubectl get")
                {
                    return true;
                }
            }
        }
        // Generic poll detection via params keywords
        let params_str = serde_json::to_string(params)
            .unwrap_or_default()
            .to_lowercase();
        params_str.contains("status") || params_str.contains("poll") || params_str.contains("wait")
    }

    /// Detect ping-pong patterns (A-B-A-B or A-B-C-A-B-C) in recent call history.
    ///
    /// Checks if the last 6+ calls form a repeating pattern of length 2 or 3.
    /// Returns a warning message if a pattern is detected, `None` otherwise.
    fn detect_ping_pong(&self) -> Option<String> {
        self.detect_ping_pong_impl()
    }

    /// Pure version for stats (no &mut self needed, just reads state).
    fn detect_ping_pong_pure(&self) -> bool {
        self.detect_ping_pong_impl().is_some()
    }

    /// Shared ping-pong detection implementation.
    fn detect_ping_pong_impl(&self) -> Option<String> {
        let len = self.recent_calls.len();

        // Check for pattern of length 2 (A-B-A-B-A-B)
        // Need at least 6 entries for 3 repeats of length 2
        if len >= 6 {
            let tail = &self.recent_calls[len - 6..];
            let a = &tail[0];
            let b = &tail[1];
            if a != b && tail[2] == *a && tail[3] == *b && tail[4] == *a && tail[5] == *b {
                let tool_a = self
                    .hash_to_tool
                    .get(a)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let tool_b = self
                    .hash_to_tool
                    .get(b)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                return Some(format!(
                    "Ping-pong detected: tools '{}' and '{}' are alternating \
                     repeatedly. Break the cycle by trying a different approach.",
                    tool_a, tool_b
                ));
            }
        }

        // Check for pattern of length 3 (A-B-C-A-B-C-A-B-C)
        // Need at least 9 entries for 3 repeats of length 3
        if len >= 9 {
            let tail = &self.recent_calls[len - 9..];
            let a = &tail[0];
            let b = &tail[1];
            let c = &tail[2];
            // Ensure they're not all the same (that's just repetition, not ping-pong)
            if !(a == b && b == c)
                && tail[3] == *a
                && tail[4] == *b
                && tail[5] == *c
                && tail[6] == *a
                && tail[7] == *b
                && tail[8] == *c
            {
                let tool_a = self
                    .hash_to_tool
                    .get(a)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let tool_b = self
                    .hash_to_tool
                    .get(b)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let tool_c = self
                    .hash_to_tool
                    .get(c)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                return Some(format!(
                    "Ping-pong detected: tools '{}', '{}', '{}' are cycling \
                     repeatedly. Break the cycle by trying a different approach.",
                    tool_a, tool_b, tool_c
                ));
            }
        }

        None
    }

    /// Count how many full repeats of the detected ping-pong pattern exist
    /// in the recent call history.
    fn count_ping_pong_repeats(&self) -> u32 {
        let len = self.recent_calls.len();

        // Check pattern of length 2
        if len >= 4 {
            let a = &self.recent_calls[len - 2];
            let b = &self.recent_calls[len - 1];
            if a != b {
                let mut repeats: u32 = 0;
                let mut i = len;
                while i >= 2 {
                    i -= 2;
                    if self.recent_calls[i] == *a && self.recent_calls[i + 1] == *b {
                        repeats += 1;
                    } else {
                        break;
                    }
                }
                if repeats >= 2 {
                    return repeats;
                }
            }
        }

        // Check pattern of length 3
        if len >= 6 {
            let a = &self.recent_calls[len - 3];
            let b = &self.recent_calls[len - 2];
            let c = &self.recent_calls[len - 1];
            if !(a == b && b == c) {
                let mut repeats: u32 = 0;
                let mut i = len;
                while i >= 3 {
                    i -= 3;
                    if self.recent_calls[i] == *a
                        && self.recent_calls[i + 1] == *b
                        && self.recent_calls[i + 2] == *c
                    {
                        repeats += 1;
                    } else {
                        break;
                    }
                }
                if repeats >= 2 {
                    return repeats;
                }
            }
        }

        0
    }

    /// Compute a SHA-256 hash of the tool name and parameters.
    pub fn compute_hash(tool_name: &str, params: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        hasher.update(tool_name.as_bytes());
        hasher.update(b"|");
        // Serialize params deterministically (serde_json sorts object keys)
        let params_str = serde_json::to_string(params).unwrap_or_default();
        hasher.update(params_str.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Compute a SHA-256 hash of the tool name, parameters, AND result.
    ///
    /// Result is truncated to 1000 chars to avoid hashing huge outputs
    /// while still catching identical short results.
    fn compute_outcome_hash(tool_name: &str, params: &serde_json::Value, result: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(tool_name.as_bytes());
        hasher.update(b"|");
        let params_str = serde_json::to_string(params).unwrap_or_default();
        hasher.update(params_str.as_bytes());
        hasher.update(b"|");
        let truncated = crate::str_utils::safe_truncate_str(result, 1000);
        hasher.update(truncated.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Existing tests (preserved unchanged)
    // ========================================================================

    #[test]
    fn allow_below_threshold() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"query": "test"});
        let v = guard.check("web_search", &params);
        assert_eq!(v, LoopGuardVerdict::Allow);
        let v = guard.check("web_search", &params);
        assert_eq!(v, LoopGuardVerdict::Allow);
    }

    #[test]
    fn warn_at_threshold() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"path": "/etc/passwd"});
        // Calls 1, 2 = Allow
        guard.check("file_read", &params);
        guard.check("file_read", &params);
        // Call 3 = Warn (warn_threshold = 3)
        let v = guard.check("file_read", &params);
        assert!(matches!(v, LoopGuardVerdict::Warn(_)));
    }

    #[test]
    fn block_at_threshold() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"command": "ls"});
        for _ in 0..4 {
            guard.check("shell_exec", &params);
        }
        // Call 5 = Block (block_threshold = 5)
        let v = guard.check("shell_exec", &params);
        assert!(matches!(v, LoopGuardVerdict::Block(_)));
    }

    #[test]
    fn different_params_no_collision() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        for i in 0..10 {
            let params = serde_json::json!({"query": format!("query_{}", i)});
            let v = guard.check("web_search", &params);
            assert_eq!(v, LoopGuardVerdict::Allow);
        }
    }

    #[test]
    fn global_circuit_breaker() {
        let config = LoopGuardConfig {
            warn_threshold: 100,
            block_threshold: 100,
            global_circuit_breaker: 5,
            ..Default::default()
        };
        let mut guard = LoopGuard::new(config);
        for i in 0..5 {
            let params = serde_json::json!({"n": i});
            let v = guard.check("tool", &params);
            assert_eq!(v, LoopGuardVerdict::Allow);
        }
        // Call 6 triggers circuit breaker (> 5)
        let v = guard.check("tool", &serde_json::json!({"n": 5}));
        assert!(matches!(v, LoopGuardVerdict::CircuitBreak(_)));
    }

    #[test]
    fn default_config() {
        let config = LoopGuardConfig::default();
        assert_eq!(config.warn_threshold, 3);
        assert_eq!(config.block_threshold, 5);
        assert_eq!(config.global_circuit_breaker, 30);
    }

    // ========================================================================
    // New tests — Outcome-Aware Detection
    // ========================================================================

    #[test]
    fn test_outcome_aware_warning() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"query": "weather"});
        let result = "sunny 72F";

        // First outcome: no warning
        let w = guard.record_outcome("web_search", &params, result);
        assert!(w.is_none());

        // Second identical outcome: warning (outcome_warn_threshold = 2)
        let w = guard.record_outcome("web_search", &params, result);
        assert!(w.is_some());
        assert!(w.unwrap().contains("identical results"));
    }

    #[test]
    fn test_outcome_aware_blocks_next_call() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"query": "weather"});
        let result = "sunny 72F";

        // Record 3 identical outcomes (outcome_block_threshold = 3)
        guard.record_outcome("web_search", &params, result);
        guard.record_outcome("web_search", &params, result);
        let w = guard.record_outcome("web_search", &params, result);
        assert!(w.is_some());

        // The NEXT check() for this call hash should auto-block
        let v = guard.check("web_search", &params);
        assert!(matches!(v, LoopGuardVerdict::Block(_)));
        if let LoopGuardVerdict::Block(msg) = v {
            assert!(msg.contains("identical results"));
        }
    }

    // ========================================================================
    // New tests — Ping-Pong Detection
    // ========================================================================

    #[test]
    fn test_ping_pong_ab_detection() {
        let mut guard = LoopGuard::new(LoopGuardConfig {
            // Set thresholds high so individual hash counting doesn't interfere
            warn_threshold: 100,
            block_threshold: 100,
            ping_pong_min_repeats: 3,
            ..Default::default()
        });
        let params_a = serde_json::json!({"file": "a.txt"});
        let params_b = serde_json::json!({"file": "b.txt"});

        // A-B-A-B-A-B = 3 repeats of (A,B)
        guard.check("file_read", &params_a);
        guard.check("file_write", &params_b);
        guard.check("file_read", &params_a);
        guard.check("file_write", &params_b);
        guard.check("file_read", &params_a);
        let v = guard.check("file_write", &params_b);

        // Should detect ping-pong and block (3 full repeats)
        assert!(
            matches!(v, LoopGuardVerdict::Block(ref msg) if msg.contains("Ping-pong"))
                || matches!(v, LoopGuardVerdict::Warn(ref msg) if msg.contains("Ping-pong")),
            "Expected ping-pong detection, got: {:?}",
            v
        );
    }

    #[test]
    fn test_ping_pong_abc_detection() {
        let mut guard = LoopGuard::new(LoopGuardConfig {
            warn_threshold: 100,
            block_threshold: 100,
            ping_pong_min_repeats: 3,
            ..Default::default()
        });
        let params_a = serde_json::json!({"a": 1});
        let params_b = serde_json::json!({"b": 2});
        let params_c = serde_json::json!({"c": 3});

        // A-B-C-A-B-C-A-B-C = 3 repeats of (A,B,C)
        for _ in 0..3 {
            guard.check("tool_a", &params_a);
            guard.check("tool_b", &params_b);
            guard.check("tool_c", &params_c);
        }

        // The pattern should be detected by the 9th call
        let stats = guard.stats();
        assert!(stats.ping_pong_detected);
    }

    #[test]
    fn test_no_false_ping_pong() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());

        // Various different calls — no pattern
        for i in 0..10 {
            let params = serde_json::json!({"n": i});
            guard.check("tool", &params);
        }

        let stats = guard.stats();
        assert!(!stats.ping_pong_detected);
    }

    // ========================================================================
    // New tests — Poll Tool Handling
    // ========================================================================

    #[test]
    fn test_poll_tool_relaxed_thresholds() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        // shell_exec with short status-check command = poll call
        // Default thresholds: warn=3, block=5, poll_multiplier=3
        // Effective for poll: warn=9, block=15
        let params = serde_json::json!({"command": "docker ps --status running"});

        // Calls 1..8 should all be Allow (below warn=9)
        for _ in 0..8 {
            let v = guard.check("shell_exec", &params);
            assert_eq!(
                v,
                LoopGuardVerdict::Allow,
                "Poll tool should have relaxed thresholds"
            );
        }

        // Call 9 should be Warn
        let v = guard.check("shell_exec", &params);
        assert!(
            matches!(v, LoopGuardVerdict::Warn(_)),
            "Expected warn at poll threshold, got: {:?}",
            v
        );
    }

    #[test]
    fn test_is_poll_call_detection() {
        // shell_exec with short status-check command
        assert!(LoopGuard::is_poll_call(
            "shell_exec",
            &serde_json::json!({"command": "docker ps --status"})
        ));

        // shell_exec with short tail command
        assert!(LoopGuard::is_poll_call(
            "shell_exec",
            &serde_json::json!({"command": "tail -f /var/log/app.log"})
        ));

        // shell_exec with short command but NO poll keywords — NOT a poll
        assert!(!LoopGuard::is_poll_call(
            "shell_exec",
            &serde_json::json!({"command": "echo hi"})
        ));

        // shell_exec with no poll keywords — NOT a poll
        assert!(!LoopGuard::is_poll_call(
            "shell_exec",
            &serde_json::json!({"command": "this is a very long command that definitely exceeds fifty characters in length"})
        ));

        // Non-poll tool with no poll keywords
        assert!(!LoopGuard::is_poll_call(
            "file_read",
            &serde_json::json!({"path": "/etc/hosts"})
        ));

        // Generic poll detection via params containing "status"
        assert!(LoopGuard::is_poll_call(
            "some_tool",
            &serde_json::json!({"check": "status"})
        ));

        // Generic poll detection via params containing "poll"
        assert!(LoopGuard::is_poll_call(
            "api_call",
            &serde_json::json!({"action": "poll_results"})
        ));

        // Generic poll detection via params containing "wait"
        assert!(LoopGuard::is_poll_call(
            "queue",
            &serde_json::json!({"mode": "wait_for_completion"})
        ));
    }

    // ========================================================================
    // New tests — Backoff Schedule
    // ========================================================================

    #[test]
    fn test_poll_backoff_schedule() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params = serde_json::json!({"command": "kubectl get pods --status"});

        // First call: no backoff
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, None);

        // Second call: 5000ms
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, Some(5000));

        // Third call: 10000ms
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, Some(10000));

        // Fourth call: 30000ms
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, Some(30000));

        // Fifth call: 60000ms
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, Some(60000));

        // Sixth call: caps at 60000ms
        let b = guard.get_poll_backoff("shell_exec", &params);
        assert_eq!(b, Some(60000));

        // Non-poll tool: no backoff
        let non_poll = serde_json::json!({"path": "/etc/hosts"});
        let b = guard.get_poll_backoff("file_read", &non_poll);
        assert_eq!(b, None);
    }

    // ========================================================================
    // New tests — Warning Bucket
    // ========================================================================

    #[test]
    fn test_warning_bucket_limits() {
        let mut guard = LoopGuard::new(LoopGuardConfig {
            warn_threshold: 2,
            block_threshold: 100, // set very high so only warning bucket triggers block
            max_warnings_per_call: 2,
            ..Default::default()
        });
        let params = serde_json::json!({"x": 1});

        // Call 1: Allow
        let v = guard.check("tool", &params);
        assert_eq!(v, LoopGuardVerdict::Allow);

        // Call 2: Warn (hits warn_threshold=2), warning_count = 1
        let v = guard.check("tool", &params);
        assert!(matches!(v, LoopGuardVerdict::Warn(_)));

        // Call 3: Warn again, warning_count = 2
        let v = guard.check("tool", &params);
        assert!(matches!(v, LoopGuardVerdict::Warn(_)));

        // Call 4: warning_count would be 3, exceeds max_warnings_per_call=2 -> Block
        let v = guard.check("tool", &params);
        assert!(
            matches!(v, LoopGuardVerdict::Block(_)),
            "Expected block after warning limit, got: {:?}",
            v
        );
    }

    #[test]
    fn test_warning_upgrade_to_block() {
        let mut guard = LoopGuard::new(LoopGuardConfig {
            warn_threshold: 1,
            block_threshold: 100,
            max_warnings_per_call: 1,
            ..Default::default()
        });
        let params = serde_json::json!({"y": 2});

        // Call 1: Warn (warn_threshold=1), warning_count = 1
        let v = guard.check("tool", &params);
        assert!(matches!(v, LoopGuardVerdict::Warn(_)));

        // Call 2: warning_count would be 2, exceeds max_warnings_per_call=1 -> Block
        let v = guard.check("tool", &params);
        assert!(
            matches!(v, LoopGuardVerdict::Block(ref msg) if msg.contains("warnings exhausted")),
            "Expected block with 'warnings exhausted', got: {:?}",
            v
        );
    }

    // ========================================================================
    // New tests — Statistics Snapshot
    // ========================================================================

    #[test]
    fn test_stats_snapshot() {
        let mut guard = LoopGuard::new(LoopGuardConfig::default());
        let params_a = serde_json::json!({"a": 1});
        let params_b = serde_json::json!({"b": 2});

        // 3 calls to tool_a, 1 to tool_b
        guard.check("tool_a", &params_a);
        guard.check("tool_a", &params_a);
        guard.check("tool_a", &params_a);
        guard.check("tool_b", &params_b);

        let stats = guard.stats();
        assert_eq!(stats.total_calls, 4);
        assert_eq!(stats.unique_calls, 2);
        assert_eq!(stats.most_repeated_tool, Some("tool_a".to_string()));
        assert_eq!(stats.most_repeated_count, 3);
        assert!(!stats.ping_pong_detected);
    }

    // ========================================================================
    // New tests — History Ring Buffer
    // ========================================================================

    #[test]
    fn test_history_ring_buffer_limit() {
        let config = LoopGuardConfig {
            warn_threshold: 100,
            block_threshold: 100,
            global_circuit_breaker: 200,
            ..Default::default()
        };
        let mut guard = LoopGuard::new(config);

        // Push 50 unique calls (exceeds HISTORY_SIZE of 30)
        for i in 0..50 {
            let params = serde_json::json!({"n": i});
            guard.check("tool", &params);
        }

        // Internal ring buffer should be capped at HISTORY_SIZE
        assert_eq!(guard.recent_calls.len(), HISTORY_SIZE);

        // Stats should reflect all 50 calls
        let stats = guard.stats();
        assert_eq!(stats.total_calls, 50);
        assert_eq!(stats.unique_calls, 50);
    }

    // ========================================================================
    // Planning text loop detection
    // ========================================================================

    #[test]
    fn test_planning_without_tools_nudge() {
        let text = "Let me check workspace and memory first.";
        let action = check_planning_loop(&[], text, false, 0);
        assert!(matches!(
            action,
            Some(PlanningLoopAction::Reprompt(PLANNING_WITHOUT_TOOLS_NUDGE))
        ));
    }

    #[test]
    fn test_planning_loop_same_prefix_second_time() {
        let msg = |t: &str| Message::assistant(t.to_string());
        let messages = vec![msg("Let me check workspace and memory for status.")];
        let action = check_planning_loop(
            &messages,
            "Let me check workspace and memory for status.",
            false,
            0,
        );
        assert!(matches!(
            action,
            Some(PlanningLoopAction::Reprompt(PLANNING_LOOP_NUDGE))
        ));
    }

    #[test]
    fn test_planning_loop_force_end_after_nudge() {
        let msg = |t: &str| Message::assistant(t.to_string());
        let messages = vec![
            msg("Let me check workspace and memory for status."),
            msg("Let me check workspace and memory for status."),
        ];
        let action = check_planning_loop(
            &messages,
            "Let me check workspace and memory for status.",
            false,
            1,
        );
        assert!(matches!(action, Some(PlanningLoopAction::ForceEnd(_))));
    }

    #[test]
    fn test_planning_loop_skipped_after_tools() {
        let action = check_planning_loop(
            &[],
            "Let me check workspace and memory.",
            true,
            0,
        );
        assert!(action.is_none());
    }

    #[test]
    fn test_obsidian_brain_stem_catches_wording_drift() {
        let msg = |t: &str| Message::assistant(t.to_string());
        let messages = vec![msg(
            "The user is asking about their Obsidian visual brain - let me check peer agents.",
        )];
        let action = check_planning_loop(
            &messages,
            "The user is asking about their Obsidian visual brain - let me look around for vault files.",
            false,
            0,
        );
        assert!(matches!(
            action,
            Some(PlanningLoopAction::Reprompt(PLANNING_LOOP_NUDGE))
        ));
        assert_eq!(
            assistant_planning_stem("Obsidian visual brain - let me check"),
            "topic:obsidian-brain"
        );
    }

    #[test]
    fn test_verification_gate_blocks_unbacked_claims() {
        let text = "There are 5 peer agents listed. The brain vault at /home/jay/vaults/omtae-brain is running.";
        let action = check_verification_gate(&[], text, false, 0);
        assert!(matches!(
            action,
            Some(PlanningLoopAction::Reprompt(TOOL_REQUIRED_NUDGE))
        ));
    }

    #[test]
    fn test_verification_gate_allows_after_tools() {
        let text = "There are 5 peer agents listed.";
        let action = check_verification_gate(&[], text, true, 0);
        assert!(action.is_none());
    }

    #[test]
    fn test_verification_gate_allows_opinion_without_facts() {
        let text = "Happy to help with your question about Obsidian.";
        let action = check_verification_gate(&[], text, false, 0);
        assert!(action.is_none());
    }

    #[test]
    fn test_meta_narration_counts_toward_loop() {
        let msg = |t: &str| Message::assistant(t.to_string());
        let messages = vec![msg("The user is asking about their brain vault.")];
        assert_eq!(count_meta_narration(&messages, "The user wants Obsidian info."), 2);
    }

    #[test]
    fn test_planning_force_end_after_two_nudges() {
        let text = "Let me check the obsidian vault and related files.";
        let action = check_planning_loop(&[], text, false, 2);
        assert!(matches!(action, Some(PlanningLoopAction::ForceEnd(_))));
    }
}
