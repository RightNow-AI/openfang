//! Claude Code CLI backend driver.
//!
//! Spawns the `claude` CLI (Claude Code) as a subprocess in print mode (`-p`),
//! which is non-interactive and handles its own authentication.
//! This allows users with Claude Code installed to use it as an LLM provider
//! without needing a separate API key.
//!
//! Tracks active subprocess PIDs and enforces message timeouts to prevent
//! hung CLI processes from blocking agents indefinitely.

use crate::bridge_auth::{SpawnGuard, TokenIssuer};
use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use dashmap::DashMap;
use openfang_types::agent::AgentId;
use openfang_types::message::{ContentBlock, MessageContent, Role, StopReason, TokenUsage};
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tracing::{debug, info, warn};

/// Env var names published by the daemon for bridge-wiring discovery.
/// Kept as string literals (not imported from `openfang_mcp_bridge`)
/// because the runtime crate intentionally does not depend on the bridge
/// crate — the bridge depends on the runtime's protocol surface, not the
/// other way around. The values must match
/// `openfang_mcp_bridge::protocol::SOCKET_ENV_VAR` and the daemon-side
/// fallback in `openfang-api::server::run_daemon`.
const BRIDGE_SOCKET_ENV: &str = "OPENFANG_BRIDGE_SOCKET";
const BRIDGE_BIN_ENV: &str = "OPENFANG_BRIDGE_BIN";
const BRIDGE_TOKEN_ENV: &str = "OPENFANG_BRIDGE_TOKEN";
const BRIDGE_AGENT_ID_ENV: &str = "OPENFANG_BRIDGE_AGENT_ID";
/// Optional comma-separated tool allowlist sent to the bridge subprocess.
/// Sourced per-spawn from the calling agent's `available_tools` via
/// `CompletionRequest::allowed_tools`. Absent (env not set) → bridge
/// falls back to its hard-coded `DEFAULT_ALLOWED`.
const BRIDGE_ALLOWED_ENV: &str = "OPENFANG_BRIDGE_ALLOWED";

/// Master kill-switch for the OpenFang MCP bridge. Default off. When unset
/// or not in {`1`, `true`}, `try_build_bridge_mcp_config` returns `None` and
/// CC is spawned exactly as it was before ANAI-30 step 4 — no `--mcp-config`,
/// no temp file, no bridge child. Flip via `launchctl setenv
/// OPENFANG_BRIDGE_ENABLED 1` (in the daemon's launchd plist for persistence)
/// then bounce the daemon. Lets us deploy the bridge code path without
/// putting it inline with every CC invocation, so a regression doesn't take
/// down model completions until the gate is removed once the bridge is
/// validated end-to-end.
const BRIDGE_ENABLED_ENV: &str = "OPENFANG_BRIDGE_ENABLED";

/// Returns `true` iff the bridge gate env var is set to a recognized truthy
/// value. Anything else — unset, empty, `0`, `false`, garbage — is `false`.
fn bridge_enabled() -> bool {
    match std::env::var(BRIDGE_ENABLED_ENV) {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
}

/// Opt-in diagnostic flag for bridge-wired CC spawns. When set, the driver
/// adds `--debug` to claude (which dumps MCP launch + handshake details into
/// `~/.claude/debug/<uuid>.txt`) and logs a 4 KB tail of CC's stderr at INFO
/// after the subprocess exits. Off by default — `--debug` is noisy and
/// produces a debug file per spawn. Use only when actively debugging the
/// bridge handshake or MCP wiring; the daemon-side `bridge_ipc` INFO logs
/// (`accepted connection`, `handshake complete`, `dispatching call`) cover
/// the normal observability needs.
const BRIDGE_DEBUG_ENV: &str = "OPENFANG_BRIDGE_DEBUG";

fn bridge_debug_enabled() -> bool {
    match std::env::var(BRIDGE_DEBUG_ENV) {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
}

/// MCP server name advertised inside the per-spawn `--mcp-config`. CC will
/// namespace each tool as `mcp__<this>__<toolname>`.
const BRIDGE_MCP_SERVER_NAME: &str = "openfang";

/// Native Claude Code tools that OpenFang denies by policy when the bridge
/// is wired. Anything that touches the filesystem, executes commands, or
/// fetches the web is funneled through the OpenFang bridge instead, where
/// it can be RBAC-gated per-agent via `agent.toml` capabilities.
///
/// Mechanism: emitted into a per-spawn `settings.json` and passed to CC via
/// `claude --settings <file>`. Per Anthropic's settings documentation,
/// `permissions.deny` rules are enforced even when
/// `--dangerously-skip-permissions` is set — that flag only bypasses
/// allow/ask prompts, not security-critical denies. So the daemon keeps the
/// non-interactive YOLO flag (required because there is no TTY to answer
/// prompts) while still authoritatively blocking the native tool surface.
///
/// Naming uses bare tool names (e.g. `"Bash"`, not `"Bash(*)"`), which
/// blanket-denies the tool with no specifier. The MCP namespace
/// (`mcp__openfang__*`) lives in a separate pattern space and is untouched —
/// that's the point: replace the native surface with the gated bridge
/// surface.
///
/// Inclusions:
/// - `Bash` — shell execution. `shell_exec` (commit 13b) is the gated
///   replacement.
/// - `Read`/`Write`/`Edit`/`MultiEdit`/`NotebookEdit` — filesystem mutation.
///   Bridge's `file_read`/`file_write` are the gated replacements.
/// - `WebFetch`/`WebSearch` — outbound network from the model. Bridge's
///   `web_fetch` is the gated replacement.
/// - `Glob`/`Grep` — filesystem read, denied for symmetry: any FS-read path
///   must go through the bridge's sandboxed `file_read`/`file_list`.
/// - `BashOutput`/`KillShell`/`KillBash` — read stdout / kill backgrounded
///   `Bash`. Inert today (Bash is denied) but locked down as defense in
///   depth: if a future CC release introduces a separate path to background
///   shells, these adjuncts must not become live.
/// - `SlashCommand` — invokes CC's skill substrate, a parallel curation
///   surface. OpenFang's skill curation is the canonical path; deny CC's.
/// - `EnterWorktree`/`ExitWorktree` — creates a git worktree on disk
///   outside the agent workspace, a direct FS-escape primitive. A native
///   workspace-aware worktree tool is on the follow-up backlog; until then,
///   no worktree creation through CC.
/// - `NotebookRead` — symmetry with `NotebookEdit`. Not advertised in the
///   current CC version we target, but pre-denied for forward-compat: if
///   a future CC ships it, the deny is already in place.
/// - `CronCreate`/`CronDelete`/`CronList` — CC-side scheduling control
///   plane (Anthropic-hosted Routines). Per the OpenFang-First Principle,
///   scheduling is owned by OF's cron / orchestrator. No parallel plane.
/// - `ScheduleWakeup` — same class as Cron*: lets CC self-pace dynamic
///   loops outside OF's orchestration. Denied.
/// - `RemoteTrigger` — directly manages Anthropic-hosted scheduled jobs
///   via `claude.ai/v1/code/triggers`. Same OpenFang-First rationale.
/// - `Monitor` — long-running shell streamer. Executes arbitrary shell
///   commands, bypassing `shell_exec`'s `exec_policy` gating. This is
///   `Bash`-with-streaming; denied for the same reason as `Bash`.
/// - `PushNotification` — bypasses OF's channel routing for user-facing
///   notifications. Soft policy gap rather than security; denied to keep
///   comms on the canonical path.
///
/// Deliberately NOT denied: `TodoWrite`, `Task` (subagent), `Skill`,
/// `AskUserQuestion`, `EnterPlanMode`/`ExitPlanMode`, `TaskOutput`/
/// `TaskStop`, `ToolSearch` — agent-internal control flow / UX with no
/// escape surface to the host system.
const CC_NATIVE_DENY: &[&str] = &[
    // Shell execution + adjuncts
    "Bash",
    "BashOutput",
    "KillShell",
    "KillBash",
    "Monitor",
    // Filesystem read/write
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "NotebookRead",
    "Glob",
    "Grep",
    // Worktree (FS escape)
    "EnterWorktree",
    "ExitWorktree",
    // Network
    "WebFetch",
    "WebSearch",
    // Skill / command substrate
    "SlashCommand",
    // Scheduling / remote control plane (OpenFang-First)
    "CronCreate",
    "CronDelete",
    "CronList",
    "ScheduleWakeup",
    "RemoteTrigger",
    // Comms routing
    "PushNotification",
];

/// Environment variable names (and suffixes) to strip from the subprocess
/// to prevent leaking API keys from other providers. We keep the full env
/// intact (so Node.js, NVM, SSL, proxies, etc. all work) and only remove
/// secrets that belong to other LLM providers.
const SENSITIVE_ENV_EXACT: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "MISTRAL_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "OPENROUTER_API_KEY",
    "PERPLEXITY_API_KEY",
    "COHERE_API_KEY",
    "AI21_API_KEY",
    "CEREBRAS_API_KEY",
    "SAMBANOVA_API_KEY",
    "HUGGINGFACE_API_KEY",
    "XAI_API_KEY",
    "REPLICATE_API_TOKEN",
    "BRAVE_API_KEY",
    "TAVILY_API_KEY",
    "ELEVENLABS_API_KEY",
];

/// Suffixes that indicate a secret — remove any env var ending with these
/// unless it starts with `CLAUDE_`.
const SENSITIVE_SUFFIXES: &[&str] = &["_SECRET", "_TOKEN", "_PASSWORD"];

/// Default subprocess timeout in seconds (5 minutes).
const DEFAULT_MESSAGE_TIMEOUT_SECS: u64 = 300;

/// LLM driver that delegates to the Claude Code CLI.
pub struct ClaudeCodeDriver {
    cli_path: String,
    skip_permissions: bool,
    /// Active subprocess PIDs keyed by a caller-provided label (e.g. agent name).
    /// Allows external code to check if a subprocess is running and kill it.
    active_pids: Arc<DashMap<String, u32>>,
    /// Message timeout in seconds. CLI subprocesses that exceed this are killed.
    message_timeout_secs: u64,
    /// Optional daemon-side token issuer for per-spawn bridge auth (ANAI-31).
    ///
    /// When `Some`, `try_build_bridge_mcp_config` requests a fresh
    /// `SpawnGuard` from the issuer and emits the guard's token over
    /// `OPENFANG_BRIDGE_TOKEN`. The guard is stashed in the returned
    /// `BridgeMcpConfig` so its `Drop` (which evicts the token from the
    /// authority's spawn table) fires when the per-spawn config drops —
    /// which itself outlives the `claude` subprocess.
    ///
    /// When `None`, the driver falls back to the legacy ANAI-30 UUID path:
    /// a random token is generated and emitted, but the daemon does not
    /// know about it. Useful for dev builds wired without the bridge
    /// daemon and for tests; production daemon wiring will always populate
    /// this field via [`Self::with_token_issuer`] in Phase C.
    token_issuer: Option<Arc<dyn TokenIssuer>>,
}

impl ClaudeCodeDriver {
    /// Create a new Claude Code driver.
    ///
    /// `cli_path` overrides the CLI binary path; defaults to `"claude"` on PATH.
    /// `skip_permissions` adds `--dangerously-skip-permissions` to the spawned
    /// command so that the CLI runs non-interactively (required for daemon mode).
    pub fn new(cli_path: Option<String>, skip_permissions: bool) -> Self {
        if skip_permissions {
            warn!(
                "Claude Code driver: --dangerously-skip-permissions enabled. \
                 The CLI will not prompt for tool approvals. \
                 OpenFang's own capability/RBAC system enforces access control."
            );
        }

        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "claude".to_string()),
            skip_permissions,
            active_pids: Arc::new(DashMap::new()),
            message_timeout_secs: DEFAULT_MESSAGE_TIMEOUT_SECS,
            token_issuer: None,
        }
    }

    /// Attach a per-spawn bridge `TokenIssuer`. Builder-style so the wiring
    /// site in `crates/openfang-runtime/src/drivers/mod.rs` (Phase C) can
    /// install the daemon's `Arc<BridgeAuthority>` after construction
    /// without breaking the existing constructor signatures or the four
    /// other driver-build call sites.
    pub fn with_token_issuer(mut self, issuer: Arc<dyn TokenIssuer>) -> Self {
        self.token_issuer = Some(issuer);
        self
    }

    /// Create a new Claude Code driver with a custom timeout.
    pub fn with_timeout(
        cli_path: Option<String>,
        skip_permissions: bool,
        timeout_secs: u64,
    ) -> Self {
        let mut driver = Self::new(cli_path, skip_permissions);
        driver.message_timeout_secs = timeout_secs;
        driver
    }

    /// Get a snapshot of active subprocess PIDs.
    /// Returns a vec of (label, pid) pairs.
    pub fn active_pids(&self) -> Vec<(String, u32)> {
        self.active_pids
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }

    /// Get the shared PID map for external monitoring.
    pub fn pid_map(&self) -> Arc<DashMap<String, u32>> {
        Arc::clone(&self.active_pids)
    }

    /// Detect if the Claude Code CLI is available on PATH.
    pub fn detect() -> Option<String> {
        let output = std::process::Command::new("claude")
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Build a text prompt from the completion request messages.
    ///
    /// The Claude Code CLI is text-only (`-p <prompt>`), so non-text content
    /// blocks (images, etc.) cannot be sent natively. Rather than dropping
    /// them silently — which causes the model to hallucinate about content
    /// it can't see — we render each non-text block as a synthetic
    /// `[attachment: ...]` marker. The model still can't *view* the
    /// attachment, but it knows the attachment exists and can acknowledge
    /// it coherently instead of confabulating.
    fn build_prompt(request: &CompletionRequest) -> String {
        let mut parts = Vec::new();

        for msg in &request.messages {
            let role_label = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
            };
            let rendered = Self::render_content(&msg.content);
            if !rendered.is_empty() {
                parts.push(format!("[{role_label}]\n{rendered}"));
            }
        }

        parts.join("\n\n")
    }

    /// Render message content for the text-only CLI prompt.
    ///
    /// Text blocks pass through verbatim. Image blocks are rendered as
    /// `[attachment: <media_type> image, ~N KB — not viewable on this
    /// provider]` so the model receives a positive signal that an
    /// attachment arrived. ToolUse/ToolResult/Thinking are omitted —
    /// the CLI manages its own tool loop.
    fn render_content(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text, .. } => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text.clone())
                        }
                    }
                    ContentBlock::Image { media_type, data } => {
                        // base64 → ~3/4 the length in decoded bytes.
                        let approx_kb = (data.len().saturating_mul(3) / 4) / 1024;
                        Some(format!(
                            "[attachment: {media_type} image, ~{approx_kb} KB — not viewable on this provider]"
                        ))
                    }
                    ContentBlock::ToolUse { .. }
                    | ContentBlock::ToolResult { .. }
                    | ContentBlock::Thinking { .. }
                    | ContentBlock::RedactedThinking { .. }
                    | ContentBlock::Unknown => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Map a model ID like "claude-code/opus" to CLI --model flag value.
    fn model_flag(model: &str) -> Option<String> {
        let stripped = model.strip_prefix("claude-code/").unwrap_or(model);
        match stripped {
            "opus" => Some("opus".to_string()),
            "sonnet" => Some("sonnet".to_string()),
            "haiku" => Some("haiku".to_string()),
            _ => Some(stripped.to_string()),
        }
    }

    /// Apply security env filtering to a command.
    ///
    /// Instead of `env_clear()` (which breaks Node.js, NVM, SSL, proxies),
    /// we keep the full environment and only remove known sensitive API keys
    /// from other LLM providers.
    fn apply_env_filter(cmd: &mut tokio::process::Command) {
        for key in SENSITIVE_ENV_EXACT {
            cmd.env_remove(key);
        }
        // Strip bridge discovery env from CC's child env. The bridge gets
        // these via the per-spawn `--mcp-config` `env` map (set explicitly
        // when `try_build_bridge_mcp_config` writes the config); CC itself
        // has no use for them and inheriting them would risk a stray bridge
        // process picking up the daemon socket without a fresh per-spawn
        // token.
        cmd.env_remove(BRIDGE_SOCKET_ENV);
        cmd.env_remove(BRIDGE_BIN_ENV);
        cmd.env_remove(BRIDGE_TOKEN_ENV);
        cmd.env_remove(BRIDGE_AGENT_ID_ENV);
        cmd.env_remove(BRIDGE_ALLOWED_ENV);
        // Remove any env var with a sensitive suffix, unless it's CLAUDE_*
        for (key, _) in std::env::vars() {
            if key.starts_with("CLAUDE_") {
                continue;
            }
            let upper = key.to_uppercase();
            for suffix in SENSITIVE_SUFFIXES {
                if upper.ends_with(suffix) {
                    cmd.env_remove(&key);
                    break;
                }
            }
        }
    }
}

/// Per-spawn MCP config handle. Holds the path to a JSON file that CC
/// reads via `--mcp-config <path>` to discover the OpenFang bridge.
///
/// The file lives next to the daemon's bridge socket (under `<home>/run/`)
/// for the duration of a single CC invocation and is removed on drop.
/// Per-spawn so each `claude` subprocess gets a fresh auth token; CC's
/// lifetime bounds the file's lifetime, which bounds the token's lifetime.
struct BridgeMcpConfig {
    config_path: PathBuf,
    /// Per-spawn token guard. `Drop` evicts the token from the daemon's
    /// `BridgeAuthority` spawn table and zeroizes the in-memory `Token`
    /// bytes. `None` on the legacy ANAI-30 path (random UUID, no daemon
    /// registration). Underscore-prefixed in field name semantically —
    /// existence-only; we never read it.
    _guard: Option<SpawnGuard>,
}

impl BridgeMcpConfig {
    fn path(&self) -> &std::path::Path {
        &self.config_path
    }
}

impl Drop for BridgeMcpConfig {
    fn drop(&mut self) {
        // Best-effort: if removal fails (e.g. socket dir already gone on
        // shutdown) we don't propagate. The file is per-invocation and
        // contains only a token + paths; staleness is harmless.
        let _ = std::fs::remove_file(&self.config_path);
    }
}

/// Build the `--mcp-config` JSON document from already-resolved inputs.
/// Pure — no env reads, no filesystem writes — so it's tractable to test.
/// The wire shape mirrors what `claude --mcp-config` accepts: a top-level
/// `mcpServers` object keyed by server name, each value carrying `command`,
/// `args`, and `env`.
fn build_bridge_mcp_config_value(
    socket: &str,
    bridge_bin: &str,
    agent_id: &str,
    token: &str,
    allowed_tools: Option<&[String]>,
) -> serde_json::Value {
    let mut env_map = serde_json::Map::new();
    env_map.insert(
        BRIDGE_SOCKET_ENV.into(),
        serde_json::Value::String(socket.to_string()),
    );
    env_map.insert(
        BRIDGE_TOKEN_ENV.into(),
        serde_json::Value::String(token.to_string()),
    );
    env_map.insert(
        BRIDGE_AGENT_ID_ENV.into(),
        serde_json::Value::String(agent_id.to_string()),
    );
    // Per-agent allowlist override for the bridge subprocess. When the
    // caller supplies a tool list (sourced from `agent.toml` →
    // `available_tools` → `CompletionRequest::allowed_tools`), emit it
    // as a comma-separated `OPENFANG_BRIDGE_ALLOWED` env entry; the
    // bridge then narrows its advertised + dispatchable surface to the
    // intersection with `built_in_tools()`. Absent (None), the bridge
    // falls back to its hard-coded `DEFAULT_ALLOWED`.
    //
    // Empty list is meaningful: it means "no bridge tools permitted."
    // We still emit the env var (as the empty string) so the bridge's
    // empty-after-trim handling produces an explicit zero-tool surface
    // instead of silently falling back to the default.
    if let Some(tools) = allowed_tools {
        let joined = tools.join(",");
        env_map.insert(BRIDGE_ALLOWED_ENV.into(), serde_json::Value::String(joined));
    }

    let mut server_entry = serde_json::Map::new();
    server_entry.insert(
        "command".into(),
        serde_json::Value::String(bridge_bin.to_string()),
    );
    server_entry.insert("args".into(), serde_json::Value::Array(vec![]));
    server_entry.insert("env".into(), serde_json::Value::Object(env_map));

    let mut servers = serde_json::Map::new();
    servers.insert(
        BRIDGE_MCP_SERVER_NAME.into(),
        serde_json::Value::Object(server_entry),
    );

    let mut root = serde_json::Map::new();
    root.insert("mcpServers".into(), serde_json::Value::Object(servers));
    serde_json::Value::Object(root)
}

/// Legacy ANAI-30 fallback: per-spawn random UUID token.
///
/// Only used when the driver has no `token_issuer` attached (dev builds
/// without daemon bridge wiring, plus the test suite). The daemon-issued
/// path goes through `TokenIssuer::issue` and emits a fresh
/// `openfang_types::bridge_auth::Token` instead. Production wiring
/// installs an issuer in Phase C — at that point this fallback no longer
/// fires in daemon builds.
fn generate_legacy_bridge_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Per-spawn CC settings file. Holds the path to a `settings.json` that CC
/// reads via `--settings <path>` and removes it on drop.
///
/// Lifetime mirrors `BridgeMcpConfig`: the file lives next to the daemon's
/// bridge socket (under `<home>/run/`) for the duration of a single CC
/// invocation. Per-spawn rather than per-agent because settings are static
/// policy and a fresh write per spawn means zero stale-state surface and
/// zero concurrency concerns when an agent spawns multiple CC subprocesses.
struct CcSettingsFile {
    path: PathBuf,
}

impl CcSettingsFile {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for CcSettingsFile {
    fn drop(&mut self) {
        // Best-effort cleanup; the file contains no secrets — only a static
        // deny set — so staleness is harmless if removal fails.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Build the per-spawn CC `settings.json` JSON document. Pure — no env
/// reads, no filesystem writes — so it's tractable to test in isolation.
///
/// Wire shape matches what `claude --settings <file>` accepts: a top-level
/// `permissions` object with a `deny` array of tool-name patterns. We emit
/// nothing else; merging precedence in CC is additive for permissions
/// arrays, so this layers cleanly over any user/managed settings without
/// clobbering them. Deny is monotone — adding entries can only further
/// restrict the surface — so it's safe to merge.
fn build_cc_settings_value(deny: &[&str]) -> serde_json::Value {
    let deny_arr = serde_json::Value::Array(
        deny.iter()
            .map(|s| serde_json::Value::String((*s).into()))
            .collect(),
    );
    let mut perms = serde_json::Map::new();
    perms.insert("deny".into(), deny_arr);
    let mut root = serde_json::Map::new();
    root.insert("permissions".into(), serde_json::Value::Object(perms));
    serde_json::Value::Object(root)
}

/// Materialize a per-spawn CC settings file containing the OpenFang deny
/// set, returning an RAII handle that removes the file on drop.
///
/// Gated on `bridge_enabled()` so the deny set is only injected when
/// OpenFang is in authoritative control of this CC subprocess. Without the
/// bridge wired, the agent has no `mcp__openfang__*` surface to fall back
/// on, so denying the native surface would yield a useless agent. Either
/// both (gated bridge + denied native) or neither — the two halves of the
/// "OpenFang is the RBAC layer" claim travel together.
///
/// Returns `None` if the bridge gate is off, the socket dir can't be
/// located, or the write fails. CC then spawns without `--settings`, which
/// is the pre-13a behavior (native tools open). The warn log on write
/// failure makes that regression observable rather than silent.
///
/// Co-located with the bridge config under the same socket dir so cleanup
/// is uniform: one directory, one removal pattern, one set of permissions.
fn try_materialize_cc_settings(caller_agent_id: Option<&str>) -> Option<CcSettingsFile> {
    // Same gate as the bridge config — the two are policy-coupled (see
    // function-level docstring) and must turn on or off together.
    if !bridge_enabled() {
        return None;
    }

    let socket = std::env::var(BRIDGE_SOCKET_ENV).ok()?;
    let socket_dir = std::path::Path::new(&socket).parent()?.to_path_buf();
    let path = socket_dir.join(format!("cc-settings-{}.json", uuid::Uuid::new_v4()));

    let cfg = build_cc_settings_value(CC_NATIVE_DENY);
    let serialized = serde_json::to_string(&cfg).ok()?;
    if let Err(e) = std::fs::write(&path, serialized) {
        warn!(error = %e, path = %path.display(), "failed to write CC --settings file");
        return None;
    }

    // 0600 for symmetry with the bridge config. The file contains no
    // secrets, but a uniform permission model is easier to audit than
    // exceptions per artifact type.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }
    }

    debug!(
        agent_id = %caller_agent_id.unwrap_or("<none>"),
        settings = %path.display(),
        deny_count = CC_NATIVE_DENY.len(),
        "materialized CC --settings deny set"
    );

    Some(CcSettingsFile { path })
}

impl ClaudeCodeDriver {
    /// Build the per-spawn `--mcp-config` JSON for a CC invocation, if the
    /// daemon has published bridge wiring discovery and the request carries
    /// a caller identity. Returns `None` when bridge wiring is unavailable —
    /// CC is then spawned without an OpenFang MCP server, exactly as before
    /// step 4. Logs at `info` level on first wire and `debug` per-spawn so
    /// operators can see whether a given run was bridge-enabled.
    ///
    /// Token sourcing:
    /// - If `self.token_issuer` is `Some`, the token is daemon-issued via
    ///   `TokenIssuer::issue`. The returned [`SpawnGuard`] is stashed in
    ///   the returned `BridgeMcpConfig` so its `Drop` evicts the entry
    ///   from the daemon's spawn table when the config drops.
    /// - If `self.token_issuer` is `None` (dev builds / tests), the token
    ///   is a random UUID. The daemon, in that build, treats any
    ///   non-empty token as authenticated — this is the ANAI-30 surface
    ///   that ANAI-31 replaces in production.
    fn try_build_bridge_mcp_config(
        &self,
        caller_agent_id: Option<&str>,
        allowed_tools: Option<&[String]>,
    ) -> Option<BridgeMcpConfig> {
        // Gate first — cheapest check, and when off we want zero side effects:
        // no temp file, no token generation, no log line beyond trace level.
        if !bridge_enabled() {
            return None;
        }
        let agent_id_str = caller_agent_id?;
        let socket = std::env::var(BRIDGE_SOCKET_ENV).ok()?;
        let bridge_bin = std::env::var(BRIDGE_BIN_ENV).ok()?;

        // Daemon-issued token path (production): ask the authority for a
        // fresh spawn slot. If the caller's agent_id string fails to parse
        // as a UUID, refuse to wire the bridge — that's a programmer error
        // upstream and we shouldn't silently fall through to a random
        // token that the daemon can't tie back to anything.
        let (token_hex, guard) = match self.token_issuer.as_ref() {
            Some(issuer) => {
                let parsed = match AgentId::from_str(agent_id_str) {
                    Ok(id) => id,
                    Err(e) => {
                        warn!(
                            error = %e,
                            agent_id = %agent_id_str,
                            "bridge token issuer present but caller_agent_id is not a valid UUID; \
                             refusing to wire bridge (programmer error upstream)"
                        );
                        return None;
                    }
                };
                let g = issuer.issue(parsed);
                (g.token().to_hex(), Some(g))
            }
            None => (generate_legacy_bridge_token(), None),
        };

        let cfg = build_bridge_mcp_config_value(
            &socket,
            &bridge_bin,
            agent_id_str,
            &token_hex,
            allowed_tools,
        );

        // Place the config next to the socket so cleanup is colocated and the
        // bridge socket dir already exists with the right permissions.
        let socket_dir = std::path::Path::new(&socket).parent()?.to_path_buf();
        let path = socket_dir.join(format!("cc-mcp-{}.json", uuid::Uuid::new_v4()));

        let serialized = serde_json::to_string(&cfg).ok()?;
        if let Err(e) = std::fs::write(&path, serialized) {
            warn!(error = %e, path = %path.display(), "failed to write CC mcp-config");
            return None;
        }

        // 0600 — file contains a per-spawn auth token. No other uid should read it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, perms);
            }
        }

        debug!(
            agent_id = %agent_id_str,
            config = %path.display(),
            fingerprint = %guard.as_ref().map(|g| g.fingerprint()).unwrap_or_else(|| "<legacy>".into()),
            "wired CC --mcp-config for OpenFang bridge"
        );

        Some(BridgeMcpConfig {
            config_path: path,
            _guard: guard,
        })
    }
}

/// JSON output from `claude -p --output-format json`.
///
/// The CLI may return the response text in different fields depending on
/// version: `result`, `content`, or `text`. We try all three.
/// All fields use `#[serde(default)]` so deserialization never fails on
/// missing keys — older and newer CLI versions differ in which fields are emitted.
#[derive(Debug, Deserialize)]
struct ClaudeJsonOutput {
    // Fix: `result` now has #[serde(default)] so deserialization succeeds
    // even when the CLI emits the response in `content` or `text` instead.
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
    #[serde(default)]
    #[allow(dead_code)]
    cost_usd: Option<f64>,
}

/// Usage stats from Claude CLI JSON output.
#[derive(Debug, Deserialize, Default)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// A single content block inside an `assistant` stream-json event.
/// The CLI emits `{"type":"text","text":"..."}` blocks inside `message.content`.
#[derive(Debug, Deserialize, Default)]
struct ClaudeMessageBlock {
    #[serde(default, rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

/// Nested `message` object carried by `type=assistant` stream-json events.
#[derive(Debug, Deserialize, Default)]
struct ClaudeAssistantMessage {
    #[serde(default)]
    content: Vec<ClaudeMessageBlock>,
}

/// Stream JSON event from `claude -p --output-format stream-json --verbose`.
///
/// Newer CLI versions (≥2.x) carry the response text inside the nested
/// `message.content[].text` of `type=assistant` events rather than a
/// flat `content` string.  Both layouts are handled here so that real-time
/// token streaming works across CLI versions.
#[derive(Debug, Deserialize)]
struct ClaudeStreamEvent {
    #[serde(default)]
    r#type: String,
    /// Flat content string — used by older CLI versions and some event types.
    #[serde(default)]
    content: Option<String>,
    /// Final result text carried by `type=result` events.
    #[serde(default)]
    result: Option<String>,
    /// Nested assistant message — used by newer CLI `type=assistant` events.
    #[serde(default)]
    message: Option<ClaudeAssistantMessage>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[async_trait]
impl LlmDriver for ClaudeCodeDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json");

        if let Some(ref sys) = request.system {
            cmd.arg("--system-prompt").arg(sys);
        }

        if self.skip_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        // Wire the OpenFang MCP bridge if the daemon has published discovery
        // env vars (`OPENFANG_BRIDGE_SOCKET` + `OPENFANG_BRIDGE_BIN`) and
        // the request carries a caller identity. The guard lives for the
        // remainder of the call; on drop it removes the temp config file.
        let _bridge_cfg = self
            .try_build_bridge_mcp_config(
                request.caller_agent_id.as_deref(),
                request.allowed_tools.as_deref(),
            )
            .inspect(|cfg| {
                cmd.arg("--mcp-config").arg(cfg.path());
                // `--strict-mcp-config` makes CC ignore any user/global MCP
                // config that might otherwise merge in — we want exactly
                // the OpenFang bridge for this invocation, nothing else.
                cmd.arg("--strict-mcp-config");
                // Optional diagnostic: with `OPENFANG_BRIDGE_DEBUG=1` we add
                // `--debug` so CC writes MCP launch + handshake details into
                // `~/.claude/debug/<uuid>.txt`. Off by default — daemon-side
                // bridge_ipc INFO logs are the supported observability path.
                if bridge_debug_enabled() {
                    cmd.arg("--debug");
                }
            });
        let bridge_wired = _bridge_cfg.is_some();
        let bridge_debug = bridge_wired && bridge_debug_enabled();

        // Inject the per-spawn settings.json that denies CC's native FS,
        // shell, and web tools. Guard kept alive for the remainder of the
        // call; on drop it removes the temp settings file. Gated on
        // `bridge_enabled()` inside `try_materialize_cc_settings` so it
        // co-travels with the bridge wiring above — either both or neither.
        let _cc_settings =
            try_materialize_cc_settings(request.caller_agent_id.as_deref()).inspect(|s| {
                cmd.arg("--settings").arg(s.path());
            });
        let native_deny_wired = _cc_settings.is_some();

        Self::apply_env_filter(&mut cmd);

        // Inject HOME so the CLI can find its credentials (~/.claude/) when
        // OpenFang runs as a service without a login shell.
        if let Some(home) = home_dir() {
            cmd.env("HOME", &home);
        }
        // Detach stdin so the CLI does not block waiting for interactive input.
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, skip_permissions = self.skip_permissions, bridge_wired, native_deny_wired, "Spawning Claude Code CLI");

        // Spawn child process instead of cmd.output() so we can track PID and timeout
        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "Claude Code CLI not found or failed to start ({}). \
                 Install: npm install -g @anthropic-ai/claude-code && claude auth",
                e
            ))
        })?;

        // Track the PID using the model name as label (best identifier available)
        let pid_label = request.model.clone();
        if let Some(pid) = child.id() {
            self.active_pids.insert(pid_label.clone(), pid);
            debug!(pid = pid, model = %pid_label, "Claude Code CLI subprocess started");
        }

        // Drain stdout and stderr concurrently while waiting for the process.
        // Sequential drain (wait → read) deadlocks when the subprocess writes
        // more than the OS pipe buffer (~64 KB): the child blocks on write,
        // child.wait() never returns, the timeout fires, and output is lost.
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut out) = child_stdout {
                let _ = out.read_to_end(&mut buf).await;
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut err) = child_stderr {
                let _ = err.read_to_end(&mut buf).await;
            }
            buf
        });

        // Wait with timeout
        let timeout_duration = std::time::Duration::from_secs(self.message_timeout_secs);
        let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

        // Collect pipe output — tasks complete once the process closes its end
        let stdout_bytes = stdout_task.await.unwrap_or_default();
        let stderr_bytes = stderr_task.await.unwrap_or_default();

        // Clear PID tracking regardless of outcome
        self.active_pids.remove(&pid_label);

        let status = match wait_result {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                warn!(error = %e, model = %pid_label, "Claude Code CLI subprocess failed");
                return Err(LlmError::Http(format!(
                    "Claude Code CLI subprocess failed: {e}"
                )));
            }
            Err(_elapsed) => {
                // Timeout — kill the process
                warn!(
                    timeout_secs = self.message_timeout_secs,
                    model = %pid_label,
                    "Claude Code CLI subprocess timed out, killing process"
                );
                let _ = child.kill().await;
                return Err(LlmError::Http(format!(
                    "Claude Code CLI subprocess timed out after {}s — process killed",
                    self.message_timeout_secs
                )));
            }
        };

        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            let stdout_str = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
            let detail = if !stderr.is_empty() {
                &stderr
            } else {
                &stdout_str
            };
            let code = status.code().unwrap_or(1);

            warn!(
                exit_code = code,
                model = %pid_label,
                stderr = %detail,
                "Claude Code CLI exited with error"
            );

            // Provide actionable error messages
            let message = if detail.contains("not authenticated")
                || detail.contains("auth")
                || detail.contains("login")
                || detail.contains("credentials")
            {
                format!("Claude Code CLI is not authenticated. Run: claude auth\nDetail: {detail}")
            } else if detail.contains("permission")
                || detail.contains("--dangerously-skip-permissions")
            {
                format!(
                    "Claude Code CLI requires permissions acceptance. \
                     Run: claude --dangerously-skip-permissions (once to accept)\nDetail: {detail}"
                )
            } else {
                format!("Claude Code CLI exited with code {code}: {detail}")
            };

            return Err(LlmError::Api {
                status: code as u16,
                message,
            });
        }

        info!(model = %pid_label, "Claude Code CLI subprocess completed successfully");

        // Optional diagnostic: when bridge debug is enabled, log a tail of
        // CC's stderr (with --debug it contains MCP launch/handshake info).
        // Bounded to 4KB so we don't blow up logs. Off by default — see
        // `bridge_debug_enabled()`.
        if bridge_debug {
            let stderr_text = String::from_utf8_lossy(&stderr_bytes);
            let tail: String = stderr_text
                .chars()
                .rev()
                .take(4096)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            info!(
                model = %pid_label,
                stderr_tail = %tail.trim(),
                "CC stderr tail (bridge wired, --debug)"
            );
        }

        let stdout = String::from_utf8_lossy(&stdout_bytes);

        // Try JSON parse first
        if let Ok(parsed) = serde_json::from_str::<ClaudeJsonOutput>(&stdout) {
            let text = parsed
                .result
                .or(parsed.content)
                .or(parsed.text)
                .unwrap_or_default();
            let usage = parsed.usage.unwrap_or_default();
            return Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: text.clone(),
                    provider_metadata: None,
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: Vec::new(),
                usage: TokenUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                },
            });
        }

        // Fallback: treat entire stdout as plain text
        let text = stdout.trim().to_string();
        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");

        if let Some(ref sys) = request.system {
            cmd.arg("--system-prompt").arg(sys);
        }

        if self.skip_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        // Bridge wiring (see `complete()` for full rationale). Guard kept
        // alive for the rest of the streaming function so the per-spawn
        // config file outlives the CC subprocess.
        let _bridge_cfg = self
            .try_build_bridge_mcp_config(
                request.caller_agent_id.as_deref(),
                request.allowed_tools.as_deref(),
            )
            .inspect(|cfg| {
                cmd.arg("--mcp-config").arg(cfg.path());
                cmd.arg("--strict-mcp-config");
                // Optional --debug — see complete() for rationale.
                if bridge_debug_enabled() {
                    cmd.arg("--debug");
                }
            });
        let bridge_wired = _bridge_cfg.is_some();
        let bridge_debug = bridge_wired && bridge_debug_enabled();

        // Native-tool deny via per-spawn `--settings` (see `complete()` for
        // full rationale). Guard kept alive for the rest of the streaming
        // function so the settings file outlives the CC subprocess.
        let _cc_settings =
            try_materialize_cc_settings(request.caller_agent_id.as_deref()).inspect(|s| {
                cmd.arg("--settings").arg(s.path());
            });
        let native_deny_wired = _cc_settings.is_some();

        Self::apply_env_filter(&mut cmd);

        // Same HOME and stdin hygiene as the non-streaming path.
        if let Some(home) = home_dir() {
            cmd.env("HOME", &home);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, bridge_wired, native_deny_wired, "Spawning Claude Code CLI (streaming)");

        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "Claude Code CLI not found or failed to start ({}). \
                 Install: npm install -g @anthropic-ai/claude-code && claude auth",
                e
            ))
        })?;

        // Track PID
        let pid_label = format!("{}-stream", request.model);
        if let Some(pid) = child.id() {
            self.active_pids.insert(pid_label.clone(), pid);
            debug!(pid = pid, model = %pid_label, "Claude Code CLI streaming subprocess started");
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            self.active_pids.remove(&pid_label);
            LlmError::Http("No stdout from claude CLI".to_string())
        })?;

        // Drain stderr concurrently with stdout. Required whenever `--debug`
        // is on (chatty CC can otherwise deadlock on a full stderr pipe once
        // the OS buffer fills, ~64 KB). Cheap when --debug is off, so we
        // unconditionally drain — keeps the streaming path uniform.
        let child_stderr = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut err) = child_stderr {
                let _ = err.read_to_end(&mut buf).await;
            }
            buf
        });

        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut full_text = String::new();
        let mut final_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };

        let timeout_duration = std::time::Duration::from_secs(self.message_timeout_secs);
        let stream_result = tokio::time::timeout(timeout_duration, async {
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<ClaudeStreamEvent>(&line) {
                    Ok(event) => {
                        match event.r#type.as_str() {
                            "content" | "text" | "assistant" | "content_block_delta" => {
                                // Older CLI: flat `content` string.
                                // CLI ≥2.x (type=assistant): text is nested in
                                // `message.content[].text`; the flat `content`
                                // field is absent or null.
                                let chunk = event.content.clone().unwrap_or_default();
                                let nested: String = event
                                    .message
                                    .as_ref()
                                    .map(|msg| {
                                        msg.content
                                            .iter()
                                            .filter(|b| b.block_type == "text")
                                            .map(|b| b.text.as_str())
                                            .collect::<Vec<_>>()
                                            .join("")
                                    })
                                    .unwrap_or_default();
                                let text_chunk = if !chunk.is_empty() { chunk } else { nested };
                                if !text_chunk.is_empty() {
                                    full_text.push_str(&text_chunk);
                                    let _ =
                                        tx.send(StreamEvent::TextDelta { text: text_chunk }).await;
                                }
                            }
                            "result" | "done" | "complete" => {
                                if let Some(ref result) = event.result {
                                    if full_text.is_empty() {
                                        full_text = result.clone();
                                        let _ = tx
                                            .send(StreamEvent::TextDelta {
                                                text: result.clone(),
                                            })
                                            .await;
                                    }
                                }
                                if let Some(usage) = event.usage {
                                    final_usage = TokenUsage {
                                        input_tokens: usage.input_tokens,
                                        output_tokens: usage.output_tokens,
                                    };
                                }
                            }
                            _ => {
                                // Unknown event type — try content field as fallback
                                if let Some(ref content) = event.content {
                                    full_text.push_str(content);
                                    let _ = tx
                                        .send(StreamEvent::TextDelta {
                                            text: content.clone(),
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Not valid JSON — treat as raw text
                        warn!(line = %line, error = %e, "Non-JSON line from Claude CLI");
                        full_text.push_str(&line);
                        let _ = tx.send(StreamEvent::TextDelta { text: line }).await;
                    }
                }
            }
        })
        .await;

        // Clear PID tracking
        self.active_pids.remove(&pid_label);

        if stream_result.is_err() {
            warn!(
                timeout_secs = self.message_timeout_secs,
                model = %pid_label,
                "Claude Code CLI streaming subprocess timed out, killing process"
            );
            let _ = child.kill().await;
            return Err(LlmError::Http(format!(
                "Claude Code CLI streaming subprocess timed out after {}s — process killed",
                self.message_timeout_secs
            )));
        }

        // Wait for process to finish
        let status = child
            .wait()
            .await
            .map_err(|e| LlmError::Http(format!("Claude CLI wait failed: {e}")))?;

        // Stderr was being drained concurrently — collect it now.
        let stderr_bytes = stderr_task.await.unwrap_or_default();
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();

        if !status.success() {
            let code = status.code().unwrap_or(1);
            warn!(
                exit_code = code,
                model = %pid_label,
                stderr = %stderr_text,
                "Claude Code CLI streaming subprocess exited with error"
            );
            return Err(LlmError::Api {
                status: code as u16,
                message: format!(
                    "Claude Code CLI streaming exited with code {code}: {}",
                    if stderr_text.is_empty() {
                        "no stderr"
                    } else {
                        &stderr_text
                    }
                ),
            });
        }

        // Optional diagnostic: log CC stderr tail when bridge debug is on.
        if bridge_debug {
            let tail: String = stderr_text
                .chars()
                .rev()
                .take(4096)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            info!(
                model = %pid_label,
                stderr_tail = %tail,
                "CC stderr tail (streaming, bridge wired, --debug)"
            );
        }

        let _ = tx
            .send(StreamEvent::ContentComplete {
                stop_reason: StopReason::EndTurn,
                usage: final_usage,
            })
            .await;

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text: full_text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: final_usage,
        })
    }
}

/// Check if the Claude Code CLI is available.
pub fn claude_code_available() -> bool {
    ClaudeCodeDriver::detect().is_some() || claude_credentials_exist()
}

/// Check if Claude credentials file exists.
///
/// Different Claude CLI versions store credentials at different paths:
/// - `~/.claude/.credentials.json` (older versions)
/// - `~/.claude/credentials.json` (newer versions)
fn claude_credentials_exist() -> bool {
    if let Some(home) = home_dir() {
        let claude_dir = home.join(".claude");
        claude_dir.join(".credentials.json").exists()
            || claude_dir.join("credentials.json").exists()
    } else {
        false
    }
}

/// Cross-platform home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_simple() {
        use openfang_types::message::{Message, MessageContent};

        let request = CompletionRequest {
            model: "claude-code/sonnet".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("Hello"),
                ..Default::default()
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: Some("You are helpful.".to_string()),
            thinking: None,
            caller_agent_id: None,
            allowed_tools: None,
        };

        let prompt = ClaudeCodeDriver::build_prompt(&request);
        assert!(!prompt.contains("[System]"));
        assert!(!prompt.contains("You are helpful."));
        assert!(prompt.contains("[User]"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn test_build_prompt_renders_image_attachment_marker() {
        use openfang_types::message::{ContentBlock, Message, MessageContent};

        // ~12 KB of base64 — decoded ~9 KB.
        let fake_b64 = "A".repeat(12 * 1024);
        let request = CompletionRequest {
            model: "claude-code/sonnet".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Blocks(vec![
                    ContentBlock::Text {
                        text: "what's in this?".to_string(),
                        provider_metadata: None,
                    },
                    ContentBlock::Image {
                        media_type: "image/png".to_string(),
                        data: fake_b64,
                    },
                ]),
                ..Default::default()
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: None,
            thinking: None,
            caller_agent_id: None,
            allowed_tools: None,
        };

        let prompt = ClaudeCodeDriver::build_prompt(&request);
        assert!(prompt.contains("what's in this?"), "text preserved");
        assert!(
            prompt.contains("[attachment: image/png image"),
            "image rendered as synthetic attachment marker, got: {prompt}"
        );
        assert!(
            prompt.contains("not viewable on this provider"),
            "marker explains the limitation, got: {prompt}"
        );
    }

    #[test]
    fn test_build_prompt_image_only_still_emits_marker() {
        use openfang_types::message::{ContentBlock, Message, MessageContent};

        let request = CompletionRequest {
            model: "claude-code/sonnet".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Blocks(vec![ContentBlock::Image {
                    media_type: "image/jpeg".to_string(),
                    data: "Zm9v".to_string(),
                }]),
                ..Default::default()
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: None,
            thinking: None,
            caller_agent_id: None,
            allowed_tools: None,
        };

        let prompt = ClaudeCodeDriver::build_prompt(&request);
        assert!(
            prompt.contains("[User]"),
            "user role label emitted even with image-only content, got: {prompt}"
        );
        assert!(
            prompt.contains("[attachment: image/jpeg image"),
            "bare image renders marker, got: {prompt}"
        );
    }

    #[test]
    fn test_model_flag_mapping() {
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/opus"),
            Some("opus".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/sonnet"),
            Some("sonnet".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/haiku"),
            Some("haiku".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("custom-model"),
            Some("custom-model".to_string())
        );
    }

    #[test]
    fn test_new_defaults_to_claude() {
        let driver = ClaudeCodeDriver::new(None, true);
        assert_eq!(driver.cli_path, "claude");
        assert_eq!(driver.message_timeout_secs, DEFAULT_MESSAGE_TIMEOUT_SECS);
        assert!(driver.active_pids().is_empty());
    }

    #[test]
    fn test_new_with_custom_path() {
        let driver = ClaudeCodeDriver::new(Some("/usr/local/bin/claude".to_string()), true);
        assert_eq!(driver.cli_path, "/usr/local/bin/claude");
    }

    #[test]
    fn test_new_with_empty_path() {
        let driver = ClaudeCodeDriver::new(Some(String::new()), true);
        assert_eq!(driver.cli_path, "claude");
    }

    #[test]
    fn test_with_timeout() {
        let driver = ClaudeCodeDriver::with_timeout(None, true, 600);
        assert_eq!(driver.message_timeout_secs, 600);
        assert_eq!(driver.cli_path, "claude");
    }

    #[test]
    fn test_pid_map_shared() {
        let driver = ClaudeCodeDriver::new(None, true);
        let map = driver.pid_map();
        map.insert("test-agent".to_string(), 12345);
        assert_eq!(driver.active_pids().len(), 1);
        assert_eq!(driver.active_pids()[0], ("test-agent".to_string(), 12345));
    }

    #[test]
    fn test_apply_env_filter_strips_bridge_discovery_vars() {
        // Verifies the filter removes every bridge-discovery var so a
        // CC subprocess can't accidentally inherit them. The bridge gets
        // these via `--mcp-config`'s `env` map only.
        let mut cmd = tokio::process::Command::new("/bin/true");
        cmd.env(BRIDGE_SOCKET_ENV, "/tmp/should-not-survive.sock");
        cmd.env(BRIDGE_BIN_ENV, "/usr/local/bin/should-not-survive");
        cmd.env(BRIDGE_TOKEN_ENV, "should-not-survive");
        cmd.env(BRIDGE_AGENT_ID_ENV, "should-not-survive");
        cmd.env(BRIDGE_ALLOWED_ENV, "file_read,agent_send");

        ClaudeCodeDriver::apply_env_filter(&mut cmd);

        // tokio's Command exposes its env via std::process::Command::get_envs()
        // through deref. Walk it; any of the bridge vars present means
        // the filter is broken.
        let std_cmd = cmd.as_std();
        for (key, value) in std_cmd.get_envs() {
            // None means "remove this env var on spawn"; any of our keys
            // showing up with Some means the filter missed them.
            let key_str = key.to_string_lossy();
            if matches!(
                key_str.as_ref(),
                BRIDGE_SOCKET_ENV
                    | BRIDGE_BIN_ENV
                    | BRIDGE_TOKEN_ENV
                    | BRIDGE_AGENT_ID_ENV
                    | BRIDGE_ALLOWED_ENV
            ) {
                assert!(
                    value.is_none(),
                    "bridge env var {key_str} survived apply_env_filter as {value:?}"
                );
            }
        }
    }

    #[test]
    fn test_build_bridge_mcp_config_shape() {
        let cfg = build_bridge_mcp_config_value(
            "/home/user/.openfang/run/bridge.sock",
            "/usr/local/bin/openfang-mcp-bridge",
            "agent-uuid-1234",
            "tok-abc",
            None,
        );

        // mcpServers.openfang.{command,args,env} all present with the
        // shape claude --mcp-config expects.
        let server = cfg
            .pointer("/mcpServers/openfang")
            .expect("openfang server entry missing");
        assert_eq!(
            server.pointer("/command").and_then(|v| v.as_str()),
            Some("/usr/local/bin/openfang-mcp-bridge")
        );
        assert!(
            server
                .pointer("/args")
                .map(|v| v.is_array())
                .unwrap_or(false),
            "args must be a JSON array"
        );

        // env carries exactly the three discovery vars when no per-agent
        // allowlist is supplied. No more, no less — any extras would leak
        // unintended state into the bridge process.
        let env = server
            .pointer("/env")
            .and_then(|v| v.as_object())
            .expect("env object missing");
        assert_eq!(
            env.len(),
            3,
            "env must contain exactly socket/token/agent_id"
        );
        assert_eq!(
            env.get(BRIDGE_SOCKET_ENV).and_then(|v| v.as_str()),
            Some("/home/user/.openfang/run/bridge.sock")
        );
        assert_eq!(
            env.get(BRIDGE_TOKEN_ENV).and_then(|v| v.as_str()),
            Some("tok-abc")
        );
        assert_eq!(
            env.get(BRIDGE_AGENT_ID_ENV).and_then(|v| v.as_str()),
            Some("agent-uuid-1234")
        );
        assert!(
            env.get(BRIDGE_ALLOWED_ENV).is_none(),
            "no allowlist supplied → OPENFANG_BRIDGE_ALLOWED must be absent so the bridge \
             falls back to its hard-coded DEFAULT_ALLOWED"
        );
    }

    #[test]
    fn test_build_bridge_mcp_config_emits_allowed_tools() {
        // When the caller supplies a per-agent allowlist, it lands in the
        // env map as a comma-separated OPENFANG_BRIDGE_ALLOWED entry. This
        // is the channel that lets the bridge's tool surface track each
        // agent's `agent.toml` capabilities instead of the static default.
        let cfg = build_bridge_mcp_config_value(
            "/sock",
            "/bin",
            "agent-uuid",
            "tok",
            Some(&[
                "file_read".to_string(),
                "agent_send".to_string(),
                "channel_send".to_string(),
            ]),
        );
        let env = cfg
            .pointer("/mcpServers/openfang/env")
            .and_then(|v| v.as_object())
            .expect("env object missing");
        assert_eq!(env.len(), 4, "env must add OPENFANG_BRIDGE_ALLOWED");
        assert_eq!(
            env.get(BRIDGE_ALLOWED_ENV).and_then(|v| v.as_str()),
            Some("file_read,agent_send,channel_send"),
        );
    }

    #[test]
    fn test_build_bridge_mcp_config_emits_empty_allowed_tools_explicitly() {
        // Empty list means "no bridge tools permitted" — emit the env var
        // as the empty string so the bridge's parser produces a zero-tool
        // surface instead of silently falling back to DEFAULT_ALLOWED.
        let cfg = build_bridge_mcp_config_value("/sock", "/bin", "agent-uuid", "tok", Some(&[]));
        let env = cfg
            .pointer("/mcpServers/openfang/env")
            .and_then(|v| v.as_object())
            .expect("env object missing");
        assert_eq!(
            env.get(BRIDGE_ALLOWED_ENV).and_then(|v| v.as_str()),
            Some(""),
            "empty allowlist must still emit the env var (empty string), not be absent"
        );
    }

    #[test]
    fn test_bridge_mcp_config_drop_removes_file() {
        // BridgeMcpConfig is a per-spawn token holder; on drop, the file
        // must vanish so a stale token can't be reused by anything that
        // happens to glob `<home>/run/cc-mcp-*.json`.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cc-mcp-test.json");
        std::fs::write(&path, "{}").expect("seed file");
        assert!(path.exists());

        {
            let _guard = BridgeMcpConfig {
                config_path: path.clone(),
                _guard: None,
            };
            assert!(path.exists(), "file present while guard held");
        }

        assert!(!path.exists(), "file must be removed when guard drops");
    }

    #[test]
    fn test_build_cc_settings_shape() {
        // The wire shape CC's `--settings` accepts: top-level `permissions`
        // object with a `deny` array of bare tool-name strings. No other
        // keys are emitted — we want minimal surface that merges cleanly
        // with any user/managed settings without subtracting from them.
        let cfg = build_cc_settings_value(CC_NATIVE_DENY);
        let root = cfg.as_object().expect("root must be a JSON object");
        assert_eq!(root.len(), 1, "root must contain only `permissions`");

        let perms = cfg
            .pointer("/permissions")
            .and_then(|v| v.as_object())
            .expect("permissions object missing");
        assert_eq!(perms.len(), 1, "permissions must contain only `deny`");

        let deny = cfg
            .pointer("/permissions/deny")
            .and_then(|v| v.as_array())
            .expect("deny array missing");
        assert_eq!(deny.len(), CC_NATIVE_DENY.len());
        // Bare tool names — no specifier in parens — so the deny is
        // blanket, not pattern-scoped. CC treats `"Bash"` as the whole tool.
        for entry in deny {
            let s = entry.as_str().expect("deny entries must be strings");
            assert!(
                !s.contains('(') && !s.contains(')'),
                "{s:?} must be a bare tool name, not a specifier pattern"
            );
        }
    }

    #[test]
    fn test_cc_native_deny_includes_glob_grep() {
        // Glob and Grep are included for symmetry: any FS-read path goes
        // through the bridge's sandboxed `file_read`/`file_list` rather
        // than CC's native readers. This test pins that decision so a
        // future refactor that drops them needs a deliberate change.
        assert!(CC_NATIVE_DENY.contains(&"Glob"), "Glob must be denied");
        assert!(CC_NATIVE_DENY.contains(&"Grep"), "Grep must be denied");

        // Core dangerous tools — the load-bearing reason for this commit.
        for must_deny in [
            "Bash",
            "Read",
            "Write",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
        ] {
            assert!(
                CC_NATIVE_DENY.contains(&must_deny),
                "{must_deny} must be denied"
            );
        }

        // Agent-internal control flow must NOT be denied — denying these
        // would break legitimate agent loops with no security upside.
        for must_allow in [
            "TodoWrite",
            "Task",
            "Skill",
            "AskUserQuestion",
            "EnterPlanMode",
            "ExitPlanMode",
            "TaskOutput",
            "TaskStop",
            "ToolSearch",
        ] {
            assert!(
                !CC_NATIVE_DENY.contains(&must_allow),
                "{must_allow} must NOT be denied (agent-internal control flow)"
            );
        }
    }

    #[test]
    fn test_cc_native_deny_covers_audit_gaps() {
        // Commit 18: the deny-set audit closed gaps in five categories.
        // Pin each addition so a future refactor that drops one needs a
        // deliberate change with this test as the canary.

        // Bash adjuncts — inert today (Bash is denied) but locked down
        // as defense in depth against future CC backgrounding paths.
        for must_deny in ["BashOutput", "KillShell", "KillBash", "Monitor"] {
            assert!(
                CC_NATIVE_DENY.contains(&must_deny),
                "{must_deny} must be denied (Bash adjunct / shell streamer)"
            );
        }

        // Worktree creation — direct FS-escape primitive.
        for must_deny in ["EnterWorktree", "ExitWorktree"] {
            assert!(
                CC_NATIVE_DENY.contains(&must_deny),
                "{must_deny} must be denied (FS escape via git worktree)"
            );
        }

        // Notebook read — symmetry with NotebookEdit + forward-compat.
        assert!(
            CC_NATIVE_DENY.contains(&"NotebookRead"),
            "NotebookRead must be denied (forward-compat with NotebookEdit)"
        );

        // SlashCommand — parallel skill substrate; OF's is canonical.
        assert!(
            CC_NATIVE_DENY.contains(&"SlashCommand"),
            "SlashCommand must be denied (parallel skill curation surface)"
        );

        // Scheduling / remote control plane — OpenFang-First.
        for must_deny in [
            "CronCreate",
            "CronDelete",
            "CronList",
            "ScheduleWakeup",
            "RemoteTrigger",
        ] {
            assert!(
                CC_NATIVE_DENY.contains(&must_deny),
                "{must_deny} must be denied (OpenFang-First: scheduling owned by OF)"
            );
        }

        // Comms routing — keep on canonical OF path.
        assert!(
            CC_NATIVE_DENY.contains(&"PushNotification"),
            "PushNotification must be denied (OF channel routing is canonical)"
        );
    }

    #[test]
    fn test_cc_settings_file_drop_removes_file() {
        // CcSettingsFile is a per-spawn artifact; on drop, the file must
        // vanish so successive runs don't accumulate stale settings
        // sidecars under the socket dir.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cc-settings-test.json");
        std::fs::write(&path, "{}").expect("seed file");
        assert!(path.exists());

        {
            let _guard = CcSettingsFile { path: path.clone() };
            assert!(path.exists(), "file present while guard held");
        }

        assert!(!path.exists(), "file must be removed when guard drops");
    }

    #[test]
    fn test_bridge_enabled_gate() {
        // Single test exercises the whole truth table for the gate, in
        // sequence, because `OPENFANG_BRIDGE_ENABLED` is process-global.
        // No other test reads or writes this var, so we don't need
        // serial_test infrastructure — just be a good citizen and
        // restore the original value on exit.
        let original = std::env::var(BRIDGE_ENABLED_ENV).ok();

        // Unset → off.
        std::env::remove_var(BRIDGE_ENABLED_ENV);
        assert!(!bridge_enabled(), "unset must read as off");

        // Truthy values.
        for v in ["1", "true", "TRUE", "True"] {
            std::env::set_var(BRIDGE_ENABLED_ENV, v);
            assert!(bridge_enabled(), "{v} must read as on");
        }

        // Anything else is off — including `2`, empty, garbage.
        for v in ["0", "false", "False", "", "yes", "on", "garbage"] {
            std::env::set_var(BRIDGE_ENABLED_ENV, v);
            assert!(!bridge_enabled(), "{v:?} must read as off");
        }

        // Even with full bridge wiring published, the gate alone suppresses
        // config generation. We don't assert positive-path here because
        // setting BRIDGE_SOCKET_ENV/BRIDGE_BIN_ENV process-globally would
        // race with apply_env_filter tests; the shape test covers the
        // construction path. This test owns the gate behavior only.
        std::env::remove_var(BRIDGE_ENABLED_ENV);
        let driver = ClaudeCodeDriver::new(None, true);
        let cfg = driver.try_build_bridge_mcp_config(Some("agent-x"), None);
        assert!(cfg.is_none(), "gate off → None regardless of other env");

        // Restore.
        match original {
            Some(v) => std::env::set_var(BRIDGE_ENABLED_ENV, v),
            None => std::env::remove_var(BRIDGE_ENABLED_ENV),
        }
    }

    #[test]
    fn test_sensitive_env_list_coverage() {
        // Ensure all major provider keys are in the strip list
        assert!(SENSITIVE_ENV_EXACT.contains(&"OPENAI_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"ANTHROPIC_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"GEMINI_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"GROQ_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"DEEPSEEK_API_KEY"));
    }
}
