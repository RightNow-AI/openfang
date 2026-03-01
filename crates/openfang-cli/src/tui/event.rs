//! Event system: crossterm polling, tick timer, streaming bridges.

use openfang_kernel::OpenFangKernel;
use openfang_runtime::agent_loop::AgentLoopResult;
use openfang_runtime::llm_driver::StreamEvent;
use openfang_types::agent::AgentId;
use ratatui::crossterm::event::{self, Event as CtEvent, KeyEvent, KeyEventKind};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::screens::{
    audit::AuditEntry,
    channels::ChannelInfo,
    dashboard::AuditRow,
    extensions::{ExtensionHealthInfo, ExtensionInfo},
    hands::{HandInfo, HandInstanceInfo},
    logs::LogEntry,
    memory::{AgentEntry, KvPair},
    peers::PeerInfo,
    security::SecurityFeature,
    sessions::SessionInfo,
    settings::{ModelInfo, ProviderInfo, TestResult, ToolInfo},
    skills::{ClawHubResult, McpServerInfo, SkillInfo},
    templates::ProviderAuth,
    triggers::TriggerInfo,
    usage::{AgentUsage, ModelUsage, UsageSummary},
    workflows::{WorkflowInfo, WorkflowRun},
};

// ── BackendRef ──────────────────────────────────────────────────────────────

/// Lightweight reference to the active backend, for passing to spawn functions.
#[derive(Clone)]
pub enum BackendRef {
    Daemon(String),
    InProcess(Arc<OpenFangKernel>),
}

// ── AppEvent ────────────────────────────────────────────────────────────────

/// Unified application event.
pub enum AppEvent {
    /// A crossterm key press event (filtered to Press only).
    Key(KeyEvent),
    /// Periodic tick for animations (spinners, etc.).
    Tick,
    /// A streaming event from the LLM (daemon SSE or kernel mpsc).
    Stream(StreamEvent),
    /// The streaming agent loop finished.
    StreamDone(Result<AgentLoopResult, String>),
    /// The kernel finished booting in the background.
    KernelReady(Arc<OpenFangKernel>),
    /// The kernel failed to boot.
    KernelError(String),
    /// An agent was successfully spawned (daemon mode).
    AgentSpawned { id: String, name: String },
    /// Agent spawn failed.
    AgentSpawnError(String),
    /// Daemon detection result from background thread.
    DaemonDetected {
        url: Option<String>,
        agent_count: u64,
    },

    // ── New tab events ──────────────────────────────────────────────────────
    /// Dashboard data loaded.
    DashboardData {
        agent_count: u64,
        uptime_secs: u64,
        version: String,
        provider: String,
        model: String,
    },
    /// Audit trail loaded.
    AuditLoaded(Vec<AuditRow>),
    /// Channel list loaded.
    ChannelListLoaded(Vec<ChannelInfo>),
    /// Channel test result.
    ChannelTestResult { success: bool, message: String },
    /// Workflow list loaded.
    WorkflowListLoaded(Vec<WorkflowInfo>),
    /// Workflow runs loaded for a specific workflow.
    WorkflowRunsLoaded(Vec<WorkflowRun>),
    /// Workflow run completed.
    WorkflowRunResult(String),
    /// Workflow created successfully.
    WorkflowCreated(String),
    /// Trigger list loaded.
    TriggerListLoaded(Vec<TriggerInfo>),
    /// Trigger created.
    TriggerCreated(String),
    /// Trigger deleted.
    TriggerDeleted(String),
    /// Agent killed successfully.
    AgentKilled { id: String },
    /// Agent kill failed.
    AgentKillError(String),
    /// Generic fetch error for any tab.
    FetchError(String),

    // ── New screen events ──────────────────────────────────────────────────
    /// Sessions loaded.
    SessionsLoaded(Vec<SessionInfo>),
    /// Session deleted.
    SessionDeleted(String),
    /// Memory agents loaded (for agent selector).
    MemoryAgentsLoaded(Vec<AgentEntry>),
    /// Memory KV pairs loaded.
    MemoryKvLoaded(Vec<KvPair>),
    /// Memory KV saved.
    MemoryKvSaved { key: String },
    /// Memory KV deleted.
    MemoryKvDeleted(String),
    /// Skills loaded.
    SkillsLoaded(Vec<SkillInfo>),
    /// ClawHub results loaded.
    ClawHubLoaded(Vec<ClawHubResult>),
    /// Skill installed.
    SkillInstalled(String),
    /// Skill uninstalled.
    SkillUninstalled(String),
    /// MCP servers loaded.
    McpServersLoaded(Vec<McpServerInfo>),
    /// Templates providers loaded (auth status).
    TemplateProvidersLoaded(Vec<ProviderAuth>),
    /// Security features loaded.
    SecurityLoaded(Vec<SecurityFeature>),
    /// Security chain verification result.
    SecurityChainVerified { valid: bool, message: String },
    /// Audit entries loaded (full audit screen).
    AuditEntriesLoaded(Vec<AuditEntry>),
    /// Audit chain verified.
    AuditChainVerified(bool),
    /// Usage summary loaded.
    UsageSummaryLoaded(UsageSummary),
    /// Usage by model loaded.
    UsageByModelLoaded(Vec<ModelUsage>),
    /// Usage by agent loaded.
    UsageByAgentLoaded(Vec<AgentUsage>),
    /// Settings providers loaded.
    SettingsProvidersLoaded(Vec<ProviderInfo>),
    /// Settings models loaded.
    SettingsModelsLoaded(Vec<ModelInfo>),
    /// Settings tools loaded.
    SettingsToolsLoaded(Vec<ToolInfo>),
    /// Provider key saved.
    ProviderKeySaved(String),
    /// Provider key deleted.
    ProviderKeyDeleted(String),
    /// Provider test result.
    ProviderTestResult(TestResult),
    /// Peers loaded.
    PeersLoaded(Vec<PeerInfo>),
    /// Log entries loaded.
    LogsLoaded(Vec<LogEntry>),
    /// Hand definitions loaded (marketplace).
    HandsLoaded(Vec<HandInfo>),
    /// Active hand instances loaded.
    ActiveHandsLoaded(Vec<HandInstanceInfo>),
    /// Hand activated.
    HandActivated(String),
    /// Hand deactivated.
    HandDeactivated(String),
    /// Hand paused.
    HandPaused(String),
    /// Hand resumed.
    HandResumed(String),
    /// Extensions loaded (available + installed).
    ExtensionsLoaded(Vec<ExtensionInfo>),
    /// Extension health loaded.
    ExtensionHealthLoaded(Vec<ExtensionHealthInfo>),
    /// Extension installed.
    ExtensionInstalled(String),
    /// Extension removed.
    ExtensionRemoved(String),
    /// Extension reconnected.
    ExtensionReconnected(String, usize),
    /// Agent skills loaded (for edit screen).
    AgentSkillsLoaded {
        assigned: Vec<String>,
        available: Vec<String>,
    },
    /// Agent MCP servers loaded (for edit screen).
    AgentMcpServersLoaded {
        assigned: Vec<String>,
        available: Vec<String>,
    },
    /// Agent skills updated.
    AgentSkillsUpdated(String),
    /// Agent MCP servers updated.
    AgentMcpServersUpdated(String),
    /// Comms topology loaded.
    CommsTopologyLoaded {
        nodes: Vec<super::screens::comms::CommsNode>,
        edges: Vec<super::screens::comms::CommsEdge>,
    },
    /// Comms events loaded.
    CommsEventsLoaded(Vec<super::screens::comms::CommsEventItem>),
    /// Comms send result.
    CommsSendResult(String),
    /// Comms task post result.
    CommsTaskResult(String),
}

/// Spawn the crossterm polling + tick thread. Returns sender + receiver.
pub fn spawn_event_thread(
    tick_rate: Duration,
) -> (mpsc::Sender<AppEvent>, mpsc::Receiver<AppEvent>) {
    let (tx, rx) = mpsc::channel();
    let poll_tx = tx.clone();

    std::thread::spawn(move || {
        loop {
            if event::poll(tick_rate).unwrap_or(false) {
                if let Ok(ev) = event::read() {
                    let sent = match ev {
                        // CRITICAL: only forward Press events — Windows sends
                        // Release and Repeat too, which causes double/triple input
                        CtEvent::Key(key) if key.kind == KeyEventKind::Press => {
                            poll_tx.send(AppEvent::Key(key))
                        }
                        _ => Ok(()),
                    };
                    if sent.is_err() {
                        break;
                    }
                }
            } else {
                // No event within tick_rate → send tick for spinner animations
                if poll_tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        }
    });

    (tx, rx)
}

// ── Original spawn functions ────────────────────────────────────────────────

/// Detect daemon in a background thread (non-blocking).
pub fn spawn_daemon_detect(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let url = crate::find_daemon();
        let mut agent_count = 0u64;

        if let Some(ref u) = url {
            if let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
            {
                if let Ok(resp) = client.get(format!("{u}/api/status")).send() {
                    if let Ok(body) = resp.json::<serde_json::Value>() {
                        agent_count = body["agent_count"].as_u64().unwrap_or(0);
                    }
                }
            }
        }

        let _ = tx.send(AppEvent::DaemonDetected { url, agent_count });
    });
}

/// Spawn a background thread that boots the kernel.
pub fn spawn_kernel_boot(config: Option<std::path::PathBuf>, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        // Create a tokio runtime context so any tokio::spawn calls during
        // boot (e.g. publish_event via set_self_handle) find the reactor.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();

        match OpenFangKernel::boot(config.as_deref()) {
            Ok(k) => {
                let k = Arc::new(k);
                k.set_self_handle();
                let _ = tx.send(AppEvent::KernelReady(k));
            }
            Err(e) => {
                let _ = tx.send(AppEvent::KernelError(format!("{e}")));
            }
        }
    });
}

/// Spawn a background thread for in-process streaming.
pub fn spawn_inprocess_stream(
    kernel: Arc<OpenFangKernel>,
    agent_id: AgentId,
    message: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(AppEvent::StreamDone(Err(format!("Runtime error: {e}"))));
                return;
            }
        };

        // Enter the runtime context so tokio::spawn inside
        // send_message_streaming() finds the reactor.
        let _guard = rt.enter();

        match kernel.send_message_streaming(agent_id, &message, None) {
            Ok((mut rx, handle)) => {
                rt.block_on(async {
                    while let Some(ev) = rx.recv().await {
                        if tx.send(AppEvent::Stream(ev)).is_err() {
                            return;
                        }
                    }
                    let result = handle
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r.map_err(|e| e.to_string()));
                    let _ = tx.send(AppEvent::StreamDone(result));
                });
            }
            Err(e) => {
                let _ = tx.send(AppEvent::StreamDone(Err(format!("{e}"))));
            }
        }
    });
}

/// Spawn a background thread for daemon SSE streaming.
pub fn spawn_daemon_stream(
    base_url: String,
    agent_id: String,
    message: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read};

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap();

        let url = format!("{base_url}/api/agents/{agent_id}/message/stream");
        let resp = client
            .post(&url)
            .json(&serde_json::json!({"message": message}))
            .send();

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(_) => {
                let fallback = daemon_fallback(&base_url, &agent_id, &message);
                let _ = tx.send(AppEvent::StreamDone(fallback));
                return;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::StreamDone(Err(format!("Connection failed: {e}"))));
                return;
            }
        };

        struct RespReader(reqwest::blocking::Response);
        impl Read for RespReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(buf)
            }
        }

        // Accumulate usage across all iterations (tool-use loops send
        // multiple ContentComplete events — one per LLM call).  Do NOT
        // return early on "done": true — the SSE stream continues until
        // the server closes the connection after the agent loop finishes.
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;

        let reader = BufReader::new(RespReader(resp));
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if line.is_empty() || line.starts_with("event:") {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                        let _ = tx.send(AppEvent::Stream(StreamEvent::TextDelta {
                            text: content.to_string(),
                        }));
                    }
                    if let Some(tool) = json.get("tool").and_then(|t| t.as_str()) {
                        if json.get("input").is_none() {
                            let _ = tx.send(AppEvent::Stream(StreamEvent::ToolUseStart {
                                id: String::new(),
                                name: tool.to_string(),
                            }));
                        } else {
                            let _ = tx.send(AppEvent::Stream(StreamEvent::ToolUseEnd {
                                id: String::new(),
                                name: tool.to_string(),
                                input: json["input"].clone(),
                            }));
                        }
                    }
                    if json.get("done").and_then(|d| d.as_bool()) == Some(true) {
                        let usage = json.get("usage").cloned().unwrap_or_default();
                        total_input_tokens += usage
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        total_output_tokens += usage
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        // Forward as ContentComplete so the UI can update
                        // token display, but do NOT terminate — the agent
                        // loop may continue with tool results.
                        let _ = tx.send(AppEvent::Stream(StreamEvent::ContentComplete {
                            stop_reason: openfang_types::message::StopReason::EndTurn,
                            usage: openfang_types::message::TokenUsage {
                                input_tokens: total_input_tokens,
                                output_tokens: total_output_tokens,
                            },
                        }));
                    }
                }
            }
        }

        // Connection closed — agent loop is truly done.
        let _ = tx.send(AppEvent::StreamDone(Ok(AgentLoopResult {
            response: String::new(),
            total_usage: openfang_types::message::TokenUsage {
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
            },
            iterations: 0,
            cost_usd: None,
            silent: false,
            directives: Default::default(),
        })));
    });
}

/// Blocking fallback for daemon chat (non-streaming).
fn daemon_fallback(
    base_url: &str,
    agent_id: &str,
    message: &str,
) -> Result<AgentLoopResult, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(format!("{base_url}/api/agents/{agent_id}/message"))
        .json(&serde_json::json!({"message": message}))
        .send()
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    if let Some(response) = body.get("response").and_then(|r| r.as_str()) {
        let input_tokens = body["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = body["output_tokens"].as_u64().unwrap_or(0);
        Ok(AgentLoopResult {
            response: response.to_string(),
            total_usage: openfang_types::message::TokenUsage {
                input_tokens,
                output_tokens,
            },
            iterations: body["iterations"].as_u64().unwrap_or(0) as u32,
            cost_usd: body["cost_usd"].as_f64(),
            silent: false,
            directives: Default::default(),
        })
    } else {
        Err(body["error"]
            .as_str()
            .unwrap_or("Unknown error")
            .to_string())
    }
}

/// Spawn a background thread that spawns an agent on the daemon.
pub fn spawn_daemon_agent(base_url: String, toml_content: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        let resp = client
            .post(format!("{base_url}/api/agents"))
            .json(&serde_json::json!({"manifest_toml": toml_content}))
            .send();

        match resp {
            Ok(r) => {
                let body: serde_json::Value = r.json().unwrap_or_default();
                if let Some(id) = body.get("agent_id").and_then(|v| v.as_str()) {
                    let name = body["name"].as_str().unwrap_or("agent").to_string();
                    let _ = tx.send(AppEvent::AgentSpawned {
                        id: id.to_string(),
                        name,
                    });
                } else {
                    let _ = tx.send(AppEvent::AgentSpawnError(
                        body["error"]
                            .as_str()
                            .unwrap_or("Failed to spawn agent")
                            .to_string(),
                    ));
                }
            }
            Err(e) => {
                let _ = tx.send(AppEvent::AgentSpawnError(format!("{e}")));
            }
        }
    });
}

// ── New spawn functions for tabs ────────────────────────────────────────────

/// Fetch dashboard data in background.
pub fn spawn_fetch_dashboard(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            if let Ok(resp) = client.get(format!("{base_url}/api/status")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let _ = tx.send(AppEvent::DashboardData {
                        agent_count: body["agent_count"].as_u64().unwrap_or(0),
                        uptime_secs: body["uptime_secs"].as_u64().unwrap_or(0),
                        version: body["version"].as_str().unwrap_or("?").to_string(),
                        provider: body["provider"].as_str().unwrap_or("").to_string(),
                        model: body["model"].as_str().unwrap_or("").to_string(),
                    });
                }
            }

            // Try to fetch audit trail
            if let Ok(resp) = client.get(format!("{base_url}/api/audit/recent")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let rows: Vec<AuditRow> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|r| AuditRow {
                                    timestamp: r["timestamp"].as_str().unwrap_or("").to_string(),
                                    agent: r["agent"].as_str().unwrap_or("").to_string(),
                                    action: r["action"].as_str().unwrap_or("").to_string(),
                                    detail: r["detail"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::AuditLoaded(rows));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let count = kernel.registry.count() as u64;
            let _ = tx.send(AppEvent::DashboardData {
                agent_count: count,
                uptime_secs: 0,
                version: env!("CARGO_PKG_VERSION").to_string(),
                provider: String::new(),
                model: String::new(),
            });
            // In-process mode doesn't have a REST audit endpoint yet
            let _ = tx.send(AppEvent::AuditLoaded(Vec::new()));
        }
    });
}

/// Fetch channel list in background.
pub fn spawn_fetch_channels(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            if let Ok(resp) = client.get(format!("{base_url}/api/channels")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let channels: Vec<ChannelInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|ch| {
                                    use super::screens::channels::ChannelStatus;
                                    let status_str =
                                        ch["status"].as_str().unwrap_or("not_configured");
                                    let status = match status_str {
                                        "ready" => ChannelStatus::Ready,
                                        "missing_env" => ChannelStatus::MissingEnv,
                                        _ => ChannelStatus::NotConfigured,
                                    };
                                    ChannelInfo {
                                        name: ch["name"].as_str().unwrap_or("?").to_string(),
                                        display_name: ch["display_name"]
                                            .as_str()
                                            .unwrap_or(ch["name"].as_str().unwrap_or("?"))
                                            .to_string(),
                                        category: ch["category"]
                                            .as_str()
                                            .unwrap_or("messaging")
                                            .to_string(),
                                        status,
                                        env_vars: Vec::new(),
                                        enabled: ch["enabled"].as_bool().unwrap_or(false),
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::ChannelListLoaded(channels));
                }
            }
        }
        BackendRef::InProcess(_kernel) => {
            // In-process: fall back to default channel detection
            let _ = tx.send(AppEvent::ChannelListLoaded(Vec::new()));
        }
    });
}

/// Test a channel in background.
pub fn spawn_test_channel(backend: BackendRef, channel: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .post(format!("{base_url}/api/channels/{channel}/test"))
                .send()
            {
                Ok(resp) => {
                    let success = resp.status().is_success();
                    let msg = resp
                        .json::<serde_json::Value>()
                        .ok()
                        .and_then(|b| b["message"].as_str().map(String::from))
                        .unwrap_or_else(|| {
                            if success {
                                "Test passed".to_string()
                            } else {
                                "Test failed".to_string()
                            }
                        });
                    let _ = tx.send(AppEvent::ChannelTestResult {
                        success,
                        message: msg,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ChannelTestResult {
                        success: false,
                        message: format!("{e}"),
                    });
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::ChannelTestResult {
                success: false,
                message: "Channel test not available in in-process mode".to_string(),
            });
        }
    });
}

/// Fetch workflow list in background.
pub fn spawn_fetch_workflows(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            if let Ok(resp) = client.get(format!("{base_url}/api/workflows")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let workflows: Vec<WorkflowInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|wf| WorkflowInfo {
                                    id: wf["id"].as_str().unwrap_or("?").to_string(),
                                    name: wf["name"].as_str().unwrap_or("?").to_string(),
                                    steps: wf["steps"].as_u64().unwrap_or(0) as usize,
                                    created: wf["created"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::WorkflowListLoaded(workflows));
                }
            }
        }
        BackendRef::InProcess(_kernel) => {
            // Workflows in in-process mode - return empty for now
            let _ = tx.send(AppEvent::WorkflowListLoaded(Vec::new()));
        }
    });
}

/// Fetch workflow runs in background.
pub fn spawn_fetch_workflow_runs(
    backend: BackendRef,
    workflow_id: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            if let Ok(resp) = client
                .get(format!("{base_url}/api/workflows/{workflow_id}/runs"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let runs: Vec<WorkflowRun> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|r| WorkflowRun {
                                    id: r["id"].as_str().unwrap_or("?").to_string(),
                                    state: r["state"].as_str().unwrap_or("?").to_string(),
                                    duration: r["duration"].as_str().unwrap_or("").to_string(),
                                    output_preview: r["output"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::WorkflowRunsLoaded(runs));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::WorkflowRunsLoaded(Vec::new()));
        }
    });
}

/// Run a workflow in background.
pub fn spawn_run_workflow(
    backend: BackendRef,
    workflow_id: String,
    input: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .post(format!("{base_url}/api/workflows/{workflow_id}/run"))
                .json(&serde_json::json!({"input": input}))
                .send()
            {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let result = body["output"]
                        .as_str()
                        .unwrap_or("Workflow completed")
                        .to_string();
                    let _ = tx.send(AppEvent::WorkflowRunResult(result));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::WorkflowRunResult(format!("Error: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::WorkflowRunResult(
                "Workflow execution not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Create a workflow in background.
pub fn spawn_create_workflow(
    backend: BackendRef,
    name: String,
    description: String,
    steps_json: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .post(format!("{base_url}/api/workflows"))
                .json(&serde_json::json!({
                    "name": name,
                    "description": description,
                    "steps": steps_json,
                }))
                .send()
            {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let id = body["id"].as_str().unwrap_or("created").to_string();
                    let _ = tx.send(AppEvent::WorkflowCreated(id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Create workflow: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Workflow creation not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Fetch triggers in background.
pub fn spawn_fetch_triggers(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            if let Ok(resp) = client.get(format!("{base_url}/api/triggers")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let triggers: Vec<TriggerInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|tr| TriggerInfo {
                                    id: tr["id"].as_str().unwrap_or("?").to_string(),
                                    agent_id: tr["agent_id"].as_str().unwrap_or("?").to_string(),
                                    pattern: tr["pattern"].as_str().unwrap_or("?").to_string(),
                                    fires: tr["fires"].as_u64().unwrap_or(0),
                                    enabled: tr["enabled"].as_bool().unwrap_or(true),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::TriggerListLoaded(triggers));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::TriggerListLoaded(Vec::new()));
        }
    });
}

/// Create a trigger in background.
pub fn spawn_create_trigger(
    backend: BackendRef,
    agent_id: String,
    pattern_type: String,
    pattern_param: String,
    prompt: String,
    max_fires: u64,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .post(format!("{base_url}/api/triggers"))
                .json(&serde_json::json!({
                    "agent_id": agent_id,
                    "pattern_type": pattern_type,
                    "pattern_param": pattern_param,
                    "prompt": prompt,
                    "max_fires": max_fires,
                }))
                .send()
            {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let id = body["id"].as_str().unwrap_or("created").to_string();
                    let _ = tx.send(AppEvent::TriggerCreated(id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Create trigger: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Trigger creation not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Delete a trigger in background.
pub fn spawn_delete_trigger(backend: BackendRef, trigger_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .delete(format!("{base_url}/api/triggers/{trigger_id}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::TriggerDeleted(trigger_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to delete trigger {trigger_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Trigger deletion not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Kill an agent in background (for detail view action).
pub fn spawn_kill_agent(backend: BackendRef, agent_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());

            match client
                .delete(format!("{base_url}/api/agents/{agent_id}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::AgentKilled { id: agent_id });
                }
                _ => {
                    let _ = tx.send(AppEvent::AgentKillError(format!(
                        "Failed to kill agent {agent_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            // Try to parse as UUID-based AgentId
            if let Ok(uuid) = uuid::Uuid::parse_str(&agent_id) {
                let aid = AgentId(uuid);
                match kernel.kill_agent(aid) {
                    Ok(()) => {
                        let _ = tx.send(AppEvent::AgentKilled { id: agent_id });
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::AgentKillError(format!("{e}")));
                    }
                }
            } else {
                let _ = tx.send(AppEvent::AgentKillError(format!(
                    "Invalid agent ID: {agent_id}"
                )));
            }
        }
    });
}

/// Fetch skill assignment for an agent.
pub fn spawn_fetch_agent_skills(backend: BackendRef, agent_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            if let Ok(resp) = client
                .get(format!("{base_url}/api/agents/{agent_id}/skills"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let assigned: Vec<String> = body["assigned"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let available: Vec<String> = body["available"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::AgentSkillsLoaded {
                        assigned,
                        available,
                    });
                    return;
                }
            }
            let _ = tx.send(AppEvent::FetchError("Failed to fetch skills".to_string()));
        }
        BackendRef::InProcess(kernel) => {
            if let Ok(uuid) = uuid::Uuid::parse_str(&agent_id) {
                let aid = openfang_types::agent::AgentId(uuid);
                let assigned = kernel
                    .registry
                    .get(aid)
                    .map(|e| e.manifest.skills.clone())
                    .unwrap_or_default();
                let available = kernel
                    .skill_registry
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .skill_names();
                let _ = tx.send(AppEvent::AgentSkillsLoaded {
                    assigned,
                    available,
                });
            }
        }
    });
}

/// Fetch MCP server assignment for an agent.
pub fn spawn_fetch_agent_mcp_servers(
    backend: BackendRef,
    agent_id: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            if let Ok(resp) = client
                .get(format!("{base_url}/api/agents/{agent_id}/mcp_servers"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let assigned: Vec<String> = body["assigned"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let available: Vec<String> = body["available"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::AgentMcpServersLoaded {
                        assigned,
                        available,
                    });
                    return;
                }
            }
            let _ = tx.send(AppEvent::FetchError(
                "Failed to fetch MCP servers".to_string(),
            ));
        }
        BackendRef::InProcess(kernel) => {
            if let Ok(uuid) = uuid::Uuid::parse_str(&agent_id) {
                let aid = openfang_types::agent::AgentId(uuid);
                let assigned = kernel
                    .registry
                    .get(aid)
                    .map(|e| e.manifest.mcp_servers.clone())
                    .unwrap_or_default();
                let mut available = Vec::new();
                if let Ok(mcp_tools) = kernel.mcp_tools.lock() {
                    let mut seen = std::collections::HashSet::new();
                    for tool in mcp_tools.iter() {
                        if let Some(server) = openfang_runtime::mcp::extract_mcp_server(&tool.name)
                        {
                            if seen.insert(server.to_string()) {
                                available.push(server.to_string());
                            }
                        }
                    }
                }
                let _ = tx.send(AppEvent::AgentMcpServersLoaded {
                    assigned,
                    available,
                });
            }
        }
    });
}

/// Update an agent's skills.
pub fn spawn_update_agent_skills(
    backend: BackendRef,
    agent_id: String,
    skills: Vec<String>,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            match client
                .put(format!("{base_url}/api/agents/{agent_id}/skills"))
                .json(&serde_json::json!({"skills": skills}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::AgentSkillsUpdated(agent_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError("Failed to update skills".to_string()));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            if let Ok(uuid) = uuid::Uuid::parse_str(&agent_id) {
                let aid = openfang_types::agent::AgentId(uuid);
                match kernel.set_agent_skills(aid, skills) {
                    Ok(()) => {
                        let _ = tx.send(AppEvent::AgentSkillsUpdated(agent_id));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::FetchError(format!("Skills update: {e}")));
                    }
                }
            }
        }
    });
}

/// Update an agent's MCP servers.
pub fn spawn_update_agent_mcp_servers(
    backend: BackendRef,
    agent_id: String,
    servers: Vec<String>,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            match client
                .put(format!("{base_url}/api/agents/{agent_id}/mcp_servers"))
                .json(&serde_json::json!({"mcp_servers": servers}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::AgentMcpServersUpdated(agent_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(
                        "Failed to update MCP servers".to_string(),
                    ));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            if let Ok(uuid) = uuid::Uuid::parse_str(&agent_id) {
                let aid = openfang_types::agent::AgentId(uuid);
                match kernel.set_agent_mcp_servers(aid, servers) {
                    Ok(()) => {
                        let _ = tx.send(AppEvent::AgentMcpServersUpdated(agent_id));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::FetchError(format!("MCP update: {e}")));
                    }
                }
            }
        }
    });
}

// ── New screen spawn functions ───────────────────────────────────────────────

fn daemon_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Fetch sessions list.
pub fn spawn_fetch_sessions(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/sessions")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let sessions: Vec<SessionInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|s| SessionInfo {
                                    id: s["id"].as_str().unwrap_or("").to_string(),
                                    agent_name: s["agent_name"].as_str().unwrap_or("").to_string(),
                                    agent_id: s["agent_id"].as_str().unwrap_or("").to_string(),
                                    message_count: s["message_count"].as_u64().unwrap_or(0),
                                    created: s["created"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::SessionsLoaded(sessions));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SessionsLoaded(Vec::new()));
        }
    });
}

/// Delete a session.
pub fn spawn_delete_session(backend: BackendRef, session_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .delete(format!("{base_url}/api/sessions/{session_id}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::SessionDeleted(session_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to delete session {session_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Session management not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Fetch agents for memory screen agent selector.
pub fn spawn_fetch_memory_agents(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/agents")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let agents: Vec<AgentEntry> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|a| AgentEntry {
                                    id: a["id"].as_str().unwrap_or("").to_string(),
                                    name: a["name"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::MemoryAgentsLoaded(agents));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let agents: Vec<AgentEntry> = kernel
                .registry
                .list()
                .iter()
                .map(|e| AgentEntry {
                    id: format!("{}", e.id),
                    name: e.name.clone(),
                })
                .collect();
            let _ = tx.send(AppEvent::MemoryAgentsLoaded(agents));
        }
    });
}

/// Fetch KV pairs for an agent.
pub fn spawn_fetch_memory_kv(backend: BackendRef, agent_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client
                .get(format!("{base_url}/api/memory/agents/{agent_id}/kv"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let pairs: Vec<KvPair> = if let Some(obj) = body.as_object() {
                        obj.iter()
                            .map(|(k, v)| KvPair {
                                key: k.clone(),
                                value: v.as_str().unwrap_or(&v.to_string()).to_string(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let _ = tx.send(AppEvent::MemoryKvLoaded(pairs));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::MemoryKvLoaded(Vec::new()));
        }
    });
}

/// Save a KV pair.
pub fn spawn_save_memory_kv(
    backend: BackendRef,
    agent_id: String,
    key: String,
    value: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .put(format!("{base_url}/api/memory/agents/{agent_id}/kv/{key}"))
                .json(&serde_json::json!({"value": value}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::MemoryKvSaved { key });
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError("Failed to save KV pair".to_string()));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Memory KV not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Delete a KV pair.
pub fn spawn_delete_memory_kv(
    backend: BackendRef,
    agent_id: String,
    key: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .delete(format!("{base_url}/api/memory/agents/{agent_id}/kv/{key}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::MemoryKvDeleted(key));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError("Failed to delete KV pair".to_string()));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Memory KV not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Fetch installed skills.
pub fn spawn_fetch_skills(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/skills")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let skills: Vec<SkillInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|s| SkillInfo {
                                    name: s["name"].as_str().unwrap_or("").to_string(),
                                    runtime: s["runtime"].as_str().unwrap_or("").to_string(),
                                    source: s["source"].as_str().unwrap_or("").to_string(),
                                    description: s["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::SkillsLoaded(skills));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SkillsLoaded(Vec::new()));
        }
    });
}

/// Search ClawHub marketplace.
pub fn spawn_search_clawhub(backend: BackendRef, query: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            let encoded: String = query
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                        c.to_string()
                    } else {
                        format!("%{:02X}", c as u32)
                    }
                })
                .collect();
            let url = format!("{base_url}/api/clawhub/search?q={encoded}");
            if let Ok(resp) = client.get(&url).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let results = parse_clawhub_results(&body);
                    let _ = tx.send(AppEvent::ClawHubLoaded(results));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::ClawHubLoaded(Vec::new()));
        }
    });
}

/// Browse ClawHub marketplace.
pub fn spawn_browse_clawhub(backend: BackendRef, sort: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            let url = format!("{base_url}/api/clawhub/browse?sort={sort}");
            if let Ok(resp) = client.get(&url).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let results = parse_clawhub_results(&body);
                    let _ = tx.send(AppEvent::ClawHubLoaded(results));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::ClawHubLoaded(Vec::new()));
        }
    });
}

fn parse_clawhub_results(body: &serde_json::Value) -> Vec<ClawHubResult> {
    body.as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| ClawHubResult {
                    name: r["name"].as_str().unwrap_or("").to_string(),
                    slug: r["slug"].as_str().unwrap_or("").to_string(),
                    description: r["description"].as_str().unwrap_or("").to_string(),
                    downloads: r["downloads"].as_u64().unwrap_or(0),
                    runtime: r["runtime"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Install a skill from ClawHub.
pub fn spawn_install_skill(backend: BackendRef, slug: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/clawhub/install"))
                .json(&serde_json::json!({"slug": slug}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::SkillInstalled(slug));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!("Failed to install {slug}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Skill installation not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Uninstall a skill.
pub fn spawn_uninstall_skill(backend: BackendRef, name: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/skills/uninstall"))
                .json(&serde_json::json!({"name": name}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::SkillUninstalled(name));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!("Failed to uninstall {name}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Skill uninstall not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Fetch MCP servers.
pub fn spawn_fetch_mcp_servers(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/mcp/servers")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let servers: Vec<McpServerInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|s| McpServerInfo {
                                    name: s["name"].as_str().unwrap_or("").to_string(),
                                    connected: s["connected"].as_bool().unwrap_or(false),
                                    tool_count: s["tool_count"].as_u64().unwrap_or(0) as usize,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::McpServersLoaded(servers));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::McpServersLoaded(Vec::new()));
        }
    });
}

/// Fetch provider auth status for templates screen.
pub fn spawn_fetch_template_providers(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/providers")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    // API returns { "providers": [...], "total": N }
                    let arr = body["providers"].as_array();
                    let providers: Vec<ProviderAuth> = arr
                        .map(|arr| {
                            arr.iter()
                                .map(|p| {
                                    let auth = p["auth_status"].as_str().unwrap_or("missing");
                                    ProviderAuth {
                                        name: p["id"].as_str().unwrap_or("").to_string(),
                                        configured: auth == "configured" || auth == "not_required",
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::TemplateProvidersLoaded(providers));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::TemplateProvidersLoaded(Vec::new()));
        }
    });
}

/// Fetch security status.
pub fn spawn_fetch_security(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/security")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let features: Vec<SecurityFeature> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|f| {
                                    use super::screens::security::SecuritySection;
                                    let section = match f["section"].as_str().unwrap_or("core") {
                                        "configurable" => SecuritySection::Configurable,
                                        "monitoring" => SecuritySection::Monitoring,
                                        _ => SecuritySection::Core,
                                    };
                                    SecurityFeature {
                                        name: f["name"].as_str().unwrap_or("").to_string(),
                                        active: f["active"].as_bool().unwrap_or(true),
                                        description: f["description"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        section,
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if !features.is_empty() {
                        let _ = tx.send(AppEvent::SecurityLoaded(features));
                    }
                }
            }
        }
        BackendRef::InProcess(_) => {
            // Use builtin defaults (already loaded in SecurityState::new())
        }
    });
}

/// Verify audit chain.
pub fn spawn_verify_chain(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client.get(format!("{base_url}/api/audit/verify")).send() {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let valid = body["valid"].as_bool().unwrap_or(false);
                    let message = body["message"]
                        .as_str()
                        .unwrap_or("Verification complete")
                        .to_string();
                    let _ = tx.send(AppEvent::SecurityChainVerified { valid, message });
                    let _ = tx.send(AppEvent::AuditChainVerified(valid));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::SecurityChainVerified {
                        valid: false,
                        message: format!("{e}"),
                    });
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SecurityChainVerified {
                valid: true,
                message: "In-process mode: chain not applicable".to_string(),
            });
        }
    });
}

/// Fetch audit entries (for dedicated audit screen).
pub fn spawn_fetch_audit(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client
                .get(format!("{base_url}/api/audit/recent?n=200"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let entries: Vec<AuditEntry> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|e| AuditEntry {
                                    timestamp: e["timestamp"].as_str().unwrap_or("").to_string(),
                                    action: e["action"].as_str().unwrap_or("").to_string(),
                                    agent: e["agent"].as_str().unwrap_or("").to_string(),
                                    detail: e["detail"].as_str().unwrap_or("").to_string(),
                                    tip_hash: e["tip_hash"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::AuditEntriesLoaded(entries));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::AuditEntriesLoaded(Vec::new()));
        }
    });
}

/// Fetch usage summary.
pub fn spawn_fetch_usage(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            // Summary
            if let Ok(resp) = client.get(format!("{base_url}/api/usage/summary")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let _ = tx.send(AppEvent::UsageSummaryLoaded(UsageSummary {
                        total_input_tokens: body["total_input_tokens"].as_u64().unwrap_or(0),
                        total_output_tokens: body["total_output_tokens"].as_u64().unwrap_or(0),
                        total_cost_usd: body["total_cost_usd"].as_f64().unwrap_or(0.0),
                        total_calls: body["total_calls"].as_u64().unwrap_or(0),
                    }));
                }
            }
            // By model
            if let Ok(resp) = client.get(format!("{base_url}/api/usage/by-model")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let models: Vec<ModelUsage> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|m| ModelUsage {
                                    model_id: m["model_id"].as_str().unwrap_or("").to_string(),
                                    input_tokens: m["input_tokens"].as_u64().unwrap_or(0),
                                    output_tokens: m["output_tokens"].as_u64().unwrap_or(0),
                                    cost_usd: m["cost_usd"].as_f64().unwrap_or(0.0),
                                    calls: m["calls"].as_u64().unwrap_or(0),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::UsageByModelLoaded(models));
                }
            }
            // By agent
            if let Ok(resp) = client.get(format!("{base_url}/api/usage")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let agents: Vec<AgentUsage> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|a| AgentUsage {
                                    agent_name: a["agent_name"].as_str().unwrap_or("").to_string(),
                                    agent_id: a["agent_id"].as_str().unwrap_or("").to_string(),
                                    total_tokens: a["total_tokens"].as_u64().unwrap_or(0),
                                    cost_usd: a["cost_usd"].as_f64().unwrap_or(0.0),
                                    tool_calls: a["tool_calls"].as_u64().unwrap_or(0),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::UsageByAgentLoaded(agents));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::UsageSummaryLoaded(UsageSummary::default()));
            let _ = tx.send(AppEvent::UsageByModelLoaded(Vec::new()));
            let _ = tx.send(AppEvent::UsageByAgentLoaded(Vec::new()));
        }
    });
}

/// Fetch settings providers.
pub fn spawn_fetch_providers(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/providers")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    // API returns { "providers": [...], "total": N }
                    let arr = body["providers"].as_array();
                    let providers: Vec<ProviderInfo> = arr
                        .map(|arr| {
                            arr.iter()
                                .map(|p| {
                                    let auth = p["auth_status"].as_str().unwrap_or("missing");
                                    let key_required = p["key_required"].as_bool().unwrap_or(true);
                                    let configured = auth == "configured" || auth == "not_required";
                                    let is_local =
                                        p["is_local"].as_bool().unwrap_or(false) || !key_required;
                                    ProviderInfo {
                                        name: p["id"].as_str().unwrap_or("").to_string(),
                                        configured,
                                        env_var: p["api_key_env"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        is_local,
                                        reachable: if is_local {
                                            p["reachable"].as_bool()
                                        } else {
                                            None
                                        },
                                        latency_ms: if is_local {
                                            p["latency_ms"].as_u64()
                                        } else {
                                            None
                                        },
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::SettingsProvidersLoaded(providers));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SettingsProvidersLoaded(Vec::new()));
        }
    });
}

/// Fetch settings models.
pub fn spawn_fetch_models(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/models")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let models: Vec<ModelInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|m| ModelInfo {
                                    id: m["id"].as_str().unwrap_or("").to_string(),
                                    provider: m["provider"].as_str().unwrap_or("").to_string(),
                                    tier: m["tier"].as_str().unwrap_or("").to_string(),
                                    context_window: m["context_window"].as_u64().unwrap_or(0),
                                    cost_input: m["cost_input"].as_f64().unwrap_or(0.0),
                                    cost_output: m["cost_output"].as_f64().unwrap_or(0.0),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::SettingsModelsLoaded(models));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SettingsModelsLoaded(Vec::new()));
        }
    });
}

/// Fetch settings tools.
pub fn spawn_fetch_tools(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/tools")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let tools: Vec<ToolInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|t| ToolInfo {
                                    name: t["name"].as_str().unwrap_or("").to_string(),
                                    description: t["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::SettingsToolsLoaded(tools));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::SettingsToolsLoaded(Vec::new()));
        }
    });
}

/// Save a provider API key.
pub fn spawn_save_provider_key(
    backend: BackendRef,
    name: String,
    api_key: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/providers/{name}/key"))
                .json(&serde_json::json!({"key": api_key}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::ProviderKeySaved(name));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to save key for {name}"
                    )));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Provider key management not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Delete a provider API key.
pub fn spawn_delete_provider_key(backend: BackendRef, name: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .delete(format!("{base_url}/api/providers/{name}/key"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::ProviderKeyDeleted(name));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to delete key for {name}"
                    )));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Provider key management not available in in-process mode".to_string(),
            ));
        }
    });
}

/// Test a provider connection.
pub fn spawn_test_provider(backend: BackendRef, name: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            let start = std::time::Instant::now();
            match client
                .post(format!("{base_url}/api/providers/{name}/test"))
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let success = resp.status().is_success();
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let message = body["message"]
                        .as_str()
                        .unwrap_or(if success {
                            "Connection OK"
                        } else {
                            "Test failed"
                        })
                        .to_string();
                    let _ = tx.send(AppEvent::ProviderTestResult(TestResult {
                        provider: name,
                        success,
                        latency_ms: latency,
                        message,
                    }));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ProviderTestResult(TestResult {
                        provider: name,
                        success: false,
                        latency_ms: 0,
                        message: format!("{e}"),
                    }));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::ProviderTestResult(TestResult {
                provider: name,
                success: false,
                latency_ms: 0,
                message: "Provider test not available in in-process mode".to_string(),
            }));
        }
    });
}

/// Fetch peers.
pub fn spawn_fetch_peers(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/peers")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let peers: Vec<PeerInfo> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|p| PeerInfo {
                                    node_id: p["node_id"].as_str().unwrap_or("").to_string(),
                                    node_name: p["node_name"].as_str().unwrap_or("").to_string(),
                                    address: p["address"].as_str().unwrap_or("").to_string(),
                                    state: p["state"].as_str().unwrap_or("").to_string(),
                                    agent_count: p["agent_count"].as_u64().unwrap_or(0),
                                    protocol_version: p["protocol_version"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::PeersLoaded(peers));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::PeersLoaded(Vec::new()));
        }
    });
}

/// Fetch log entries (uses audit endpoint, polled frequently).
pub fn spawn_fetch_logs(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client
                .get(format!("{base_url}/api/audit/recent?n=200"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let entries: Vec<LogEntry> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|e| {
                                    let action = e["action"].as_str().unwrap_or("").to_string();
                                    let detail = e["detail"].as_str().unwrap_or("").to_string();
                                    let level =
                                        super::screens::logs::classify_level(&action, &detail);
                                    LogEntry {
                                        timestamp: e["timestamp"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        level,
                                        action,
                                        detail,
                                        agent: e["agent"].as_str().unwrap_or("").to_string(),
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::LogsLoaded(entries));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::LogsLoaded(Vec::new()));
        }
    });
}

// ── Hands events ────────────────────────────────────────────────────────────

/// Fetch hand definitions (marketplace).
pub fn spawn_fetch_hands(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/hands")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let hands: Vec<HandInfo> = body["hands"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|h| HandInfo {
                                    id: h["id"].as_str().unwrap_or("").to_string(),
                                    name: h["name"].as_str().unwrap_or("").to_string(),
                                    description: h["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    category: h["category"].as_str().unwrap_or("").to_string(),
                                    icon: h["icon"].as_str().unwrap_or("").to_string(),
                                    requirements_met: h["requirements_met"]
                                        .as_bool()
                                        .unwrap_or(false),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::HandsLoaded(hands));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let defs = kernel.hand_registry.list_definitions();
            let hands: Vec<HandInfo> = defs
                .iter()
                .map(|d| {
                    let reqs_met = kernel
                        .hand_registry
                        .check_requirements(&d.id)
                        .map(|r| r.iter().all(|(_, ok)| *ok))
                        .unwrap_or(false);
                    HandInfo {
                        id: d.id.clone(),
                        name: d.name.clone(),
                        description: d.description.clone(),
                        category: d.category.to_string(),
                        icon: d.icon.clone(),
                        requirements_met: reqs_met,
                    }
                })
                .collect();
            let _ = tx.send(AppEvent::HandsLoaded(hands));
        }
    });
}

/// Fetch active hand instances.
pub fn spawn_fetch_active_hands(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client.get(format!("{base_url}/api/hands/active")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let instances: Vec<HandInstanceInfo> = body["instances"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|i| HandInstanceInfo {
                                    instance_id: i["instance_id"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    hand_id: i["hand_id"].as_str().unwrap_or("").to_string(),
                                    status: i["status"].as_str().unwrap_or("").to_string(),
                                    agent_name: i["agent_name"].as_str().unwrap_or("").to_string(),
                                    agent_id: i["agent_id"].as_str().unwrap_or("").to_string(),
                                    activated_at: i["activated_at"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::ActiveHandsLoaded(instances));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let instances: Vec<HandInstanceInfo> = kernel
                .hand_registry
                .list_instances()
                .iter()
                .map(|i| HandInstanceInfo {
                    instance_id: i.instance_id.to_string(),
                    hand_id: i.hand_id.clone(),
                    status: i.status.to_string(),
                    agent_name: i.agent_name.clone(),
                    agent_id: i.agent_id.map(|a| a.to_string()).unwrap_or_default(),
                    activated_at: i.activated_at.to_rfc3339(),
                })
                .collect();
            let _ = tx.send(AppEvent::ActiveHandsLoaded(instances));
        }
    });
}

/// Activate a hand.
pub fn spawn_activate_hand(backend: BackendRef, hand_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/hands/{hand_id}/activate"))
                .json(&serde_json::json!({}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::HandActivated(hand_id));
                }
                Ok(resp) => {
                    let msg = resp
                        .json::<serde_json::Value>()
                        .ok()
                        .and_then(|b| b["error"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "Activation failed".to_string());
                    let _ = tx.send(AppEvent::FetchError(msg));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Failed to activate: {e}")));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            match kernel.activate_hand(&hand_id, std::collections::HashMap::new()) {
                Ok(_) => {
                    let _ = tx.send(AppEvent::HandActivated(hand_id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Activation failed: {e}")));
                }
            }
        }
    });
}

/// Deactivate a hand instance.
pub fn spawn_deactivate_hand(backend: BackendRef, instance_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .delete(format!("{base_url}/api/hands/instances/{instance_id}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::HandDeactivated(instance_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to deactivate {instance_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(kernel) => match uuid::Uuid::parse_str(&instance_id) {
            Ok(uuid) => match kernel.deactivate_hand(uuid) {
                Ok(()) => {
                    let _ = tx.send(AppEvent::HandDeactivated(instance_id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Deactivate failed: {e}")));
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::FetchError(format!("Invalid instance ID: {e}")));
            }
        },
    });
}

/// Pause a hand instance.
pub fn spawn_pause_hand(backend: BackendRef, instance_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!(
                    "{base_url}/api/hands/instances/{instance_id}/pause"
                ))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::HandPaused(instance_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to pause {instance_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(kernel) => match uuid::Uuid::parse_str(&instance_id) {
            Ok(uuid) => match kernel.pause_hand(uuid) {
                Ok(()) => {
                    let _ = tx.send(AppEvent::HandPaused(instance_id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Pause failed: {e}")));
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::FetchError(format!("Invalid instance ID: {e}")));
            }
        },
    });
}

/// Resume a hand instance.
pub fn spawn_resume_hand(backend: BackendRef, instance_id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!(
                    "{base_url}/api/hands/instances/{instance_id}/resume"
                ))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::HandResumed(instance_id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!(
                        "Failed to resume {instance_id}"
                    )));
                }
            }
        }
        BackendRef::InProcess(kernel) => match uuid::Uuid::parse_str(&instance_id) {
            Ok(uuid) => match kernel.resume_hand(uuid) {
                Ok(()) => {
                    let _ = tx.send(AppEvent::HandResumed(instance_id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Resume failed: {e}")));
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::FetchError(format!("Invalid instance ID: {e}")));
            }
        },
    });
}

// ── Extension spawn functions ───────────────────────────────────────────────

/// Fetch all extensions (available + installed).
pub fn spawn_fetch_extensions(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client
                .get(format!("{base_url}/api/integrations/available"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    // Also fetch installed to merge status
                    let installed_ids: Vec<String> = client
                        .get(format!("{base_url}/api/integrations"))
                        .send()
                        .ok()
                        .and_then(|r| r.json::<serde_json::Value>().ok())
                        .and_then(|b| {
                            b["installed"].as_array().map(|arr| {
                                arr.iter()
                                    .filter_map(|i| i["id"].as_str().map(String::from))
                                    .collect()
                            })
                        })
                        .unwrap_or_default();

                    let extensions: Vec<ExtensionInfo> = body["integrations"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|e| {
                                    let id = e["id"].as_str().unwrap_or("").to_string();
                                    let installed = installed_ids.contains(&id);
                                    ExtensionInfo {
                                        id: id.clone(),
                                        name: e["name"].as_str().unwrap_or("").to_string(),
                                        description: e["description"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        icon: e["icon"].as_str().unwrap_or("").to_string(),
                                        category: e["category"].as_str().unwrap_or("").to_string(),
                                        installed,
                                        status: if installed {
                                            "installed".to_string()
                                        } else {
                                            "available".to_string()
                                        },
                                        tags: e["tags"]
                                            .as_array()
                                            .map(|t| {
                                                t.iter()
                                                    .filter_map(|v| v.as_str().map(String::from))
                                                    .collect()
                                            })
                                            .unwrap_or_default(),
                                        has_oauth: e["has_oauth"].as_bool().unwrap_or(false),
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::ExtensionsLoaded(extensions));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let registry = kernel
                .extension_registry
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let extensions: Vec<ExtensionInfo> = registry
                .list_templates()
                .iter()
                .map(|t| {
                    let installed = registry.is_installed(&t.id);
                    ExtensionInfo {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        description: t.description.clone(),
                        icon: t.icon.clone(),
                        category: t.category.to_string(),
                        installed,
                        status: if installed {
                            "installed".to_string()
                        } else {
                            "available".to_string()
                        },
                        tags: t.tags.clone(),
                        has_oauth: t.oauth.is_some(),
                    }
                })
                .collect();
            let _ = tx.send(AppEvent::ExtensionsLoaded(extensions));
        }
    });
}

/// Fetch extension health data.
pub fn spawn_fetch_extension_health(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            if let Ok(resp) = client
                .get(format!("{base_url}/api/integrations/health"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let entries: Vec<ExtensionHealthInfo> = body["health"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|h| ExtensionHealthInfo {
                                    id: h["id"].as_str().unwrap_or("").to_string(),
                                    status: h["status"].as_str().unwrap_or("").to_string(),
                                    tool_count: h["tool_count"].as_u64().unwrap_or(0) as usize,
                                    last_ok: h["last_ok"].as_str().unwrap_or("").to_string(),
                                    last_error: h["last_error"].as_str().unwrap_or("").to_string(),
                                    consecutive_failures: h["consecutive_failures"]
                                        .as_u64()
                                        .unwrap_or(0)
                                        as u32,
                                    reconnecting: h["reconnecting"].as_bool().unwrap_or(false),
                                    connected_since: h["connected_since"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::ExtensionHealthLoaded(entries));
                }
            }
        }
        BackendRef::InProcess(kernel) => {
            let health = kernel.extension_health.all_health();
            let entries: Vec<ExtensionHealthInfo> = health
                .iter()
                .map(|h| ExtensionHealthInfo {
                    id: h.id.clone(),
                    status: h.status.to_string(),
                    tool_count: h.tool_count,
                    last_ok: h.last_ok.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    last_error: h.last_error.clone().unwrap_or_default(),
                    consecutive_failures: h.consecutive_failures,
                    reconnecting: h.reconnecting,
                    connected_since: h
                        .connected_since
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default(),
                })
                .collect();
            let _ = tx.send(AppEvent::ExtensionHealthLoaded(entries));
        }
    });
}

/// Install an extension.
pub fn spawn_install_extension(backend: BackendRef, id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/integrations/add"))
                .json(&serde_json::json!({"id": id}))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::ExtensionInstalled(id));
                }
                Ok(resp) => {
                    let body = resp.json::<serde_json::Value>().ok();
                    let err = body
                        .and_then(|b| b["error"].as_str().map(String::from))
                        .unwrap_or_else(|| format!("Failed to install {id}"));
                    let _ = tx.send(AppEvent::FetchError(err));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError(format!("Install failed: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Install via in-process mode not supported — use CLI".to_string(),
            ));
        }
    });
}

/// Remove an extension.
pub fn spawn_remove_extension(backend: BackendRef, id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .delete(format!("{base_url}/api/integrations/{id}"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let _ = tx.send(AppEvent::ExtensionRemoved(id));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!("Failed to remove {id}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Remove via in-process mode not supported — use CLI".to_string(),
            ));
        }
    });
}

/// Reconnect an extension's MCP server.
pub fn spawn_reconnect_extension(backend: BackendRef, id: String, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            match client
                .post(format!("{base_url}/api/integrations/{id}/reconnect"))
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    let tool_count = resp
                        .json::<serde_json::Value>()
                        .ok()
                        .and_then(|b| b["tool_count"].as_u64())
                        .unwrap_or(0) as usize;
                    let _ = tx.send(AppEvent::ExtensionReconnected(id, tool_count));
                }
                _ => {
                    let _ = tx.send(AppEvent::FetchError(format!("Failed to reconnect {id}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::FetchError(
                "Reconnect via in-process mode not supported".to_string(),
            ));
        }
    });
}

/// Fetch comms topology + events.
pub fn spawn_fetch_comms(backend: BackendRef, tx: mpsc::Sender<AppEvent>) {
    use super::screens::comms::{CommsEdge, CommsEventItem, CommsNode};

    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            // Fetch topology
            if let Ok(resp) = client.get(format!("{base_url}/api/comms/topology")).send() {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let nodes: Vec<CommsNode> = body["nodes"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|n| CommsNode {
                                    id: n["id"].as_str().unwrap_or("").to_string(),
                                    name: n["name"].as_str().unwrap_or("").to_string(),
                                    state: n["state"].as_str().unwrap_or("").to_string(),
                                    model: n["model"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let edges: Vec<CommsEdge> = body["edges"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|e| CommsEdge {
                                    from: e["from"].as_str().unwrap_or("").to_string(),
                                    to: e["to"].as_str().unwrap_or("").to_string(),
                                    kind: e["kind"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::CommsTopologyLoaded { nodes, edges });
                }
            }
            // Fetch events
            if let Ok(resp) = client
                .get(format!("{base_url}/api/comms/events?limit=100"))
                .send()
            {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    let events: Vec<CommsEventItem> = body
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|e| CommsEventItem {
                                    id: e["id"].as_str().unwrap_or("").to_string(),
                                    timestamp: e["timestamp"].as_str().unwrap_or("").to_string(),
                                    kind: e["kind"].as_str().unwrap_or("").to_string(),
                                    source_name: e["source_name"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    target_name: e["target_name"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    detail: e["detail"].as_str().unwrap_or("").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = tx.send(AppEvent::CommsEventsLoaded(events));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::CommsTopologyLoaded {
                nodes: Vec::new(),
                edges: Vec::new(),
            });
            let _ = tx.send(AppEvent::CommsEventsLoaded(Vec::new()));
        }
    });
}

/// Send a message between agents via comms endpoint.
pub fn spawn_comms_send(
    backend: BackendRef,
    from: String,
    to: String,
    msg: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            let body = serde_json::json!({
                "from_agent_id": from,
                "to_agent_id": to,
                "message": msg,
            });
            match client
                .post(format!("{base_url}/api/comms/send"))
                .json(&body)
                .send()
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let _ = tx.send(AppEvent::CommsSendResult("Message sent".to_string()));
                    } else {
                        let err = resp
                            .json::<serde_json::Value>()
                            .ok()
                            .and_then(|v| v["error"].as_str().map(String::from))
                            .unwrap_or_else(|| "Send failed".to_string());
                        let _ = tx.send(AppEvent::CommsSendResult(err));
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::CommsSendResult(format!("Error: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::CommsSendResult(
                "Send not supported in-process".to_string(),
            ));
        }
    });
}

/// Post a task via comms endpoint.
pub fn spawn_comms_task(
    backend: BackendRef,
    title: String,
    desc: String,
    assign: String,
    tx: mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || match backend {
        BackendRef::Daemon(base_url) => {
            let client = daemon_client();
            let mut body = serde_json::json!({
                "title": title,
                "description": desc,
            });
            if !assign.is_empty() {
                body["assigned_to"] = serde_json::Value::String(assign);
            }
            match client
                .post(format!("{base_url}/api/comms/task"))
                .json(&body)
                .send()
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let _ = tx.send(AppEvent::CommsTaskResult("Task posted".to_string()));
                    } else {
                        let err = resp
                            .json::<serde_json::Value>()
                            .ok()
                            .and_then(|v| v["error"].as_str().map(String::from))
                            .unwrap_or_else(|| "Post failed".to_string());
                        let _ = tx.send(AppEvent::CommsTaskResult(err));
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::CommsTaskResult(format!("Error: {e}")));
                }
            }
        }
        BackendRef::InProcess(_) => {
            let _ = tx.send(AppEvent::CommsTaskResult(
                "Task post not supported in-process".to_string(),
            ));
        }
    });
}
