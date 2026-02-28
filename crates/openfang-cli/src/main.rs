//! OpenFang CLI — command-line interface for the OpenFang Agent OS.
//!
//! When a daemon is running (`openfang start`), the CLI talks to it over HTTP.
//! Otherwise, commands boot an in-process kernel (single-shot mode).

mod bundled_agents;
mod dotenv;
mod launcher;
mod mcp;
pub mod progress;
pub mod table;
mod templates;
mod tui;
mod ui;

use clap::{Parser, Subcommand};
use colored::Colorize;
use openfang_api::server::read_daemon_info;
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::{AgentId, AgentManifest};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::atomic::Ordering;

/// Global flag set by the Ctrl+C handler.
static CTRLC_PRESSED: AtomicBool = AtomicBool::new(false);

/// Install a Ctrl+C handler that force-exits the process.
/// On Windows/MINGW, the default handler doesn't reliably interrupt blocking
/// `read_line` calls, so we explicitly call `process::exit`.
fn install_ctrlc_handler() {
    #[cfg(windows)]
    {
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        unsafe extern "system" fn handler(_ctrl_type: u32) -> i32 {
            if CTRLC_PRESSED.swap(true, Ordering::SeqCst) {
                // Second press: hard exit
                std::process::exit(130);
            }
            // First press: print message and exit cleanly
            let _ = std::io::Write::write_all(&mut std::io::stderr(), b"\nInterrupted.\n");
            std::process::exit(0);
        }
        unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
    }

    #[cfg(not(windows))]
    {
        // On Unix, the default SIGINT handler already interrupts read_line
        // and terminates the process.
        let _ = &CTRLC_PRESSED;
    }
}

const AFTER_HELP: &str = "\
\x1b[1mHint:\x1b[0m Commands suffixed with [*] have subcommands. Run `<command> --help` for details.

\x1b[1;36mExamples:\x1b[0m
  openfang init                 Initialize config and data directories
  openfang start                Start the kernel daemon
  openfang tui                  Launch the interactive terminal dashboard
  openfang chat                 Quick chat with the default agent
  openfang agent new coder      Spawn a new agent from a template
  openfang models list          Browse available LLM models
  openfang add github           Install the GitHub integration
  openfang doctor               Run diagnostic health checks
  openfang channel setup        Interactive channel setup wizard
  openfang cron list            List scheduled jobs

\x1b[1;36mQuick Start:\x1b[0m
  1. openfang init              Set up config + API key
  2. openfang start             Launch the daemon
  3. openfang chat              Start chatting!

\x1b[1;36mMore:\x1b[0m
  Docs:       https://github.com/RightNow-AI/openfang
  Dashboard:  http://127.0.0.1:4200/ (when daemon is running)";

/// OpenFang — the open-source Agent Operating System.
#[derive(Parser)]
#[command(
    name = "openfang",
    version,
    about = "\u{1F40D} OpenFang \u{2014} Open-source Agent Operating System",
    long_about = "\u{1F40D} OpenFang \u{2014} Open-source Agent Operating System\n\n\
                  Deploy, manage, and orchestrate AI agents from your terminal.\n\
                  40 channels \u{00b7} 60 skills \u{00b7} 50+ models \u{00b7} infinite possibilities.",
    after_help = AFTER_HELP,
)]
struct Cli {
    /// Path to config file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize OpenFang (create ~/.openfang/ and default config).
    Init {
        /// Quick mode: no prompts, just write config + .env (for CI/scripts).
        #[arg(long)]
        quick: bool,
    },
    /// Start the OpenFang kernel daemon (API server + kernel).
    Start,
    /// Stop the running daemon.
    Stop,
    /// Manage agents (new, list, chat, kill, spawn) [*].
    #[command(subcommand)]
    Agent(AgentCommands),
    /// Manage workflows (list, create, run) [*].
    #[command(subcommand)]
    Workflow(WorkflowCommands),
    /// Manage event triggers (list, create, delete) [*].
    #[command(subcommand)]
    Trigger(TriggerCommands),
    /// Migrate from another agent framework to OpenFang.
    Migrate(MigrateArgs),
    /// Manage skills (install, list, search, create, remove) [*].
    #[command(subcommand)]
    Skill(SkillCommands),
    /// Manage channel integrations (setup, test, enable, disable) [*].
    #[command(subcommand)]
    Channel(ChannelCommands),
    /// Show or edit configuration (show, edit, get, set, keys) [*].
    #[command(subcommand)]
    Config(ConfigCommands),
    /// Quick chat with the default agent.
    Chat {
        /// Optional agent name or ID to chat with.
        agent: Option<String>,
    },
    /// Show kernel status.
    Status {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Run diagnostic health checks.
    Doctor {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
        /// Attempt to auto-fix issues (create missing dirs/config).
        #[arg(long)]
        repair: bool,
    },
    /// Open the web dashboard in the default browser.
    Dashboard,
    /// Generate shell completion scripts.
    Completion {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Start MCP (Model Context Protocol) server over stdio.
    Mcp,
    /// Add an integration (one-click MCP server setup).
    Add {
        /// Integration name (e.g., "github", "slack", "notion").
        name: String,
        /// API key or token to store in the vault.
        #[arg(long)]
        key: Option<String>,
    },
    /// Remove an installed integration.
    Remove {
        /// Integration name.
        name: String,
    },
    /// List or search integrations.
    Integrations {
        /// Search query (optional — lists all if omitted).
        query: Option<String>,
    },
    /// Manage the credential vault (init, set, list, remove) [*].
    #[command(subcommand)]
    Vault(VaultCommands),
    /// Scaffold a new skill or integration template.
    New {
        /// What to scaffold.
        #[arg(value_enum)]
        kind: ScaffoldKind,
    },
    /// Launch the interactive terminal dashboard.
    Tui,
    /// Browse models, aliases, and providers [*].
    #[command(subcommand)]
    Models(ModelsCommands),
    /// Daemon control (start, stop, status) [*].
    #[command(subcommand)]
    Gateway(GatewayCommands),
    /// Manage execution approvals (list, approve, reject) [*].
    #[command(subcommand)]
    Approvals(ApprovalsCommands),
    /// Manage scheduled jobs (list, create, delete, enable, disable) [*].
    #[command(subcommand)]
    Cron(CronCommands),
    /// List conversation sessions.
    Sessions {
        /// Optional agent name or ID to filter by.
        agent: Option<String>,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Tail the OpenFang log file.
    Logs {
        /// Number of lines to show.
        #[arg(long, default_value = "50")]
        lines: usize,
        /// Follow log output in real time.
        #[arg(long, short)]
        follow: bool,
    },
    /// Quick daemon health check.
    Health {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Security tools and audit trail [*].
    #[command(subcommand)]
    Security(SecurityCommands),
    /// Search and manage agent memory (KV store) [*].
    #[command(subcommand)]
    Memory(MemoryCommands),
    /// Device pairing and token management [*].
    #[command(subcommand)]
    Devices(DevicesCommands),
    /// Generate device pairing QR code.
    Qr,
    /// Webhook helpers and trigger management [*].
    #[command(subcommand)]
    Webhooks(WebhooksCommands),
    /// Interactive onboarding wizard.
    Onboard {
        /// Quick non-interactive mode.
        #[arg(long)]
        quick: bool,
    },
    /// Quick non-interactive initialization.
    Setup {
        /// Quick mode (same as `init --quick`).
        #[arg(long)]
        quick: bool,
    },
    /// Interactive setup wizard for credentials and channels.
    Configure,
    /// Send a one-shot message to an agent.
    Message {
        /// Agent name or ID.
        agent: String,
        /// Message text.
        text: String,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// System info and version [*].
    #[command(subcommand)]
    System(SystemCommands),
    /// Reset local config and state.
    Reset {
        /// Skip confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Subcommand)]
enum VaultCommands {
    /// Initialize the credential vault.
    Init,
    /// Store a credential in the vault.
    Set {
        /// Credential key (env var name).
        key: String,
    },
    /// List all keys in the vault (values are hidden).
    List,
    /// Remove a credential from the vault.
    Remove {
        /// Credential key.
        key: String,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum ScaffoldKind {
    Skill,
    Integration,
}

#[derive(clap::Args)]
struct MigrateArgs {
    /// Source framework to migrate from.
    #[arg(long, value_enum)]
    from: MigrateSourceArg,
    /// Path to the source workspace (auto-detected if not set).
    #[arg(long)]
    source_dir: Option<PathBuf>,
    /// Dry run — show what would be imported without making changes.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, clap::ValueEnum)]
enum MigrateSourceArg {
    Openclaw,
    Langchain,
    Autogpt,
}

#[derive(Subcommand)]
enum SkillCommands {
    /// Install a skill from FangHub or a local directory.
    Install {
        /// Skill name, local path, or git URL.
        source: String,
    },
    /// List installed skills.
    List,
    /// Remove an installed skill.
    Remove {
        /// Skill name.
        name: String,
    },
    /// Search FangHub for skills.
    Search {
        /// Search query.
        query: String,
    },
    /// Create a new skill scaffold.
    Create,
}

#[derive(Subcommand)]
enum ChannelCommands {
    /// List configured channels and their status.
    List,
    /// Interactive setup wizard for a channel.
    Setup {
        /// Channel name (telegram, discord, slack, whatsapp, etc.). Shows picker if omitted.
        channel: Option<String>,
    },
    /// Test a channel by sending a test message.
    Test {
        /// Channel name.
        channel: String,
    },
    /// Enable a channel.
    Enable {
        /// Channel name.
        channel: String,
    },
    /// Disable a channel without removing its configuration.
    Disable {
        /// Channel name.
        channel: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show the current configuration.
    Show,
    /// Open the configuration file in your editor.
    Edit,
    /// Get a config value by dotted key path (e.g. "default_model.provider").
    Get {
        /// Dotted key path (e.g. "default_model.provider", "api_listen").
        key: String,
    },
    /// Set a config value (warning: strips TOML comments).
    Set {
        /// Dotted key path.
        key: String,
        /// New value.
        value: String,
    },
    /// Remove a config key (warning: strips TOML comments).
    Unset {
        /// Dotted key path to remove (e.g. "api.cors_origin").
        key: String,
    },
    /// Save an API key to ~/.openfang/.env (prompts interactively).
    SetKey {
        /// Provider name (groq, anthropic, openai, gemini, deepseek, etc.).
        provider: String,
    },
    /// Remove an API key from ~/.openfang/.env.
    DeleteKey {
        /// Provider name.
        provider: String,
    },
    /// Test provider connectivity with the stored API key.
    TestKey {
        /// Provider name.
        provider: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// Spawn a new agent from a template (interactive or by name).
    New {
        /// Template name (e.g., "coder", "assistant"). Interactive picker if omitted.
        template: Option<String>,
    },
    /// Spawn a new agent from a manifest file.
    Spawn {
        /// Path to the agent manifest TOML file.
        manifest: PathBuf,
    },
    /// List all running agents.
    List {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Interactive chat with an agent.
    Chat {
        /// Agent ID (UUID).
        agent_id: String,
    },
    /// Kill an agent.
    Kill {
        /// Agent ID (UUID).
        agent_id: String,
    },
}

#[derive(Subcommand)]
enum WorkflowCommands {
    /// List all registered workflows.
    List,
    /// Create a workflow from a JSON file.
    Create {
        /// Path to a JSON file describing the workflow.
        file: PathBuf,
    },
    /// Run a workflow by ID.
    Run {
        /// Workflow ID (UUID).
        workflow_id: String,
        /// Input text for the workflow.
        input: String,
    },
}

#[derive(Subcommand)]
enum TriggerCommands {
    /// List all triggers (optionally filtered by agent).
    List {
        /// Optional agent ID to filter by.
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Create a trigger for an agent.
    Create {
        /// Agent ID (UUID) that owns the trigger.
        agent_id: String,
        /// Trigger pattern as JSON (e.g. '{"lifecycle":{}}' or '{"agent_spawned":{"name_pattern":"*"}}').
        pattern_json: String,
        /// Prompt template (use {{event}} placeholder).
        #[arg(long, default_value = "Event: {{event}}")]
        prompt: String,
        /// Maximum number of times to fire (0 = unlimited).
        #[arg(long, default_value = "0")]
        max_fires: u64,
    },
    /// Delete a trigger by ID.
    Delete {
        /// Trigger ID (UUID).
        trigger_id: String,
    },
}

#[derive(Subcommand)]
enum ModelsCommands {
    /// List available models (optionally filter by provider).
    List {
        /// Filter by provider name.
        #[arg(long)]
        provider: Option<String>,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Show model aliases (shorthand names).
    Aliases {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// List known LLM providers and their auth status.
    Providers {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Set the default model for the daemon.
    Set {
        /// Model ID or alias (e.g. "gpt-4o", "claude-sonnet"). Interactive picker if omitted.
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum GatewayCommands {
    /// Start the kernel daemon.
    Start,
    /// Stop the running daemon.
    Stop,
    /// Show daemon status.
    Status {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ApprovalsCommands {
    /// List pending approvals.
    List {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Approve a pending request.
    Approve {
        /// Approval ID.
        id: String,
    },
    /// Reject a pending request.
    Reject {
        /// Approval ID.
        id: String,
    },
}

#[derive(Subcommand)]
enum CronCommands {
    /// List scheduled jobs.
    List {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Create a new scheduled job.
    Create {
        /// Agent name or ID to run.
        agent: String,
        /// Cron expression (e.g. "0 */6 * * *").
        spec: String,
        /// Prompt to send when the job fires.
        prompt: String,
    },
    /// Delete a scheduled job.
    Delete {
        /// Job ID.
        id: String,
    },
    /// Enable a disabled job.
    Enable {
        /// Job ID.
        id: String,
    },
    /// Disable a job without deleting it.
    Disable {
        /// Job ID.
        id: String,
    },
}

#[derive(Subcommand)]
enum SecurityCommands {
    /// Show security status summary.
    Status {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Show recent audit trail entries.
    Audit {
        /// Maximum number of entries to show.
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Verify audit trail integrity (Merkle chain).
    Verify,
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// List KV pairs for an agent.
    List {
        /// Agent name or ID.
        agent: String,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Get a specific KV value.
    Get {
        /// Agent name or ID.
        agent: String,
        /// Key name.
        key: String,
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Set a KV value.
    Set {
        /// Agent name or ID.
        agent: String,
        /// Key name.
        key: String,
        /// Value to store.
        value: String,
    },
    /// Delete a KV pair.
    Delete {
        /// Agent name or ID.
        agent: String,
        /// Key name.
        key: String,
    },
}

#[derive(Subcommand)]
enum DevicesCommands {
    /// List paired devices.
    List {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Start a new device pairing flow.
    Pair,
    /// Remove a paired device.
    Remove {
        /// Device ID.
        id: String,
    },
}

#[derive(Subcommand)]
enum WebhooksCommands {
    /// List configured webhooks.
    List {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Create a new webhook trigger.
    Create {
        /// Agent name or ID.
        agent: String,
        /// Webhook callback URL.
        url: String,
    },
    /// Delete a webhook.
    Delete {
        /// Webhook ID.
        id: String,
    },
    /// Send a test payload to a webhook.
    Test {
        /// Webhook ID.
        id: String,
    },
}

#[derive(Subcommand)]
enum SystemCommands {
    /// Show detailed system info.
    Info {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Show version information.
    Version {
        /// Output as JSON for scripting.
        #[arg(long)]
        json: bool,
    },
}

fn init_tracing_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

/// Redirect tracing to a log file so it doesn't corrupt the ratatui TUI.
fn init_tracing_file() {
    let log_dir = dirs::home_dir()
        .map(|h| h.join(".openfang"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("tui.log");

    match std::fs::File::create(&log_path) {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
                .init();
        }
        Err(_) => {
            // Fallback: suppress all output rather than corrupt the TUI
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::ERROR)
                .with_writer(std::io::sink)
                .init();
        }
    }
}

fn main() {
    // Load ~/.openfang/.env into process environment (system env takes priority).
    dotenv::load_dotenv();

    let cli = Cli::parse();

    // Determine if this invocation launches a ratatui TUI.
    // TUI modes must NOT install the Ctrl+C handler (it calls process::exit
    // which bypasses ratatui::restore and leaves the terminal in raw mode).
    // TUI modes also need file-based tracing (stderr output corrupts the TUI).
    let is_launcher = cli.command.is_none() && std::io::IsTerminal::is_terminal(&std::io::stdout());
    let is_tui_mode = is_launcher
        || matches!(cli.command, Some(Commands::Tui))
        || matches!(cli.command, Some(Commands::Chat { .. }))
        || matches!(
            cli.command,
            Some(Commands::Agent(AgentCommands::Chat { .. }))
        );

    if is_tui_mode {
        init_tracing_file();
    } else {
        // CLI subcommands: install Ctrl+C handler for clean interrupt of
        // blocking read_line calls, and trace to stderr.
        install_ctrlc_handler();
        init_tracing_stderr();
    }

    match cli.command {
        None => {
            if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                // Piped: fall back to text help
                use clap::CommandFactory;
                Cli::command().print_help().unwrap();
                println!();
                return;
            }
            match launcher::run(cli.config.clone()) {
                launcher::LauncherChoice::GetStarted => cmd_init(false),
                launcher::LauncherChoice::Chat => cmd_quick_chat(cli.config, None),
                launcher::LauncherChoice::Dashboard => cmd_dashboard(),
                launcher::LauncherChoice::DesktopApp => launcher::launch_desktop_app(),
                launcher::LauncherChoice::TerminalUI => tui::run(cli.config),
                launcher::LauncherChoice::ShowHelp => {
                    use clap::CommandFactory;
                    Cli::command().print_help().unwrap();
                    println!();
                }
                launcher::LauncherChoice::Quit => {}
            }
        }
        Some(Commands::Tui) => tui::run(cli.config),
        Some(Commands::Init { quick }) => cmd_init(quick),
        Some(Commands::Start) => cmd_start(cli.config),
        Some(Commands::Stop) => cmd_stop(),
        Some(Commands::Agent(sub)) => match sub {
            AgentCommands::New { template } => cmd_agent_new(cli.config, template),
            AgentCommands::Spawn { manifest } => cmd_agent_spawn(cli.config, manifest),
            AgentCommands::List { json } => cmd_agent_list(cli.config, json),
            AgentCommands::Chat { agent_id } => cmd_agent_chat(cli.config, &agent_id),
            AgentCommands::Kill { agent_id } => cmd_agent_kill(cli.config, &agent_id),
        },
        Some(Commands::Workflow(sub)) => match sub {
            WorkflowCommands::List => cmd_workflow_list(),
            WorkflowCommands::Create { file } => cmd_workflow_create(file),
            WorkflowCommands::Run { workflow_id, input } => cmd_workflow_run(&workflow_id, &input),
        },
        Some(Commands::Trigger(sub)) => match sub {
            TriggerCommands::List { agent_id } => cmd_trigger_list(agent_id.as_deref()),
            TriggerCommands::Create {
                agent_id,
                pattern_json,
                prompt,
                max_fires,
            } => cmd_trigger_create(&agent_id, &pattern_json, &prompt, max_fires),
            TriggerCommands::Delete { trigger_id } => cmd_trigger_delete(&trigger_id),
        },
        Some(Commands::Migrate(args)) => cmd_migrate(args),
        Some(Commands::Skill(sub)) => match sub {
            SkillCommands::Install { source } => cmd_skill_install(&source),
            SkillCommands::List => cmd_skill_list(),
            SkillCommands::Remove { name } => cmd_skill_remove(&name),
            SkillCommands::Search { query } => cmd_skill_search(&query),
            SkillCommands::Create => cmd_skill_create(),
        },
        Some(Commands::Channel(sub)) => match sub {
            ChannelCommands::List => cmd_channel_list(),
            ChannelCommands::Setup { channel } => cmd_channel_setup(channel.as_deref()),
            ChannelCommands::Test { channel } => cmd_channel_test(&channel),
            ChannelCommands::Enable { channel } => cmd_channel_toggle(&channel, true),
            ChannelCommands::Disable { channel } => cmd_channel_toggle(&channel, false),
        },
        Some(Commands::Config(sub)) => match sub {
            ConfigCommands::Show => cmd_config_show(),
            ConfigCommands::Edit => cmd_config_edit(),
            ConfigCommands::Get { key } => cmd_config_get(&key),
            ConfigCommands::Set { key, value } => cmd_config_set(&key, &value),
            ConfigCommands::Unset { key } => cmd_config_unset(&key),
            ConfigCommands::SetKey { provider } => cmd_config_set_key(&provider),
            ConfigCommands::DeleteKey { provider } => cmd_config_delete_key(&provider),
            ConfigCommands::TestKey { provider } => cmd_config_test_key(&provider),
        },
        Some(Commands::Chat { agent }) => cmd_quick_chat(cli.config, agent),
        Some(Commands::Status { json }) => cmd_status(cli.config, json),
        Some(Commands::Doctor { json, repair }) => cmd_doctor(json, repair),
        Some(Commands::Dashboard) => cmd_dashboard(),
        Some(Commands::Completion { shell }) => cmd_completion(shell),
        Some(Commands::Mcp) => mcp::run_mcp_server(cli.config),
        Some(Commands::Add { name, key }) => cmd_integration_add(&name, key.as_deref()),
        Some(Commands::Remove { name }) => cmd_integration_remove(&name),
        Some(Commands::Integrations { query }) => cmd_integrations_list(query.as_deref()),
        Some(Commands::Vault(sub)) => match sub {
            VaultCommands::Init => cmd_vault_init(),
            VaultCommands::Set { key } => cmd_vault_set(&key),
            VaultCommands::List => cmd_vault_list(),
            VaultCommands::Remove { key } => cmd_vault_remove(&key),
        },
        Some(Commands::New { kind }) => cmd_scaffold(kind),
        // ── New commands ────────────────────────────────────────────────
        Some(Commands::Models(sub)) => match sub {
            ModelsCommands::List { provider, json } => cmd_models_list(provider.as_deref(), json),
            ModelsCommands::Aliases { json } => cmd_models_aliases(json),
            ModelsCommands::Providers { json } => cmd_models_providers(json),
            ModelsCommands::Set { model } => cmd_models_set(model),
        },
        Some(Commands::Gateway(sub)) => match sub {
            GatewayCommands::Start => cmd_start(cli.config),
            GatewayCommands::Stop => cmd_stop(),
            GatewayCommands::Status { json } => cmd_status(cli.config, json),
        },
        Some(Commands::Approvals(sub)) => match sub {
            ApprovalsCommands::List { json } => cmd_approvals_list(json),
            ApprovalsCommands::Approve { id } => cmd_approvals_respond(&id, true),
            ApprovalsCommands::Reject { id } => cmd_approvals_respond(&id, false),
        },
        Some(Commands::Cron(sub)) => match sub {
            CronCommands::List { json } => cmd_cron_list(json),
            CronCommands::Create {
                agent,
                spec,
                prompt,
            } => cmd_cron_create(&agent, &spec, &prompt),
            CronCommands::Delete { id } => cmd_cron_delete(&id),
            CronCommands::Enable { id } => cmd_cron_toggle(&id, true),
            CronCommands::Disable { id } => cmd_cron_toggle(&id, false),
        },
        Some(Commands::Sessions { agent, json }) => cmd_sessions(agent.as_deref(), json),
        Some(Commands::Logs { lines, follow }) => cmd_logs(lines, follow),
        Some(Commands::Health { json }) => cmd_health(json),
        Some(Commands::Security(sub)) => match sub {
            SecurityCommands::Status { json } => cmd_security_status(json),
            SecurityCommands::Audit { limit, json } => cmd_security_audit(limit, json),
            SecurityCommands::Verify => cmd_security_verify(),
        },
        Some(Commands::Memory(sub)) => match sub {
            MemoryCommands::List { agent, json } => cmd_memory_list(&agent, json),
            MemoryCommands::Get { agent, key, json } => cmd_memory_get(&agent, &key, json),
            MemoryCommands::Set { agent, key, value } => cmd_memory_set(&agent, &key, &value),
            MemoryCommands::Delete { agent, key } => cmd_memory_delete(&agent, &key),
        },
        Some(Commands::Devices(sub)) => match sub {
            DevicesCommands::List { json } => cmd_devices_list(json),
            DevicesCommands::Pair => cmd_devices_pair(),
            DevicesCommands::Remove { id } => cmd_devices_remove(&id),
        },
        Some(Commands::Qr) => cmd_devices_pair(),
        Some(Commands::Webhooks(sub)) => match sub {
            WebhooksCommands::List { json } => cmd_webhooks_list(json),
            WebhooksCommands::Create { agent, url } => cmd_webhooks_create(&agent, &url),
            WebhooksCommands::Delete { id } => cmd_webhooks_delete(&id),
            WebhooksCommands::Test { id } => cmd_webhooks_test(&id),
        },
        Some(Commands::Onboard { quick }) | Some(Commands::Setup { quick }) => cmd_init(quick),
        Some(Commands::Configure) => cmd_init(false),
        Some(Commands::Message { agent, text, json }) => cmd_message(&agent, &text, json),
        Some(Commands::System(sub)) => match sub {
            SystemCommands::Info { json } => cmd_system_info(json),
            SystemCommands::Version { json } => cmd_system_version(json),
        },
        Some(Commands::Reset { confirm }) => cmd_reset(confirm),
    }
}

// ---------------------------------------------------------------------------
// Daemon detection helpers
// ---------------------------------------------------------------------------

/// Try to find a running daemon. Returns its base URL if found.
/// SECURITY: Restrict file permissions to owner-only (0600) on Unix.
#[cfg(unix)]
pub(crate) fn restrict_file_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
pub(crate) fn restrict_file_permissions(_path: &std::path::Path) {}

/// SECURITY: Restrict directory permissions to owner-only (0700) on Unix.
#[cfg(unix)]
pub(crate) fn restrict_dir_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
pub(crate) fn restrict_dir_permissions(_path: &std::path::Path) {}

pub(crate) fn find_daemon() -> Option<String> {
    let home_dir = dirs::home_dir()?.join(".openfang");
    let info = read_daemon_info(&home_dir)?;
    let url = format!("http://{}/api/health", info.listen_addr);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    let resp = client.get(&url).send().ok()?;
    if resp.status().is_success() {
        Some(format!("http://{}", info.listen_addr))
    } else {
        None
    }
}

/// Build an HTTP client for daemon calls.
pub(crate) fn daemon_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("Failed to build HTTP client")
}

/// Helper: send a request to the daemon and parse the JSON body.
/// Exits with error on connection failure.
pub(crate) fn daemon_json(
    resp: Result<reqwest::blocking::Response, reqwest::Error>,
) -> serde_json::Value {
    match resp {
        Ok(r) => {
            let status = r.status();
            let body = r.json::<serde_json::Value>().unwrap_or_default();
            if status.is_server_error() {
                ui::error_with_fix(
                    &format!("Daemon returned error ({})", status),
                    "Check daemon logs: ~/.openfang/tui.log",
                );
            }
            body
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("timed out") || msg.contains("Timeout") {
                ui::error_with_fix(
                    "Request timed out",
                    "The agent may be processing a complex request. Try again, or check `openfang status`",
                );
            } else if msg.contains("Connection refused") || msg.contains("connect") {
                ui::error_with_fix(
                    "Cannot connect to daemon",
                    "Is the daemon running? Start it with: openfang start",
                );
            } else {
                ui::error_with_fix(
                    &format!("Daemon communication error: {msg}"),
                    "Check `openfang status` or restart: openfang start",
                );
            }
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_init(quick: bool) {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            ui::error("Could not determine home directory");
            std::process::exit(1);
        }
    };

    let openfang_dir = home.join(".openfang");

    // --- Ensure directories exist ---
    if !openfang_dir.exists() {
        std::fs::create_dir_all(&openfang_dir).unwrap_or_else(|e| {
            ui::error_with_fix(
                &format!("Failed to create {}", openfang_dir.display()),
                &format!("Check permissions on {}", home.display()),
            );
            eprintln!("  {e}");
            std::process::exit(1);
        });
        restrict_dir_permissions(&openfang_dir);
    }

    for sub in ["data", "agents"] {
        let dir = openfang_dir.join(sub);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
                eprintln!("Error creating {sub} dir: {e}");
                std::process::exit(1);
            });
        }
    }

    // Install bundled agent templates (skips existing ones to preserve user edits)
    bundled_agents::install_bundled_agents(&openfang_dir.join("agents"));

    if quick {
        cmd_init_quick(&openfang_dir);
    } else {
        cmd_init_interactive(&openfang_dir);
    }
}

/// Quick init: no prompts, auto-detect, write config + .env, print next steps.
fn cmd_init_quick(openfang_dir: &std::path::Path) {
    ui::banner();
    ui::blank();

    let (provider, api_key_env, model) = detect_best_provider();

    write_config_if_missing(openfang_dir, provider, model, api_key_env);

    ui::blank();
    ui::success("OpenFang initialized (quick mode)");
    ui::kv("Provider", provider);
    ui::kv("Model", model);
    ui::blank();
    ui::next_steps(&[
        "Start the daemon:  openfang start",
        "Chat:              openfang chat",
    ]);
}

/// Interactive 5-step onboarding wizard (ratatui TUI).
fn cmd_init_interactive(openfang_dir: &std::path::Path) {
    use tui::screens::init_wizard::{self, InitResult, LaunchChoice};

    match init_wizard::run() {
        InitResult::Completed {
            provider,
            model,
            daemon_started,
            launch,
        } => {
            // Print summary after TUI restores terminal
            ui::blank();
            ui::success("OpenFang initialized!");
            ui::kv("Provider", &provider);
            ui::kv("Model", &model);

            if daemon_started {
                ui::kv_ok("Daemon", "running");
            }
            ui::blank();

            // Execute the user's chosen launch action.
            match launch {
                LaunchChoice::Desktop => {
                    launch_desktop_app(openfang_dir);
                }
                LaunchChoice::Dashboard => {
                    if let Some(base) = find_daemon() {
                        let url = format!("{base}/");
                        ui::success(&format!("Opening dashboard at {url}"));
                        if !open_in_browser(&url) {
                            ui::hint(&format!("Could not open browser. Visit: {url}"));
                        }
                    } else {
                        ui::error("Daemon is not running. Start it with: openfang start");
                    }
                }
                LaunchChoice::Chat => {
                    ui::hint("Starting chat session...");
                    ui::blank();
                    // Note: tracing was initialized for stderr (init is a CLI
                    // subcommand).  The chat TUI takes over the terminal with
                    // raw mode so stderr output is suppressed.  We can't
                    // reinitialize tracing (global subscriber is set once).
                    cmd_quick_chat(None, None);
                }
            }
        }
        InitResult::Cancelled => {
            println!("  Setup cancelled.");
        }
    }
}

/// Launch the openfang-desktop Tauri app, connecting to the running daemon.
fn launch_desktop_app(_openfang_dir: &std::path::Path) {
    // Look for the desktop binary next to our own executable.
    let desktop_bin = {
        let exe = std::env::current_exe().ok();
        let dir = exe.as_ref().and_then(|e| e.parent());

        #[cfg(windows)]
        let name = "openfang-desktop.exe";
        #[cfg(not(windows))]
        let name = "openfang-desktop";

        dir.map(|d| d.join(name))
    };

    match desktop_bin {
        Some(ref path) if path.exists() => {
            ui::success("Launching OpenFang Desktop...");
            match std::process::Command::new(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(_) => {
                    ui::success("Desktop app started.");
                }
                Err(e) => {
                    ui::error(&format!("Failed to launch desktop app: {e}"));
                    ui::hint("Try: openfang dashboard");
                }
            }
        }
        _ => {
            ui::error("Desktop app not found.");
            ui::hint("Install it with: cargo install openfang-desktop");
            ui::hint("Falling back to web dashboard...");
            ui::blank();
            if let Some(base) = find_daemon() {
                let url = format!("{base}/");
                if !open_in_browser(&url) {
                    ui::hint(&format!("Visit: {url}"));
                }
            }
        }
    }
}

/// Auto-detect the best available provider.
fn detect_best_provider() -> (&'static str, &'static str, &'static str) {
    let providers = provider_list();

    for (p, env_var, m, display) in &providers {
        if std::env::var(env_var).is_ok() {
            ui::success(&format!("Detected {display} ({env_var})"));
            return (p, env_var, m);
        }
    }
    // Also check GOOGLE_API_KEY
    if std::env::var("GOOGLE_API_KEY").is_ok() {
        ui::success("Detected Gemini (GOOGLE_API_KEY)");
        return ("gemini", "GOOGLE_API_KEY", "gemini-2.5-flash");
    }
    ui::hint("No LLM provider API keys found");
    ui::hint("Groq offers a free tier: https://console.groq.com");
    ("groq", "GROQ_API_KEY", "llama-3.3-70b-versatile")
}

/// Static list of supported providers: (id, env_var, default_model, display_name).
fn provider_list() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("groq", "GROQ_API_KEY", "llama-3.3-70b-versatile", "Groq"),
        ("gemini", "GEMINI_API_KEY", "gemini-2.5-flash", "Gemini"),
        ("deepseek", "DEEPSEEK_API_KEY", "deepseek-chat", "DeepSeek"),
        (
            "anthropic",
            "ANTHROPIC_API_KEY",
            "claude-sonnet-4-20250514",
            "Anthropic",
        ),
        ("openai", "OPENAI_API_KEY", "gpt-4o", "OpenAI"),
        (
            "openrouter",
            "OPENROUTER_API_KEY",
            "openrouter/auto",
            "OpenRouter",
        ),
    ]
}

/// Write config.toml if it doesn't already exist.
fn write_config_if_missing(
    openfang_dir: &std::path::Path,
    provider: &str,
    model: &str,
    api_key_env: &str,
) {
    let config_path = openfang_dir.join("config.toml");
    if config_path.exists() {
        ui::check_ok(&format!("Config already exists: {}", config_path.display()));
    } else {
        let default_config = format!(
            r#"# OpenFang Agent OS configuration
# See https://github.com/RightNow-AI/openfang for documentation

api_listen = "127.0.0.1:4200"

[default_model]
provider = "{provider}"
model = "{model}"
api_key_env = "{api_key_env}"

[memory]
decay_rate = 0.05
"#
        );
        std::fs::write(&config_path, &default_config).unwrap_or_else(|e| {
            ui::error_with_fix("Failed to write config", &e.to_string());
            std::process::exit(1);
        });
        restrict_file_permissions(&config_path);
        ui::success(&format!("Created: {}", config_path.display()));
    }
}

fn cmd_start(config: Option<PathBuf>) {
    if let Some(base) = find_daemon() {
        ui::error_with_fix(
            &format!("Daemon already running at {base}"),
            "Use `openfang status` to check it, or stop it first",
        );
        std::process::exit(1);
    }

    ui::banner();
    ui::blank();
    println!("  Starting daemon...");
    ui::blank();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let kernel = match OpenFangKernel::boot(config.as_deref()) {
            Ok(k) => k,
            Err(e) => {
                boot_kernel_error(&e);
                std::process::exit(1);
            }
        };

        let listen_addr = kernel.config.api_listen.clone();
        let daemon_info_path = kernel.config.home_dir.join("daemon.json");
        let provider = kernel.config.default_model.provider.clone();
        let model = kernel.config.default_model.model.clone();
        let agent_count = kernel.registry.count();
        let model_count = kernel
            .model_catalog
            .read()
            .map(|c| c.list_models().len())
            .unwrap_or(0);

        ui::success(&format!("Kernel booted ({provider}/{model})"));
        if model_count > 0 {
            ui::success(&format!("{model_count} models available"));
        }
        if agent_count > 0 {
            ui::success(&format!("{agent_count} agent(s) loaded"));
        }
        ui::blank();
        ui::kv("API", &format!("http://{listen_addr}"));
        ui::kv("Dashboard", &format!("http://{listen_addr}/"));
        ui::kv("Provider", &provider);
        ui::kv("Model", &model);
        ui::blank();
        ui::hint("Open the dashboard in your browser, or run `openfang chat`");
        ui::hint("Press Ctrl+C to stop the daemon");
        ui::blank();

        if let Err(e) =
            openfang_api::server::run_daemon(kernel, &listen_addr, Some(&daemon_info_path)).await
        {
            ui::error(&format!("Daemon error: {e}"));
            std::process::exit(1);
        }

        ui::blank();
        println!("  OpenFang daemon stopped.");
    });
}

fn cmd_stop() {
    match find_daemon() {
        Some(base) => {
            let client = daemon_client();
            match client.post(format!("{base}/api/shutdown")).send() {
                Ok(r) if r.status().is_success() => {
                    // Wait for daemon to actually stop (up to 5 seconds)
                    for _ in 0..10 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        if find_daemon().is_none() {
                            ui::success("Daemon stopped");
                            return;
                        }
                    }
                    // Still alive — force kill via PID
                    if let Some(home) = dirs::home_dir() {
                        let of_dir = home.join(".openfang");
                        if let Some(info) = read_daemon_info(&of_dir) {
                            force_kill_pid(info.pid);
                            let _ = std::fs::remove_file(of_dir.join("daemon.json"));
                        }
                    }
                    ui::success("Daemon stopped (forced)");
                }
                Ok(r) => {
                    ui::error(&format!("Shutdown request failed ({})", r.status()));
                }
                Err(e) => {
                    ui::error(&format!("Could not reach daemon: {e}"));
                }
            }
        }
        None => {
            ui::warn_with_fix(
                "No running daemon found",
                "Is it running? Check with: openfang status",
            );
        }
    }
}

fn force_kill_pid(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
}

/// Show context-aware error for kernel boot failures.
fn boot_kernel_error(e: &openfang_kernel::error::KernelError) {
    let msg = e.to_string();
    if msg.contains("parse") || msg.contains("toml") || msg.contains("config") {
        ui::error_with_fix(
            "Failed to parse configuration",
            "Check your config.toml syntax: openfang config show",
        );
    } else if msg.contains("database") || msg.contains("locked") || msg.contains("sqlite") {
        ui::error_with_fix(
            "Database error (file may be locked)",
            "Check if another OpenFang process is running: openfang status",
        );
    } else if msg.contains("key") || msg.contains("API") || msg.contains("auth") {
        ui::error_with_fix(
            "LLM provider authentication failed",
            "Run `openfang doctor` to check your API key configuration",
        );
    } else {
        ui::error_with_fix(
            &format!("Failed to boot kernel: {msg}"),
            "Run `openfang doctor` to diagnose the issue",
        );
    }
}

fn cmd_agent_spawn(config: Option<PathBuf>, manifest_path: PathBuf) {
    if !manifest_path.exists() {
        ui::error_with_fix(
            &format!("Manifest file not found: {}", manifest_path.display()),
            "Use `openfang agent new` to spawn from a template instead",
        );
        std::process::exit(1);
    }

    let contents = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
        eprintln!("Error reading manifest: {e}");
        std::process::exit(1);
    });

    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(
            client
                .post(format!("{base}/api/agents"))
                .json(&serde_json::json!({"manifest_toml": contents}))
                .send(),
        );
        if body.get("agent_id").is_some() {
            println!("Agent spawned successfully!");
            println!("  ID:   {}", body["agent_id"].as_str().unwrap_or("?"));
            println!("  Name: {}", body["name"].as_str().unwrap_or("?"));
        } else {
            eprintln!(
                "Failed to spawn agent: {}",
                body["error"].as_str().unwrap_or("Unknown error")
            );
            std::process::exit(1);
        }
    } else {
        let manifest: AgentManifest = toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("Error parsing manifest: {e}");
            std::process::exit(1);
        });
        let kernel = boot_kernel(config);
        match kernel.spawn_agent(manifest) {
            Ok(id) => {
                println!("Agent spawned (in-process mode).");
                println!("  ID: {id}");
                println!("\n  Note: Agent will be lost when this process exits.");
                println!("  For persistent agents, use `openfang start` first.");
            }
            Err(e) => {
                eprintln!("Failed to spawn agent: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_agent_list(config: Option<PathBuf>, json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(client.get(format!("{base}/api/agents")).send());

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            return;
        }

        let agents = body.as_array();

        match agents {
            Some(agents) if agents.is_empty() => println!("No agents running."),
            Some(agents) => {
                println!(
                    "{:<38} {:<16} {:<10} {:<12} MODEL",
                    "ID", "NAME", "STATE", "PROVIDER"
                );
                println!("{}", "-".repeat(95));
                for a in agents {
                    println!(
                        "{:<38} {:<16} {:<10} {:<12} {}",
                        a["id"].as_str().unwrap_or("?"),
                        a["name"].as_str().unwrap_or("?"),
                        a["state"].as_str().unwrap_or("?"),
                        a["model_provider"].as_str().unwrap_or("?"),
                        a["model_name"].as_str().unwrap_or("?"),
                    );
                }
            }
            None => println!("No agents running."),
        }
    } else {
        let kernel = boot_kernel(config);
        let agents = kernel.registry.list();

        if json {
            let list: Vec<serde_json::Value> = agents
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.id.to_string(),
                        "name": e.name,
                        "state": format!("{:?}", e.state),
                        "created_at": e.created_at.to_rfc3339(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&list).unwrap_or_default()
            );
            return;
        }

        if agents.is_empty() {
            println!("No agents running.");
            return;
        }

        println!("{:<38} {:<20} {:<12} CREATED", "ID", "NAME", "STATE");
        println!("{}", "-".repeat(85));
        for entry in agents {
            println!(
                "{:<38} {:<20} {:<12} {}",
                entry.id,
                entry.name,
                format!("{:?}", entry.state),
                entry.created_at.format("%Y-%m-%d %H:%M")
            );
        }
    }
}

fn cmd_agent_chat(config: Option<PathBuf>, agent_id_str: &str) {
    tui::chat_runner::run_chat_tui(config, Some(agent_id_str.to_string()));
}

fn cmd_agent_kill(config: Option<PathBuf>, agent_id_str: &str) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(
            client
                .delete(format!("{base}/api/agents/{agent_id_str}"))
                .send(),
        );
        if body.get("status").is_some() {
            println!("Agent {agent_id_str} killed.");
        } else {
            eprintln!(
                "Failed to kill agent: {}",
                body["error"].as_str().unwrap_or("Unknown error")
            );
            std::process::exit(1);
        }
    } else {
        let agent_id: AgentId = agent_id_str.parse().unwrap_or_else(|_| {
            eprintln!("Invalid agent ID: {agent_id_str}");
            std::process::exit(1);
        });
        let kernel = boot_kernel(config);
        match kernel.kill_agent(agent_id) {
            Ok(()) => println!("Agent {agent_id} killed."),
            Err(e) => {
                eprintln!("Failed to kill agent: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_agent_new(config: Option<PathBuf>, template_name: Option<String>) {
    let all_templates = templates::load_all_templates();
    if all_templates.is_empty() {
        ui::error_with_fix(
            "No agent templates found",
            "Run `openfang init` to set up the agents directory",
        );
        std::process::exit(1);
    }

    // Resolve template: by name or interactive picker
    let chosen = match template_name {
        Some(ref name) => match all_templates.iter().find(|t| t.name == *name) {
            Some(t) => t,
            None => {
                ui::error_with_fix(
                    &format!("Template '{name}' not found"),
                    "Run `openfang agent new` to see available templates",
                );
                std::process::exit(1);
            }
        },
        None => {
            ui::section("Available Agent Templates");
            ui::blank();
            for (i, t) in all_templates.iter().enumerate() {
                let desc = if t.description.is_empty() {
                    String::new()
                } else {
                    format!("  {}", t.description)
                };
                println!(
                    "    {:>2}. {:<22}{}",
                    i + 1,
                    t.name,
                    colored::Colorize::dimmed(desc.as_str())
                );
            }
            ui::blank();
            let choice = prompt_input("  Choose template [1]: ");
            let idx = if choice.is_empty() {
                0
            } else {
                choice
                    .parse::<usize>()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(all_templates.len() - 1)
            };
            &all_templates[idx]
        }
    };

    // Spawn the agent
    spawn_template_agent(config, chosen);
}

/// Spawn an agent from a template, via daemon or in-process.
fn spawn_template_agent(config: Option<PathBuf>, template: &templates::AgentTemplate) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(
            client
                .post(format!("{base}/api/agents"))
                .json(&serde_json::json!({"manifest_toml": template.content}))
                .send(),
        );
        if let Some(id) = body["agent_id"].as_str() {
            ui::blank();
            ui::success(&format!("Agent '{}' spawned", template.name));
            ui::kv("ID", id);
            if let Some(model) = body["model_name"].as_str() {
                let provider = body["model_provider"].as_str().unwrap_or("?");
                ui::kv("Model", &format!("{provider}/{model}"));
            }
            ui::blank();
            ui::hint(&format!("Chat: openfang chat {}", template.name));
        } else {
            ui::error(&format!(
                "Failed to spawn: {}",
                body["error"].as_str().unwrap_or("Unknown error")
            ));
            std::process::exit(1);
        }
    } else {
        let manifest: AgentManifest = toml::from_str(&template.content).unwrap_or_else(|e| {
            ui::error_with_fix(
                &format!("Failed to parse template '{}': {e}", template.name),
                "The template manifest may be corrupted",
            );
            std::process::exit(1);
        });
        let kernel = boot_kernel(config);
        match kernel.spawn_agent(manifest) {
            Ok(id) => {
                ui::blank();
                ui::success(&format!("Agent '{}' spawned (in-process)", template.name));
                ui::kv("ID", &id.to_string());
                ui::blank();
                ui::hint(&format!("Chat: openfang chat {}", template.name));
                ui::hint("Note: Agent will be lost when this process exits");
                ui::hint("For persistent agents, use `openfang start` first");
            }
            Err(e) => {
                ui::error(&format!("Failed to spawn agent: {e}"));
                std::process::exit(1);
            }
        }
    }
}

fn cmd_status(config: Option<PathBuf>, json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(client.get(format!("{base}/api/status")).send());

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            return;
        }

        ui::section("OpenFang Daemon Status");
        ui::blank();
        ui::kv_ok("Status", body["status"].as_str().unwrap_or("?"));
        ui::kv(
            "Agents",
            &body["agent_count"].as_u64().unwrap_or(0).to_string(),
        );
        ui::kv("Provider", body["default_provider"].as_str().unwrap_or("?"));
        ui::kv("Model", body["default_model"].as_str().unwrap_or("?"));
        ui::kv("API", &base);
        ui::kv("Dashboard", &format!("{base}/"));
        ui::kv("Data dir", body["data_dir"].as_str().unwrap_or("?"));
        ui::kv(
            "Uptime",
            &format!("{}s", body["uptime_seconds"].as_u64().unwrap_or(0)),
        );

        if let Some(agents) = body["agents"].as_array() {
            if !agents.is_empty() {
                ui::blank();
                ui::section("Active Agents");
                for a in agents {
                    println!(
                        "    {} ({}) -- {} [{}:{}]",
                        a["name"].as_str().unwrap_or("?"),
                        a["id"].as_str().unwrap_or("?"),
                        a["state"].as_str().unwrap_or("?"),
                        a["model_provider"].as_str().unwrap_or("?"),
                        a["model_name"].as_str().unwrap_or("?"),
                    );
                }
            }
        }
    } else {
        let kernel = boot_kernel(config);
        let agent_count = kernel.registry.count();

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "in-process",
                    "agent_count": agent_count,
                    "data_dir": kernel.config.data_dir.display().to_string(),
                    "default_provider": kernel.config.default_model.provider,
                    "default_model": kernel.config.default_model.model,
                    "daemon": false,
                }))
                .unwrap_or_default()
            );
            return;
        }

        ui::section("OpenFang Status (in-process)");
        ui::blank();
        ui::kv("Agents", &agent_count.to_string());
        ui::kv("Provider", &kernel.config.default_model.provider);
        ui::kv("Model", &kernel.config.default_model.model);
        ui::kv("Data dir", &kernel.config.data_dir.display().to_string());
        ui::kv_warn("Daemon", "NOT RUNNING");
        ui::blank();
        ui::hint("Run `openfang start` to launch the daemon");

        if agent_count > 0 {
            ui::blank();
            ui::section("Persisted Agents");
            for entry in kernel.registry.list() {
                println!("    {} ({}) -- {:?}", entry.name, entry.id, entry.state);
            }
        }
    }
}

fn cmd_doctor(json: bool, repair: bool) {
    let mut checks: Vec<serde_json::Value> = Vec::new();
    let mut all_ok = true;
    let mut repaired = false;

    if !json {
        ui::step("OpenFang Doctor");
        println!();
    }

    let home = dirs::home_dir();
    if let Some(h) = &home {
        let openfang_dir = h.join(".openfang");

        // --- Check 1: OpenFang directory ---
        if openfang_dir.exists() {
            if !json {
                ui::check_ok(&format!("OpenFang directory: {}", openfang_dir.display()));
            }
            checks.push(serde_json::json!({"check": "openfang_dir", "status": "ok", "path": openfang_dir.display().to_string()}));
        } else if repair {
            if !json {
                ui::check_fail("OpenFang directory not found.");
            }
            let answer = prompt_input("    Create it now? [Y/n] ");
            if answer.is_empty() || answer.starts_with('y') || answer.starts_with('Y') {
                if std::fs::create_dir_all(&openfang_dir).is_ok() {
                    restrict_dir_permissions(&openfang_dir);
                    for sub in ["data", "agents"] {
                        let _ = std::fs::create_dir_all(openfang_dir.join(sub));
                    }
                    if !json {
                        ui::check_ok("Created OpenFang directory");
                    }
                    repaired = true;
                } else {
                    if !json {
                        ui::check_fail("Failed to create directory");
                    }
                    all_ok = false;
                }
            } else {
                all_ok = false;
            }
            checks.push(serde_json::json!({"check": "openfang_dir", "status": if repaired { "repaired" } else { "fail" }}));
        } else {
            if !json {
                ui::check_fail("OpenFang directory not found. Run `openfang init` first.");
            }
            checks.push(serde_json::json!({"check": "openfang_dir", "status": "fail"}));
            all_ok = false;
        }

        // --- Check 2: .env file exists + permissions ---
        let env_path = openfang_dir.join(".env");
        if env_path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&env_path) {
                    let mode = meta.permissions().mode() & 0o777;
                    if mode == 0o600 {
                        if !json {
                            ui::check_ok(".env file (permissions OK)");
                        }
                    } else if repair {
                        let _ = std::fs::set_permissions(
                            &env_path,
                            std::fs::Permissions::from_mode(0o600),
                        );
                        if !json {
                            ui::check_ok(".env file (permissions fixed to 0600)");
                        }
                        repaired = true;
                    } else if !json {
                        ui::check_warn(&format!(
                            ".env file has loose permissions ({:o}), should be 0600",
                            mode
                        ));
                    }
                } else if !json {
                    ui::check_ok(".env file");
                }
            }
            #[cfg(not(unix))]
            {
                if !json {
                    ui::check_ok(".env file");
                }
            }
            checks.push(serde_json::json!({"check": "env_file", "status": "ok"}));
        } else {
            if !json {
                ui::check_warn(
                    ".env file not found (create with: openfang config set-key <provider>)",
                );
            }
            checks.push(serde_json::json!({"check": "env_file", "status": "warn"}));
        }

        // --- Check 3: Config TOML syntax validation ---
        let config_path = openfang_dir.join("config.toml");
        if config_path.exists() {
            let config_content = std::fs::read_to_string(&config_path).unwrap_or_default();
            match toml::from_str::<toml::Value>(&config_content) {
                Ok(_) => {
                    if !json {
                        ui::check_ok(&format!("Config file: {}", config_path.display()));
                    }
                    checks.push(serde_json::json!({"check": "config_file", "status": "ok"}));
                }
                Err(e) => {
                    if !json {
                        ui::check_fail(&format!("Config file has syntax errors: {e}"));
                        ui::hint("Fix with: openfang config edit");
                    }
                    checks.push(serde_json::json!({"check": "config_syntax", "status": "fail", "error": e.to_string()}));
                    all_ok = false;
                }
            }
        } else if repair {
            if !json {
                ui::check_fail("Config file not found.");
            }
            let answer = prompt_input("    Create default config? [Y/n] ");
            if answer.is_empty() || answer.starts_with('y') || answer.starts_with('Y') {
                let default_config = r#"# OpenFang Agent OS configuration
# See https://github.com/RightNow-AI/openfang for documentation

api_listen = "127.0.0.1:4200"

[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"

[memory]
decay_rate = 0.05
"#;
                let _ = std::fs::create_dir_all(&openfang_dir);
                if std::fs::write(&config_path, default_config).is_ok() {
                    restrict_file_permissions(&config_path);
                    if !json {
                        ui::check_ok("Created default config.toml");
                    }
                    repaired = true;
                } else {
                    if !json {
                        ui::check_fail("Failed to create config.toml");
                    }
                    all_ok = false;
                }
            } else {
                all_ok = false;
            }
            checks.push(serde_json::json!({"check": "config_file", "status": if repaired { "repaired" } else { "fail" }}));
        } else {
            if !json {
                ui::check_fail("Config file not found.");
            }
            checks.push(serde_json::json!({"check": "config_file", "status": "fail"}));
            all_ok = false;
        }

        // --- Check 4: Port 4200 availability ---
        if !json {
            println!();
        }
        let daemon_running = find_daemon();
        if let Some(ref base) = daemon_running {
            if !json {
                ui::check_ok(&format!("Daemon running at {base}"));
            }
            checks.push(serde_json::json!({"check": "daemon", "status": "ok", "url": base}));
        } else {
            if !json {
                ui::check_warn("Daemon not running (start with `openfang start`)");
            }
            checks.push(serde_json::json!({"check": "daemon", "status": "warn"}));

            // Check if port 4200 is available
            match std::net::TcpListener::bind("127.0.0.1:4200") {
                Ok(_) => {
                    if !json {
                        ui::check_ok("Port 4200 is available");
                    }
                    checks.push(serde_json::json!({"check": "port_4200", "status": "ok"}));
                }
                Err(_) => {
                    if !json {
                        ui::check_warn("Port 4200 is in use by another process");
                    }
                    checks.push(serde_json::json!({"check": "port_4200", "status": "warn"}));
                }
            }
        }

        // --- Check 5: Stale daemon.json ---
        let daemon_json_path = openfang_dir.join("daemon.json");
        if daemon_json_path.exists() && daemon_running.is_none() {
            if repair {
                let _ = std::fs::remove_file(&daemon_json_path);
                if !json {
                    ui::check_ok("Removed stale daemon.json");
                }
                repaired = true;
            } else if !json {
                ui::check_warn(
                    "Stale daemon.json found (daemon not running). Run with --repair to clean up.",
                );
            }
            checks.push(serde_json::json!({"check": "stale_daemon_json", "status": if repair { "repaired" } else { "warn" }}));
        }

        // --- Check 6: Database file ---
        let db_path = openfang_dir.join("data").join("openfang.db");
        if db_path.exists() {
            // Quick SQLite magic bytes check
            if let Ok(bytes) = std::fs::read(&db_path) {
                if bytes.len() >= 16 && bytes.starts_with(b"SQLite format 3") {
                    if !json {
                        ui::check_ok("Database file (valid SQLite)");
                    }
                    checks.push(serde_json::json!({"check": "database", "status": "ok"}));
                } else {
                    if !json {
                        ui::check_fail("Database file exists but is not valid SQLite");
                    }
                    checks.push(serde_json::json!({"check": "database", "status": "fail"}));
                    all_ok = false;
                }
            }
        } else {
            if !json {
                ui::check_warn("No database file (will be created on first run)");
            }
            checks.push(serde_json::json!({"check": "database", "status": "warn"}));
        }

        // --- Check 7: Disk space ---
        #[cfg(unix)]
        {
            if let Ok(output) = std::process::Command::new("df")
                .args(["-m", &openfang_dir.display().to_string()])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse the available MB from df output (4th column of 2nd line)
                if let Some(line) = stdout.lines().nth(1) {
                    let cols: Vec<&str> = line.split_whitespace().collect();
                    if cols.len() >= 4 {
                        if let Ok(available_mb) = cols[3].parse::<u64>() {
                            if available_mb < 100 {
                                if !json {
                                    ui::check_warn(&format!(
                                        "Low disk space: {available_mb}MB available"
                                    ));
                                }
                                checks.push(serde_json::json!({"check": "disk_space", "status": "warn", "available_mb": available_mb}));
                            } else {
                                if !json {
                                    ui::check_ok(&format!(
                                        "Disk space: {available_mb}MB available"
                                    ));
                                }
                                checks.push(serde_json::json!({"check": "disk_space", "status": "ok", "available_mb": available_mb}));
                            }
                        }
                    }
                }
            }
        }

        // --- Check 8: Agent manifests parse correctly ---
        let agents_dir = openfang_dir.join("agents");
        if agents_dir.exists() {
            let mut agent_errors = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&agents_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Err(e) = toml::from_str::<AgentManifest>(&content) {
                                agent_errors.push((
                                    path.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                    e.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
            if agent_errors.is_empty() {
                if !json {
                    ui::check_ok("Agent manifests are valid");
                }
                checks.push(serde_json::json!({"check": "agent_manifests", "status": "ok"}));
            } else {
                for (file, err) in &agent_errors {
                    if !json {
                        ui::check_fail(&format!("Invalid manifest {file}: {err}"));
                    }
                }
                checks.push(serde_json::json!({"check": "agent_manifests", "status": "fail", "errors": agent_errors.len()}));
                all_ok = false;
            }
        }
    } else {
        if !json {
            ui::check_fail("Could not determine home directory");
        }
        checks.push(serde_json::json!({"check": "home_dir", "status": "fail"}));
        all_ok = false;
    }

    // --- LLM providers ---
    if !json {
        println!("\n  LLM Providers:");
    }
    let provider_keys = [
        ("GROQ_API_KEY", "Groq", "groq"),
        ("OPENROUTER_API_KEY", "OpenRouter", "openrouter"),
        ("ANTHROPIC_API_KEY", "Anthropic", "anthropic"),
        ("OPENAI_API_KEY", "OpenAI", "openai"),
        ("DEEPSEEK_API_KEY", "DeepSeek", "deepseek"),
        ("GEMINI_API_KEY", "Gemini", "gemini"),
        ("GOOGLE_API_KEY", "Google", "google"),
        ("TOGETHER_API_KEY", "Together", "together"),
        ("MISTRAL_API_KEY", "Mistral", "mistral"),
        ("FIREWORKS_API_KEY", "Fireworks", "fireworks"),
    ];

    let mut any_key_set = false;
    for (env_var, name, provider_id) in &provider_keys {
        let set = std::env::var(env_var).is_ok();
        if set {
            // --- Check 9: Live key validation ---
            let valid = test_api_key(provider_id, env_var);
            if valid {
                if !json {
                    ui::provider_status(name, env_var, true);
                }
            } else if !json {
                ui::check_warn(&format!("{name} ({env_var}) - key rejected (401/403)"));
            }
            any_key_set = true;
            checks.push(serde_json::json!({"check": "provider", "name": name, "env_var": env_var, "status": if valid { "ok" } else { "warn" }, "live_test": !valid}));
        } else {
            if !json {
                ui::provider_status(name, env_var, false);
            }
            checks.push(serde_json::json!({"check": "provider", "name": name, "env_var": env_var, "status": "warn"}));
        }
    }

    if !any_key_set {
        if !json {
            println!();
            ui::check_fail("No LLM provider API keys found!");
            ui::blank();
            ui::section("Getting an API key (free tiers)");
            ui::suggest_cmd("Groq:", "https://console.groq.com       (free, fast)");
            ui::suggest_cmd("Gemini:", "https://aistudio.google.com    (free tier)");
            ui::suggest_cmd("DeepSeek:", "https://platform.deepseek.com  (low cost)");
            ui::blank();
            ui::hint("Or run: openfang config set-key groq");
        }
        all_ok = false;
    }

    // --- Check 10: Channel token format validation ---
    if !json {
        println!("\n  Channel Integrations:");
    }
    let channel_keys = [
        ("TELEGRAM_BOT_TOKEN", "Telegram"),
        ("DISCORD_BOT_TOKEN", "Discord"),
        ("SLACK_APP_TOKEN", "Slack App"),
        ("SLACK_BOT_TOKEN", "Slack Bot"),
    ];
    for (env_var, name) in &channel_keys {
        let set = std::env::var(env_var).is_ok();
        if set {
            // Format validation
            let val = std::env::var(env_var).unwrap_or_default();
            let format_ok = match *env_var {
                "TELEGRAM_BOT_TOKEN" => val.contains(':'), // Telegram tokens have format "123456:ABC-DEF..."
                "DISCORD_BOT_TOKEN" => val.len() > 50,     // Discord tokens are typically 59+ chars
                "SLACK_APP_TOKEN" => val.starts_with("xapp-"),
                "SLACK_BOT_TOKEN" => val.starts_with("xoxb-"),
                _ => true,
            };
            if format_ok {
                if !json {
                    ui::provider_status(name, env_var, true);
                }
            } else if !json {
                ui::check_warn(&format!("{name} ({env_var}) - unexpected token format"));
            }
            checks.push(serde_json::json!({"check": "channel", "name": name, "env_var": env_var, "status": if format_ok { "ok" } else { "warn" }}));
        } else {
            if !json {
                ui::provider_status(name, env_var, false);
            }
            checks.push(serde_json::json!({"check": "channel", "name": name, "env_var": env_var, "status": "warn"}));
        }
    }

    // --- Check 11: .env keys vs config api_key_env consistency ---
    if let Some(ref h) = home {
        let openfang_dir = h.join(".openfang");
        let config_path = openfang_dir.join("config.toml");
        if config_path.exists() {
            let config_str = std::fs::read_to_string(&config_path).unwrap_or_default();
            // Look for api_key_env references in config
            for line in config_str.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("api_key_env") {
                    if let Some(val_part) = rest.strip_prefix('=') {
                        let val = val_part.trim().trim_matches('"');
                        if !val.is_empty() && std::env::var(val).is_err() {
                            if !json {
                                ui::check_warn(&format!(
                                    "Config references {val} but it is not set in env or .env"
                                ));
                            }
                            checks.push(serde_json::json!({"check": "env_consistency", "status": "warn", "missing_var": val}));
                        }
                    }
                }
            }
        }
    }

    // --- Check 12: Config deserialization into KernelConfig ---
    if let Some(ref h) = home {
        let openfang_dir = h.join(".openfang");
        let config_path = openfang_dir.join("config.toml");
        if config_path.exists() {
            if !json {
                println!("\n  Config Validation:");
            }
            let config_content = std::fs::read_to_string(&config_path).unwrap_or_default();
            match toml::from_str::<openfang_types::config::KernelConfig>(&config_content) {
                Ok(cfg) => {
                    if !json {
                        ui::check_ok("Config deserializes into KernelConfig");
                    }
                    checks.push(serde_json::json!({"check": "config_deser", "status": "ok"}));

                    // Check exec policy
                    let mode = format!("{:?}", cfg.exec_policy.mode);
                    let safe_bins_count = cfg.exec_policy.safe_bins.len();
                    if !json {
                        ui::check_ok(&format!(
                            "Exec policy: mode={mode}, safe_bins={safe_bins_count}"
                        ));
                    }
                    checks.push(serde_json::json!({"check": "exec_policy", "status": "ok", "mode": mode, "safe_bins": safe_bins_count}));

                    // Check includes
                    if !cfg.include.is_empty() {
                        let mut include_ok = true;
                        for inc in &cfg.include {
                            let inc_path = openfang_dir.join(inc);
                            if inc_path.exists() {
                                if !json {
                                    ui::check_ok(&format!("Include file: {inc}"));
                                }
                            } else if repair {
                                if !json {
                                    ui::check_warn(&format!("Include file missing: {inc}"));
                                }
                                include_ok = false;
                            } else {
                                if !json {
                                    ui::check_fail(&format!("Include file not found: {inc}"));
                                }
                                include_ok = false;
                                all_ok = false;
                            }
                        }
                        checks.push(serde_json::json!({"check": "config_includes", "status": if include_ok { "ok" } else { "fail" }, "count": cfg.include.len()}));
                    }

                    // Check MCP server configs
                    if !cfg.mcp_servers.is_empty() {
                        let mcp_count = cfg.mcp_servers.len();
                        if !json {
                            ui::check_ok(&format!("MCP servers configured: {mcp_count}"));
                        }
                        for server in &cfg.mcp_servers {
                            // Validate transport config
                            match &server.transport {
                                openfang_types::config::McpTransportEntry::Stdio {
                                    command,
                                    ..
                                } => {
                                    if command.is_empty() {
                                        if !json {
                                            ui::check_warn(&format!(
                                                "MCP server '{}' has empty command",
                                                server.name
                                            ));
                                        }
                                        checks.push(serde_json::json!({"check": "mcp_server_config", "status": "warn", "name": server.name}));
                                    }
                                }
                                openfang_types::config::McpTransportEntry::Sse { url } => {
                                    if url.is_empty() {
                                        if !json {
                                            ui::check_warn(&format!(
                                                "MCP server '{}' has empty URL",
                                                server.name
                                            ));
                                        }
                                        checks.push(serde_json::json!({"check": "mcp_server_config", "status": "warn", "name": server.name}));
                                    }
                                }
                            }
                        }
                        checks.push(serde_json::json!({"check": "mcp_servers", "status": "ok", "count": mcp_count}));
                    }
                }
                Err(e) => {
                    if !json {
                        ui::check_fail(&format!("Config fails KernelConfig deserialization: {e}"));
                    }
                    checks.push(serde_json::json!({"check": "config_deser", "status": "fail", "error": e.to_string()}));
                    all_ok = false;
                }
            }
        }
    }

    // --- Check 13: Skill registry health ---
    {
        if !json {
            println!("\n  Skills:");
        }
        let skills_dir = home
            .as_ref()
            .map(|h| h.join(".openfang").join("skills"))
            .unwrap_or_else(|| std::path::PathBuf::from("skills"));
        let mut skill_reg = openfang_skills::registry::SkillRegistry::new(skills_dir.clone());
        skill_reg.load_bundled();
        let bundled_count = skill_reg.count();
        if !json {
            ui::check_ok(&format!("Bundled skills loaded: {bundled_count}"));
        }
        checks.push(
            serde_json::json!({"check": "bundled_skills", "status": "ok", "count": bundled_count}),
        );

        // Check workspace skills if home dir available
        if skills_dir.exists() {
            match skill_reg.load_workspace_skills(&skills_dir) {
                Ok(_) => {
                    let total = skill_reg.count();
                    let ws_count = total.saturating_sub(bundled_count);
                    if ws_count > 0 {
                        if !json {
                            ui::check_ok(&format!("Workspace skills loaded: {ws_count}"));
                        }
                        checks.push(serde_json::json!({"check": "workspace_skills", "status": "ok", "count": ws_count}));
                    }
                }
                Err(e) => {
                    if !json {
                        ui::check_warn(&format!("Failed to load workspace skills: {e}"));
                    }
                    checks.push(serde_json::json!({"check": "workspace_skills", "status": "warn", "error": e.to_string()}));
                }
            }
        }

        // Check for prompt injection issues in skill definitions
        let skills = skill_reg.list();
        let mut injection_warnings = 0;
        for skill in &skills {
            if let Some(ref prompt) = skill.manifest.prompt_context {
                let warnings = openfang_skills::verify::SkillVerifier::scan_prompt_content(prompt);
                if !warnings.is_empty() {
                    injection_warnings += 1;
                    if !json {
                        ui::check_warn(&format!(
                            "Prompt injection warning in skill: {}",
                            skill.manifest.skill.name
                        ));
                    }
                }
            }
        }
        if injection_warnings > 0 {
            checks.push(serde_json::json!({"check": "skill_injection_scan", "status": "warn", "warnings": injection_warnings}));
        } else {
            if !json {
                ui::check_ok("All skills pass prompt injection scan");
            }
            checks.push(serde_json::json!({"check": "skill_injection_scan", "status": "ok"}));
        }
    }

    // --- Check 14: Extension registry health ---
    if let Some(ref h) = home {
        if !json {
            println!("\n  Extensions:");
        }
        let openfang_dir = h.join(".openfang");
        let mut ext_registry =
            openfang_extensions::registry::IntegrationRegistry::new(&openfang_dir);
        ext_registry.load_bundled();
        let _ = ext_registry.load_installed();
        let template_count = ext_registry.template_count();
        let installed_count = ext_registry.installed_count();
        if !json {
            ui::check_ok(&format!(
                "Available integration templates: {template_count}"
            ));
            ui::check_ok(&format!("Installed integrations: {installed_count}"));
        }
        checks.push(serde_json::json!({"check": "extensions_available", "status": "ok", "count": template_count}));
        checks.push(serde_json::json!({"check": "extensions_installed", "status": "ok", "count": installed_count}));
    }

    // --- Check 15: Daemon health detail (if running) ---
    if let Some(ref base) = find_daemon() {
        if !json {
            println!("\n  Daemon Health:");
        }
        let client = daemon_client();
        match client.get(format!("{base}/api/health/detail")).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if let Some(agents) = body.get("agent_count").and_then(|v| v.as_u64()) {
                        if !json {
                            ui::check_ok(&format!("Running agents: {agents}"));
                        }
                        checks.push(serde_json::json!({"check": "daemon_agents", "status": "ok", "count": agents}));
                    }
                    if let Some(uptime) = body.get("uptime_secs").and_then(|v| v.as_u64()) {
                        let hours = uptime / 3600;
                        let mins = (uptime % 3600) / 60;
                        if !json {
                            ui::check_ok(&format!("Daemon uptime: {hours}h {mins}m"));
                        }
                        checks.push(serde_json::json!({"check": "daemon_uptime", "status": "ok", "secs": uptime}));
                    }
                    if let Some(db_status) = body.get("database").and_then(|v| v.as_str()) {
                        if db_status == "ok" {
                            if !json {
                                ui::check_ok("Database connectivity: OK");
                            }
                        } else {
                            if !json {
                                ui::check_fail(&format!("Database status: {db_status}"));
                            }
                            all_ok = false;
                        }
                        checks.push(serde_json::json!({"check": "daemon_db", "status": db_status}));
                    }
                }
            }
            Ok(resp) => {
                if !json {
                    ui::check_warn(&format!("Health detail returned {}", resp.status()));
                }
                checks.push(serde_json::json!({"check": "daemon_health", "status": "warn"}));
            }
            Err(e) => {
                if !json {
                    ui::check_warn(&format!("Failed to query daemon health: {e}"));
                }
                checks.push(serde_json::json!({"check": "daemon_health", "status": "warn", "error": e.to_string()}));
            }
        }

        // Check skills endpoint
        match client.get(format!("{base}/api/skills")).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if let Some(arr) = body.as_array() {
                        if !json {
                            ui::check_ok(&format!("Skills loaded in daemon: {}", arr.len()));
                        }
                        checks.push(serde_json::json!({"check": "daemon_skills", "status": "ok", "count": arr.len()}));
                    }
                }
            }
            _ => {}
        }

        // Check MCP servers endpoint
        match client.get(format!("{base}/api/mcp/servers")).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if let Some(arr) = body.as_array() {
                        let connected = arr
                            .iter()
                            .filter(|s| {
                                s.get("connected")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                            })
                            .count();
                        if !json {
                            ui::check_ok(&format!(
                                "MCP servers: {} configured, {} connected",
                                arr.len(),
                                connected
                            ));
                        }
                        checks.push(serde_json::json!({"check": "daemon_mcp", "status": "ok", "configured": arr.len(), "connected": connected}));
                    }
                }
            }
            _ => {}
        }

        // Check extensions health endpoint
        match client.get(format!("{base}/api/integrations/health")).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if let Some(obj) = body.as_object() {
                        let healthy = obj
                            .values()
                            .filter(|v| v.get("healthy").and_then(|h| h.as_bool()).unwrap_or(false))
                            .count();
                        let total = obj.len();
                        if healthy == total {
                            if !json {
                                ui::check_ok(&format!(
                                    "Integration health: {healthy}/{total} healthy"
                                ));
                            }
                        } else if !json {
                            ui::check_warn(&format!(
                                "Integration health: {healthy}/{total} healthy"
                            ));
                        }
                        checks.push(serde_json::json!({"check": "integration_health", "status": if healthy == total { "ok" } else { "warn" }, "healthy": healthy, "total": total}));
                    }
                }
            }
            _ => {}
        }
    }

    if !json {
        println!();
    }
    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !json {
                ui::check_ok(&format!("Rust: {version}"));
            }
            checks.push(serde_json::json!({"check": "rust", "status": "ok", "version": version}));
        }
        Err(_) => {
            if !json {
                ui::check_fail("Rust toolchain not found");
            }
            checks.push(serde_json::json!({"check": "rust", "status": "fail"}));
            all_ok = false;
        }
    }

    // Python runtime check
    match std::process::Command::new("python3")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !json {
                ui::check_ok(&format!("Python: {version}"));
            }
            checks.push(serde_json::json!({"check": "python", "status": "ok", "version": version}));
        }
        _ => {
            // Try `python` instead
            match std::process::Command::new("python")
                .arg("--version")
                .output()
            {
                Ok(output) if output.status.success() => {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !json {
                        ui::check_ok(&format!("Python: {version}"));
                    }
                    checks.push(
                        serde_json::json!({"check": "python", "status": "ok", "version": version}),
                    );
                }
                _ => {
                    if !json {
                        ui::check_warn("Python not found (needed for Python skill runtime)");
                    }
                    checks.push(serde_json::json!({"check": "python", "status": "warn"}));
                }
            }
        }
    }

    // Node.js runtime check
    match std::process::Command::new("node").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !json {
                ui::check_ok(&format!("Node.js: {version}"));
            }
            checks.push(serde_json::json!({"check": "node", "status": "ok", "version": version}));
        }
        _ => {
            if !json {
                ui::check_warn("Node.js not found (needed for Node skill runtime)");
            }
            checks.push(serde_json::json!({"check": "node", "status": "warn"}));
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "all_ok": all_ok,
                "checks": checks,
            }))
            .unwrap_or_default()
        );
    } else {
        println!();
        if all_ok {
            ui::success("All checks passed! OpenFang is ready.");
            ui::hint("Start the daemon: openfang start");
        } else if repaired {
            ui::success("Repairs applied. Re-run `openfang doctor` to verify.");
        } else {
            ui::error("Some checks failed.");
            if !repair {
                ui::hint("Run `openfang doctor --repair` to attempt auto-fix");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard command
// ---------------------------------------------------------------------------

fn cmd_dashboard() {
    let base = if let Some(url) = find_daemon() {
        url
    } else {
        // Auto-start the daemon
        ui::hint("No daemon running — starting one now...");
        match start_daemon_background() {
            Ok(url) => {
                ui::success("Daemon started");
                url
            }
            Err(e) => {
                ui::error_with_fix(
                    &format!("Could not start daemon: {e}"),
                    "Start it manually: openfang start",
                );
                std::process::exit(1);
            }
        }
    };

    let url = format!("{base}/");
    ui::success(&format!("Opening dashboard at {url}"));
    if copy_to_clipboard(&url) {
        ui::hint("URL copied to clipboard");
    }
    if !open_in_browser(&url) {
        ui::hint(&format!("Could not open browser. Visit: {url}"));
    }
}

/// Copy text to the system clipboard. Returns true on success.
fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to set clipboard (handles special characters better than cmd)
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Set-Clipboard '{}'", text.replace('\'', "''")),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Write as IoWrite;
        std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                child.wait()
            })
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        use std::io::Write as IoWrite;
        // Try xclip first, then xsel
        let result = std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                child.wait()
            })
            .map(|s| s.success())
            .unwrap_or(false);
        if result {
            return true;
        }
        std::process::Command::new("xsel")
            .args(["--clipboard", "--input"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                child.wait()
            })
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        false
    }
}

/// Try to open a URL in the default browser. Returns true on success.
pub(crate) fn open_in_browser(url: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        false
    }
}

// ---------------------------------------------------------------------------
// Shell completion command
// ---------------------------------------------------------------------------

fn cmd_completion(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "openfang", &mut std::io::stdout());
}

// ---------------------------------------------------------------------------
// Workflow commands
// ---------------------------------------------------------------------------

fn cmd_workflow_list() {
    let base = require_daemon("workflow list");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/workflows")).send());

    match body.as_array() {
        Some(workflows) if workflows.is_empty() => println!("No workflows registered."),
        Some(workflows) => {
            println!("{:<38} {:<20} {:<6} CREATED", "ID", "NAME", "STEPS");
            println!("{}", "-".repeat(80));
            for w in workflows {
                println!(
                    "{:<38} {:<20} {:<6} {}",
                    w["id"].as_str().unwrap_or("?"),
                    w["name"].as_str().unwrap_or("?"),
                    w["steps"].as_u64().unwrap_or(0),
                    w["created_at"].as_str().unwrap_or("?"),
                );
            }
        }
        None => println!("No workflows registered."),
    }
}

fn cmd_workflow_create(file: PathBuf) {
    let base = require_daemon("workflow create");
    if !file.exists() {
        eprintln!("Workflow file not found: {}", file.display());
        std::process::exit(1);
    }
    let contents = std::fs::read_to_string(&file).unwrap_or_else(|e| {
        eprintln!("Error reading workflow file: {e}");
        std::process::exit(1);
    });
    let json_body: serde_json::Value = serde_json::from_str(&contents).unwrap_or_else(|e| {
        eprintln!("Invalid JSON: {e}");
        std::process::exit(1);
    });

    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/workflows"))
            .json(&json_body)
            .send(),
    );

    if let Some(id) = body["workflow_id"].as_str() {
        println!("Workflow created successfully!");
        println!("  ID: {id}");
    } else {
        eprintln!(
            "Failed to create workflow: {}",
            body["error"].as_str().unwrap_or("Unknown error")
        );
        std::process::exit(1);
    }
}

fn cmd_workflow_run(workflow_id: &str, input: &str) {
    let base = require_daemon("workflow run");
    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/workflows/{workflow_id}/run"))
            .json(&serde_json::json!({"input": input}))
            .send(),
    );

    if let Some(output) = body["output"].as_str() {
        println!("Workflow completed!");
        println!("  Run ID: {}", body["run_id"].as_str().unwrap_or("?"));
        println!("  Output:\n{output}");
    } else {
        eprintln!(
            "Workflow failed: {}",
            body["error"].as_str().unwrap_or("Unknown error")
        );
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Trigger commands
// ---------------------------------------------------------------------------

fn cmd_trigger_list(agent_id: Option<&str>) {
    let base = require_daemon("trigger list");
    let client = daemon_client();

    let url = match agent_id {
        Some(id) => format!("{base}/api/triggers?agent_id={id}"),
        None => format!("{base}/api/triggers"),
    };
    let body = daemon_json(client.get(&url).send());

    match body.as_array() {
        Some(triggers) if triggers.is_empty() => println!("No triggers registered."),
        Some(triggers) => {
            println!(
                "{:<38} {:<38} {:<8} {:<6} PATTERN",
                "TRIGGER ID", "AGENT ID", "ENABLED", "FIRES"
            );
            println!("{}", "-".repeat(110));
            for t in triggers {
                println!(
                    "{:<38} {:<38} {:<8} {:<6} {}",
                    t["id"].as_str().unwrap_or("?"),
                    t["agent_id"].as_str().unwrap_or("?"),
                    t["enabled"].as_bool().unwrap_or(false),
                    t["fire_count"].as_u64().unwrap_or(0),
                    t["pattern"],
                );
            }
        }
        None => println!("No triggers registered."),
    }
}

fn cmd_trigger_create(agent_id: &str, pattern_json: &str, prompt: &str, max_fires: u64) {
    let base = require_daemon("trigger create");
    let pattern: serde_json::Value = serde_json::from_str(pattern_json).unwrap_or_else(|e| {
        eprintln!("Invalid pattern JSON: {e}");
        eprintln!("Examples:");
        eprintln!("  '{{\"lifecycle\":{{}}}}'");
        eprintln!("  '{{\"agent_spawned\":{{\"name_pattern\":\"*\"}}}}'");
        eprintln!("  '{{\"agent_terminated\":{{}}}}'");
        eprintln!("  '{{\"all\":{{}}}}'");
        std::process::exit(1);
    });

    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/triggers"))
            .json(&serde_json::json!({
                "agent_id": agent_id,
                "pattern": pattern,
                "prompt_template": prompt,
                "max_fires": max_fires,
            }))
            .send(),
    );

    if let Some(id) = body["trigger_id"].as_str() {
        println!("Trigger created successfully!");
        println!("  Trigger ID: {id}");
        println!("  Agent ID:   {agent_id}");
    } else {
        eprintln!(
            "Failed to create trigger: {}",
            body["error"].as_str().unwrap_or("Unknown error")
        );
        std::process::exit(1);
    }
}

fn cmd_trigger_delete(trigger_id: &str) {
    let base = require_daemon("trigger delete");
    let client = daemon_client();
    let body = daemon_json(
        client
            .delete(format!("{base}/api/triggers/{trigger_id}"))
            .send(),
    );

    if body.get("status").is_some() {
        println!("Trigger {trigger_id} deleted.");
    } else {
        eprintln!(
            "Failed to delete trigger: {}",
            body["error"].as_str().unwrap_or("Unknown error")
        );
        std::process::exit(1);
    }
}

/// Require a running daemon — exit with helpful message if not found.
fn require_daemon(command: &str) -> String {
    find_daemon().unwrap_or_else(|| {
        ui::error_with_fix(
            &format!("`openfang {command}` requires a running daemon"),
            "Start the daemon: openfang start",
        );
        ui::hint("Or try `openfang chat` which works without a daemon");
        std::process::exit(1);
    })
}

fn boot_kernel(config: Option<PathBuf>) -> OpenFangKernel {
    match OpenFangKernel::boot(config.as_deref()) {
        Ok(k) => k,
        Err(e) => {
            boot_kernel_error(&e);
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Migrate command
// ---------------------------------------------------------------------------

fn cmd_migrate(args: MigrateArgs) {
    let source = match args.from {
        MigrateSourceArg::Openclaw => openfang_migrate::MigrateSource::OpenClaw,
        MigrateSourceArg::Langchain => openfang_migrate::MigrateSource::LangChain,
        MigrateSourceArg::Autogpt => openfang_migrate::MigrateSource::AutoGpt,
    };

    let source_dir = args.source_dir.unwrap_or_else(|| {
        let home = dirs::home_dir().unwrap_or_else(|| {
            eprintln!("Error: Could not determine home directory");
            std::process::exit(1);
        });
        match source {
            openfang_migrate::MigrateSource::OpenClaw => home.join(".openclaw"),
            openfang_migrate::MigrateSource::LangChain => home.join(".langchain"),
            openfang_migrate::MigrateSource::AutoGpt => home.join("Auto-GPT"),
        }
    });

    let target_dir = dirs::home_dir()
        .unwrap_or_else(|| {
            eprintln!("Error: Could not determine home directory");
            std::process::exit(1);
        })
        .join(".openfang");

    println!("Migrating from {} ({})...", source, source_dir.display());
    if args.dry_run {
        println!("  (dry run — no changes will be made)\n");
    }

    let options = openfang_migrate::MigrateOptions {
        source,
        source_dir,
        target_dir,
        dry_run: args.dry_run,
    };

    match openfang_migrate::run_migration(&options) {
        Ok(report) => {
            report.print_summary();

            // Save migration report
            if !args.dry_run {
                let report_path = options.target_dir.join("migration_report.md");
                if let Err(e) = std::fs::write(&report_path, report.to_markdown()) {
                    eprintln!("Warning: Could not save migration report: {e}");
                } else {
                    println!("\n  Report saved to: {}", report_path.display());
                }
            }
        }
        Err(e) => {
            eprintln!("Migration failed: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Skill commands
// ---------------------------------------------------------------------------

fn cmd_skill_install(source: &str) {
    let home = openfang_home();
    let skills_dir = home.join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap_or_else(|e| {
        eprintln!("Error creating skills directory: {e}");
        std::process::exit(1);
    });

    let source_path = PathBuf::from(source);
    if source_path.exists() && source_path.is_dir() {
        // Local directory install
        let manifest_path = source_path.join("skill.toml");
        if !manifest_path.exists() {
            // Check if it's an OpenClaw skill
            if openfang_skills::openclaw_compat::detect_openclaw_skill(&source_path) {
                println!("Detected OpenClaw skill format. Converting...");
                match openfang_skills::openclaw_compat::convert_openclaw_skill(&source_path) {
                    Ok(manifest) => {
                        let dest = skills_dir.join(&manifest.skill.name);
                        // Copy skill directory
                        copy_dir_recursive(&source_path, &dest);
                        if let Err(e) = openfang_skills::openclaw_compat::write_openfang_manifest(
                            &dest, &manifest,
                        ) {
                            eprintln!("Failed to write manifest: {e}");
                            std::process::exit(1);
                        }
                        println!("Installed OpenClaw skill: {}", manifest.skill.name);
                    }
                    Err(e) => {
                        eprintln!("Failed to convert OpenClaw skill: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            eprintln!("No skill.toml found in {source}");
            std::process::exit(1);
        }

        // Read manifest to get skill name
        let toml_str = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
            eprintln!("Error reading skill.toml: {e}");
            std::process::exit(1);
        });
        let manifest: openfang_skills::SkillManifest =
            toml::from_str(&toml_str).unwrap_or_else(|e| {
                eprintln!("Error parsing skill.toml: {e}");
                std::process::exit(1);
            });

        let dest = skills_dir.join(&manifest.skill.name);
        copy_dir_recursive(&source_path, &dest);
        println!(
            "Installed skill: {} v{}",
            manifest.skill.name, manifest.skill.version
        );
    } else {
        // Remote install from FangHub
        println!("Installing {source} from FangHub...");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = openfang_skills::marketplace::MarketplaceClient::new(
            openfang_skills::marketplace::MarketplaceConfig::default(),
        );
        match rt.block_on(client.install(source, &skills_dir)) {
            Ok(version) => println!("Installed {source} {version}"),
            Err(e) => {
                eprintln!("Failed to install skill: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_skill_list() {
    let home = openfang_home();
    let skills_dir = home.join("skills");

    let mut registry = openfang_skills::registry::SkillRegistry::new(skills_dir);
    match registry.load_all() {
        Ok(0) => println!("No skills installed."),
        Ok(count) => {
            println!("{count} skill(s) installed:\n");
            println!(
                "{:<20} {:<10} {:<8} DESCRIPTION",
                "NAME", "VERSION", "TOOLS"
            );
            println!("{}", "-".repeat(70));
            for skill in registry.list() {
                println!(
                    "{:<20} {:<10} {:<8} {}",
                    skill.manifest.skill.name,
                    skill.manifest.skill.version,
                    skill.manifest.tools.provided.len(),
                    skill.manifest.skill.description,
                );
            }
        }
        Err(e) => {
            eprintln!("Error loading skills: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_skill_remove(name: &str) {
    let home = openfang_home();
    let skills_dir = home.join("skills");

    let mut registry = openfang_skills::registry::SkillRegistry::new(skills_dir);
    let _ = registry.load_all();
    match registry.remove(name) {
        Ok(()) => println!("Removed skill: {name}"),
        Err(e) => {
            eprintln!("Failed to remove skill: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_skill_search(query: &str) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = openfang_skills::marketplace::MarketplaceClient::new(
        openfang_skills::marketplace::MarketplaceConfig::default(),
    );
    match rt.block_on(client.search(query)) {
        Ok(results) if results.is_empty() => println!("No skills found for \"{query}\"."),
        Ok(results) => {
            println!("Skills matching \"{query}\":\n");
            for r in results {
                println!("  {} ({})", r.name, r.stars);
                if !r.description.is_empty() {
                    println!("    {}", r.description);
                }
                println!("    {}", r.url);
                println!();
            }
        }
        Err(e) => {
            eprintln!("Search failed: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_skill_create() {
    let name = prompt_input("Skill name: ");
    let description = prompt_input("Description: ");
    let runtime = prompt_input("Runtime (python/node/wasm) [python]: ");
    let runtime = if runtime.is_empty() {
        "python".to_string()
    } else {
        runtime
    };

    let home = openfang_home();
    let skill_dir = home.join("skills").join(&name);
    std::fs::create_dir_all(skill_dir.join("src")).unwrap_or_else(|e| {
        eprintln!("Error creating skill directory: {e}");
        std::process::exit(1);
    });

    let manifest = format!(
        r#"[skill]
name = "{name}"
version = "0.1.0"
description = "{description}"
author = ""
license = "MIT"
tags = []

[runtime]
type = "{runtime}"
entry = "src/main.py"

[[tools.provided]]
name = "{tool_name}"
description = "{description}"
input_schema = {{ type = "object", properties = {{ input = {{ type = "string" }} }}, required = ["input"] }}

[requirements]
tools = []
capabilities = []
"#,
        tool_name = name.replace('-', "_"),
    );

    std::fs::write(skill_dir.join("skill.toml"), &manifest).unwrap();

    // Create entry point
    let entry_content = match runtime.as_str() {
        "python" => format!(
            r#"#!/usr/bin/env python3
"""OpenFang skill: {name}"""
import json
import sys

def main():
    payload = json.loads(sys.stdin.read())
    tool_name = payload["tool"]
    input_data = payload["input"]

    # TODO: Implement your skill logic here
    result = {{"result": f"Processed: {{input_data.get('input', '')}}"}}

    print(json.dumps(result))

if __name__ == "__main__":
    main()
"#
        ),
        _ => "// TODO: Implement your skill\n".to_string(),
    };

    let entry_path = if runtime == "python" {
        "src/main.py"
    } else {
        "src/index.js"
    };
    std::fs::write(skill_dir.join(entry_path), entry_content).unwrap();

    println!("\nSkill created: {}", skill_dir.display());
    println!("\nFiles:");
    println!("  skill.toml");
    println!("  {entry_path}");
    println!("\nNext steps:");
    println!("  1. Edit the entry point to implement your skill logic");
    println!("  2. Test locally: openfang skill test");
    println!(
        "  3. Install: openfang skill install {}",
        skill_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Channel commands
// ---------------------------------------------------------------------------

fn cmd_channel_list() {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        println!("No configuration found. Run `openfang init` first.");
        return;
    }

    let config_str = std::fs::read_to_string(&config_path).unwrap_or_default();

    println!("Channel Integrations:\n");
    println!("{:<12} {:<10} STATUS", "CHANNEL", "ENV VAR");
    println!("{}", "-".repeat(50));

    let channels: Vec<(&str, &str)> = vec![
        ("webchat", ""),
        ("telegram", "TELEGRAM_BOT_TOKEN"),
        ("discord", "DISCORD_BOT_TOKEN"),
        ("slack", "SLACK_BOT_TOKEN"),
        ("whatsapp", "WA_ACCESS_TOKEN"),
        ("signal", ""),
        ("matrix", "MATRIX_TOKEN"),
        ("email", "EMAIL_PASSWORD"),
    ];

    for (name, env_var) in channels {
        let configured = config_str.contains(&format!("[channels.{name}]"));
        let env_set = if env_var.is_empty() {
            true
        } else {
            std::env::var(env_var).is_ok()
        };

        let status = match (configured, env_set) {
            (true, true) => "Ready",
            (true, false) => "Missing env",
            (false, _) => "Not configured",
        };

        println!(
            "{:<12} {:<10} {}",
            name,
            if env_var.is_empty() { "—" } else { env_var },
            status,
        );
    }

    println!("\nUse `openfang channel setup <channel>` to configure a channel.");
}

fn cmd_channel_setup(channel: Option<&str>) {
    let channel = match channel {
        Some(c) => c.to_string(),
        None => {
            // Interactive channel picker
            ui::section("Channel Setup");
            ui::blank();
            let channel_list = [
                ("telegram", "Telegram bot (BotFather)"),
                ("discord", "Discord bot"),
                ("slack", "Slack app (Socket Mode)"),
                ("whatsapp", "WhatsApp Cloud API"),
                ("email", "Email (IMAP/SMTP)"),
                ("signal", "Signal (signal-cli)"),
                ("matrix", "Matrix homeserver"),
            ];

            for (i, (name, desc)) in channel_list.iter().enumerate() {
                println!("    {:>2}. {:<12} {}", i + 1, name, desc.dimmed());
            }
            ui::blank();

            let choice = prompt_input("  Choose channel [1]: ");
            let idx = if choice.is_empty() {
                0
            } else {
                choice
                    .parse::<usize>()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(channel_list.len() - 1)
            };
            channel_list[idx].0.to_string()
        }
    };

    match channel.as_str() {
        "telegram" => {
            ui::section("Setting up Telegram");
            ui::blank();
            println!("  1. Open Telegram and message @BotFather");
            println!("  2. Send /newbot and follow the prompts");
            println!("  3. Copy the bot token");
            ui::blank();

            let token = prompt_input("  Paste your bot token: ");
            if token.is_empty() {
                ui::error("No token provided. Setup cancelled.");
                return;
            }

            let config_block = "\n[channels.telegram]\nbot_token_env = \"TELEGRAM_BOT_TOKEN\"\ndefault_agent = \"assistant\"\n";
            maybe_write_channel_config("telegram", config_block);

            // Save token to .env
            match dotenv::save_env_key("TELEGRAM_BOT_TOKEN", &token) {
                Ok(()) => ui::success("Token saved to ~/.openfang/.env"),
                Err(_) => println!("    export TELEGRAM_BOT_TOKEN={token}"),
            }

            ui::blank();
            ui::success("Telegram configured");
            notify_daemon_restart();
        }
        "discord" => {
            ui::section("Setting up Discord");
            ui::blank();
            println!("  1. Go to https://discord.com/developers/applications");
            println!("  2. Create a New Application");
            println!("  3. Go to Bot section and click 'Add Bot'");
            println!("  4. Copy the bot token");
            println!("  5. Under Privileged Gateway Intents, enable:");
            println!("     - Message Content Intent");
            println!("  6. Use OAuth2 URL Generator to invite bot to your server");
            ui::blank();

            let token = prompt_input("  Paste your bot token: ");
            if token.is_empty() {
                ui::error("No token provided. Setup cancelled.");
                return;
            }

            let config_block = "\n[channels.discord]\nbot_token_env = \"DISCORD_BOT_TOKEN\"\ndefault_agent = \"coder\"\n";
            maybe_write_channel_config("discord", config_block);

            match dotenv::save_env_key("DISCORD_BOT_TOKEN", &token) {
                Ok(()) => ui::success("Token saved to ~/.openfang/.env"),
                Err(_) => println!("    export DISCORD_BOT_TOKEN={token}"),
            }

            ui::blank();
            ui::success("Discord configured");
            notify_daemon_restart();
        }
        "slack" => {
            ui::section("Setting up Slack");
            ui::blank();
            println!("  1. Go to https://api.slack.com/apps");
            println!("  2. Create New App -> From Scratch");
            println!("  3. Enable Socket Mode (Settings -> Socket Mode)");
            println!("  4. Copy the App-Level Token (xapp-...)");
            println!("  5. Go to OAuth & Permissions, add scopes:");
            println!("     - chat:write, app_mentions:read, im:history");
            println!("  6. Install to workspace and copy Bot Token (xoxb-...)");
            ui::blank();

            let app_token = prompt_input("  Paste your App Token (xapp-...): ");
            let bot_token = prompt_input("  Paste your Bot Token (xoxb-...): ");

            let config_block = "\n[channels.slack]\napp_token_env = \"SLACK_APP_TOKEN\"\nbot_token_env = \"SLACK_BOT_TOKEN\"\ndefault_agent = \"assistant\"\n";
            maybe_write_channel_config("slack", config_block);

            if !app_token.is_empty() {
                match dotenv::save_env_key("SLACK_APP_TOKEN", &app_token) {
                    Ok(()) => ui::success("App token saved to ~/.openfang/.env"),
                    Err(_) => println!("    export SLACK_APP_TOKEN={app_token}"),
                }
            }
            if !bot_token.is_empty() {
                match dotenv::save_env_key("SLACK_BOT_TOKEN", &bot_token) {
                    Ok(()) => ui::success("Bot token saved to ~/.openfang/.env"),
                    Err(_) => println!("    export SLACK_BOT_TOKEN={bot_token}"),
                }
            }

            ui::blank();
            ui::success("Slack configured");
            notify_daemon_restart();
        }
        "whatsapp" => {
            ui::section("Setting up WhatsApp");
            ui::blank();
            println!("  WhatsApp Cloud API (recommended for production):");
            println!("  1. Go to https://developers.facebook.com");
            println!("  2. Create a Business App");
            println!("  3. Add WhatsApp product");
            println!("  4. Set up a test phone number");
            println!("  5. Copy Phone Number ID and Access Token");
            ui::blank();

            let phone_id = prompt_input("  Phone Number ID: ");
            let access_token = prompt_input("  Access Token: ");
            let verify_token = prompt_input("  Verify Token: ");

            let config_block = "\n[channels.whatsapp]\nmode = \"cloud_api\"\nphone_number_id_env = \"WA_PHONE_ID\"\naccess_token_env = \"WA_ACCESS_TOKEN\"\nverify_token_env = \"WA_VERIFY_TOKEN\"\nwebhook_port = 8443\ndefault_agent = \"assistant\"\n";
            maybe_write_channel_config("whatsapp", config_block);

            for (key, val) in [
                ("WA_PHONE_ID", &phone_id),
                ("WA_ACCESS_TOKEN", &access_token),
                ("WA_VERIFY_TOKEN", &verify_token),
            ] {
                if !val.is_empty() {
                    match dotenv::save_env_key(key, val) {
                        Ok(()) => ui::success(&format!("{key} saved to ~/.openfang/.env")),
                        Err(_) => println!("    export {key}={val}"),
                    }
                }
            }

            ui::blank();
            ui::success("WhatsApp configured");
            notify_daemon_restart();
        }
        "email" => {
            ui::section("Setting up Email");
            ui::blank();
            println!("  For Gmail, use an App Password:");
            println!("  https://myaccount.google.com/apppasswords");
            ui::blank();

            let username = prompt_input("  Email address: ");
            if username.is_empty() {
                ui::error("No email provided. Setup cancelled.");
                return;
            }

            let password = prompt_input("  App password (or Enter to set later): ");

            let config_block = format!(
                "\n[channels.email]\nimap_host = \"imap.gmail.com\"\nimap_port = 993\nsmtp_host = \"smtp.gmail.com\"\nsmtp_port = 587\nusername = \"{username}\"\npassword_env = \"EMAIL_PASSWORD\"\npoll_interval = 30\ndefault_agent = \"assistant\"\n"
            );
            maybe_write_channel_config("email", &config_block);

            if !password.is_empty() {
                match dotenv::save_env_key("EMAIL_PASSWORD", &password) {
                    Ok(()) => ui::success("Password saved to ~/.openfang/.env"),
                    Err(_) => println!("    export EMAIL_PASSWORD=your_app_password"),
                }
            } else {
                ui::hint("Set later: openfang config set-key email (or export EMAIL_PASSWORD=...)");
            }

            ui::blank();
            ui::success("Email configured");
            notify_daemon_restart();
        }
        "signal" => {
            ui::section("Setting up Signal");
            ui::blank();
            println!("  Signal requires signal-cli (https://github.com/AsamK/signal-cli).");
            ui::blank();
            println!("  1. Install signal-cli:");
            println!("     - macOS: brew install signal-cli");
            println!("     - Linux: download from GitHub releases");
            println!("     - Or use the Docker image");
            println!("  2. Register or link a phone number:");
            println!("     signal-cli -u +1YOURPHONE register");
            println!("     signal-cli -u +1YOURPHONE verify CODE");
            println!("  3. Start signal-cli in JSON-RPC mode:");
            println!("     signal-cli -u +1YOURPHONE jsonRpc --socket /tmp/signal-cli.sock");
            ui::blank();

            let phone = prompt_input("  Your phone number (+1XXXX, or Enter to skip): ");

            let config_block = "\n[channels.signal]\nphone_env = \"SIGNAL_PHONE\"\nsocket_path = \"/tmp/signal-cli.sock\"\ndefault_agent = \"assistant\"\n";
            maybe_write_channel_config("signal", config_block);

            if !phone.is_empty() {
                match dotenv::save_env_key("SIGNAL_PHONE", &phone) {
                    Ok(()) => ui::success("Phone saved to ~/.openfang/.env"),
                    Err(_) => println!("    export SIGNAL_PHONE={phone}"),
                }
            }

            ui::blank();
            ui::success("Signal configured");
            notify_daemon_restart();
        }
        "matrix" => {
            ui::section("Setting up Matrix");
            ui::blank();
            println!("  1. Create a bot account on your Matrix homeserver");
            println!("     (e.g., register @openfang-bot:matrix.org)");
            println!("  2. Obtain an access token:");
            println!("     curl -X POST https://matrix.org/_matrix/client/r0/login \\");
            println!("       -d '{{\"type\":\"m.login.password\",\"user\":\"openfang-bot\",\"password\":\"...\"}}'");
            println!("     Copy the access_token from the response.");
            println!("  3. Invite the bot to rooms you want it to monitor.");
            ui::blank();

            let homeserver = prompt_input("  Homeserver URL [https://matrix.org]: ");
            let homeserver = if homeserver.is_empty() {
                "https://matrix.org".to_string()
            } else {
                homeserver
            };
            let token = prompt_input("  Access token: ");

            let config_block = "\n[channels.matrix]\nhomeserver_env = \"MATRIX_HOMESERVER\"\naccess_token_env = \"MATRIX_ACCESS_TOKEN\"\ndefault_agent = \"assistant\"\n";
            maybe_write_channel_config("matrix", config_block);

            let _ = dotenv::save_env_key("MATRIX_HOMESERVER", &homeserver);
            if !token.is_empty() {
                match dotenv::save_env_key("MATRIX_ACCESS_TOKEN", &token) {
                    Ok(()) => ui::success("Token saved to ~/.openfang/.env"),
                    Err(_) => println!("    export MATRIX_ACCESS_TOKEN={token}"),
                }
            }

            ui::blank();
            ui::success("Matrix configured");
            notify_daemon_restart();
        }
        other => {
            ui::error_with_fix(
                &format!("Unknown channel: {other}"),
                "Available: telegram, discord, slack, whatsapp, email, signal, matrix",
            );
            std::process::exit(1);
        }
    }
}

/// Offer to append a channel config block to config.toml if it doesn't already exist.
fn maybe_write_channel_config(channel: &str, config_block: &str) {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        ui::hint("No config.toml found. Run `openfang init` first.");
        return;
    }

    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    let section_header = format!("[channels.{channel}]");
    if existing.contains(&section_header) {
        ui::check_ok(&format!("{section_header} already in config.toml"));
        return;
    }

    let answer = prompt_input("  Write to config.toml? [Y/n] ");
    if answer.is_empty() || answer.starts_with('y') || answer.starts_with('Y') {
        let mut content = existing;
        content.push_str(config_block);
        if std::fs::write(&config_path, &content).is_ok() {
            restrict_file_permissions(&config_path);
            ui::check_ok(&format!("Added {section_header} to config.toml"));
        } else {
            ui::check_fail("Failed to write config.toml");
        }
    }
}

/// After channel config changes, warn user if daemon is running.
fn notify_daemon_restart() {
    if find_daemon().is_some() {
        ui::check_warn("Restart the daemon to activate this channel");
    } else {
        ui::hint("Start the daemon: openfang start");
    }
}

fn cmd_channel_test(channel: &str) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(
            client
                .post(format!("{base}/api/channels/{channel}/test"))
                .send(),
        );
        if body.get("status").is_some() {
            println!("Test message sent to {channel}!");
        } else {
            eprintln!(
                "Failed: {}",
                body["error"].as_str().unwrap_or("Unknown error")
            );
        }
    } else {
        eprintln!("Channel test requires a running daemon. Start with: openfang start");
        std::process::exit(1);
    }
}

fn cmd_channel_toggle(channel: &str, enable: bool) {
    let action = if enable { "enabled" } else { "disabled" };
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let endpoint = if enable { "enable" } else { "disable" };
        let body = daemon_json(
            client
                .post(format!("{base}/api/channels/{channel}/{endpoint}"))
                .send(),
        );
        if body.get("status").is_some() {
            println!("Channel {channel} {action}.");
        } else {
            eprintln!(
                "Failed: {}",
                body["error"].as_str().unwrap_or("Unknown error")
            );
        }
    } else {
        println!("Note: Channel {channel} will be {action} when the daemon starts.");
        println!("Edit ~/.openfang/config.toml to persist this change.");
    }
}

// ---------------------------------------------------------------------------
// Provider / API key helpers
// ---------------------------------------------------------------------------

/// Map a provider name to its conventional environment variable name.
fn provider_to_env_var(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "groq" => "GROQ_API_KEY".to_string(),
        "anthropic" => "ANTHROPIC_API_KEY".to_string(),
        "openai" => "OPENAI_API_KEY".to_string(),
        "gemini" => "GEMINI_API_KEY".to_string(),
        "google" => "GOOGLE_API_KEY".to_string(),
        "deepseek" => "DEEPSEEK_API_KEY".to_string(),
        "openrouter" => "OPENROUTER_API_KEY".to_string(),
        "together" => "TOGETHER_API_KEY".to_string(),
        "mistral" => "MISTRAL_API_KEY".to_string(),
        "fireworks" => "FIREWORKS_API_KEY".to_string(),
        "perplexity" => "PERPLEXITY_API_KEY".to_string(),
        "cohere" => "COHERE_API_KEY".to_string(),
        "xai" => "XAI_API_KEY".to_string(),
        "brave" => "BRAVE_API_KEY".to_string(),
        "tavily" => "TAVILY_API_KEY".to_string(),
        other => format!("{}_API_KEY", other.to_uppercase()),
    }
}

/// Test an API key by hitting the provider's models/health endpoint.
///
/// Returns true if the key is accepted (status != 401/403).
/// Returns true on timeout/network errors (best-effort — don't block setup).
pub(crate) fn test_api_key(provider: &str, env_var: &str) -> bool {
    let key = match std::env::var(env_var) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return true, // can't build client — assume ok
    };

    let result = match provider.to_lowercase().as_str() {
        "groq" => client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(&key)
            .send(),
        "anthropic" => client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .send(),
        "openai" => client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(&key)
            .send(),
        "gemini" | "google" => client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={key}"
            ))
            .send(),
        "deepseek" => client
            .get("https://api.deepseek.com/models")
            .bearer_auth(&key)
            .send(),
        "openrouter" => client
            .get("https://openrouter.ai/api/v1/models")
            .bearer_auth(&key)
            .send(),
        _ => return true, // unknown provider — skip test
    };

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            status != 401 && status != 403
        }
        Err(_) => true, // network error — don't block setup
    }
}

// ---------------------------------------------------------------------------
// Background daemon start
// ---------------------------------------------------------------------------

/// Spawn `openfang start` as a detached background process.
///
/// Polls for daemon health for up to 10 seconds. Returns the daemon URL on success.
pub(crate) fn start_daemon_background() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot find executable: {e}"))?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        std::process::Command::new(&exe)
            .arg("start")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon: {e}"))?;
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new(&exe)
            .arg("start")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon: {e}"))?;
    }

    // Poll for daemon readiness
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(url) = find_daemon() {
            return Ok(url);
        }
    }

    Err("Daemon did not become ready within 10 seconds".to_string())
}

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

fn cmd_config_show() {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        println!("No configuration found at: {}", config_path.display());
        println!("Run `openfang init` to create one.");
        return;
    }

    let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
        eprintln!("Error reading config: {e}");
        std::process::exit(1);
    });

    println!("# {}\n", config_path.display());
    println!("{content}");
}

fn cmd_config_edit() {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Editor exited with: {s}");
        }
        Err(e) => {
            eprintln!("Failed to open editor '{editor}': {e}");
            eprintln!("Set $EDITOR to your preferred editor.");
        }
    }
}

fn cmd_config_get(key: &str) {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        ui::error_with_fix("No config file found", "Run `openfang init` first");
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
        ui::error(&format!("Failed to read config: {e}"));
        std::process::exit(1);
    });

    let table: toml::Value = toml::from_str(&content).unwrap_or_else(|e| {
        ui::error_with_fix(
            &format!("Config parse error: {e}"),
            "Fix your config.toml syntax, or run `openfang config edit`",
        );
        std::process::exit(1);
    });

    // Navigate dotted path
    let mut current = &table;
    for part in key.split('.') {
        match current.get(part) {
            Some(v) => current = v,
            None => {
                ui::error(&format!("Key not found: {key}"));
                std::process::exit(1);
            }
        }
    }

    // Print value
    match current {
        toml::Value::String(s) => println!("{s}"),
        toml::Value::Integer(i) => println!("{i}"),
        toml::Value::Float(f) => println!("{f}"),
        toml::Value::Boolean(b) => println!("{b}"),
        other => println!("{other}"),
    }
}

fn cmd_config_set(key: &str, value: &str) {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        ui::error_with_fix("No config file found", "Run `openfang init` first");
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
        ui::error(&format!("Failed to read config: {e}"));
        std::process::exit(1);
    });

    let mut table: toml::Value = toml::from_str(&content).unwrap_or_else(|e| {
        ui::error_with_fix(
            &format!("Config parse error: {e}"),
            "Fix your config.toml syntax first",
        );
        std::process::exit(1);
    });

    // Navigate to parent and set key
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        ui::error("Empty key");
        std::process::exit(1);
    }

    let mut current = &mut table;
    for part in &parts[..parts.len() - 1] {
        current = current
            .as_table_mut()
            .and_then(|t| t.get_mut(*part))
            .unwrap_or_else(|| {
                ui::error(&format!("Key path not found: {key}"));
                std::process::exit(1);
            });
    }

    let last_key = parts[parts.len() - 1];
    let tbl = current.as_table_mut().unwrap_or_else(|| {
        ui::error(&format!("Parent of '{key}' is not a table"));
        std::process::exit(1);
    });

    // Try to preserve type: if the existing value is an integer, parse as int, etc.
    let new_value = if let Some(existing) = tbl.get(last_key) {
        match existing {
            toml::Value::Integer(_) => value
                .parse::<i64>()
                .map(toml::Value::Integer)
                .unwrap_or_else(|_| toml::Value::String(value.to_string())),
            toml::Value::Float(_) => value
                .parse::<f64>()
                .map(toml::Value::Float)
                .unwrap_or_else(|_| toml::Value::String(value.to_string())),
            toml::Value::Boolean(_) => value
                .parse::<bool>()
                .map(toml::Value::Boolean)
                .unwrap_or_else(|_| toml::Value::String(value.to_string())),
            _ => toml::Value::String(value.to_string()),
        }
    } else {
        toml::Value::String(value.to_string())
    };

    tbl.insert(last_key.to_string(), new_value);

    // Write back (note: this strips comments — warned in help text)
    let serialized = toml::to_string_pretty(&table).unwrap_or_else(|e| {
        ui::error(&format!("Failed to serialize config: {e}"));
        std::process::exit(1);
    });

    std::fs::write(&config_path, &serialized).unwrap_or_else(|e| {
        ui::error(&format!("Failed to write config: {e}"));
        std::process::exit(1);
    });
    restrict_file_permissions(&config_path);

    ui::success(&format!("Set {key} = {value}"));
}

fn cmd_config_unset(key: &str) {
    let home = openfang_home();
    let config_path = home.join("config.toml");

    if !config_path.exists() {
        ui::error_with_fix("No config file found", "Run `openfang init` first");
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
        ui::error(&format!("Failed to read config: {e}"));
        std::process::exit(1);
    });

    let mut table: toml::Value = toml::from_str(&content).unwrap_or_else(|e| {
        ui::error_with_fix(
            &format!("Config parse error: {e}"),
            "Fix your config.toml syntax first",
        );
        std::process::exit(1);
    });

    // Navigate to parent table and remove the final key
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        ui::error("Empty key");
        std::process::exit(1);
    }

    let mut current = &mut table;
    for part in &parts[..parts.len() - 1] {
        current = current
            .as_table_mut()
            .and_then(|t| t.get_mut(*part))
            .unwrap_or_else(|| {
                ui::error(&format!("Key path not found: {key}"));
                std::process::exit(1);
            });
    }

    let last_key = parts[parts.len() - 1];
    let tbl = current.as_table_mut().unwrap_or_else(|| {
        ui::error(&format!("Parent of '{key}' is not a table"));
        std::process::exit(1);
    });

    if tbl.remove(last_key).is_none() {
        ui::error(&format!("Key not found: {key}"));
        std::process::exit(1);
    }

    // Write back (note: this strips comments — warned in help text)
    let serialized = toml::to_string_pretty(&table).unwrap_or_else(|e| {
        ui::error(&format!("Failed to serialize config: {e}"));
        std::process::exit(1);
    });

    std::fs::write(&config_path, &serialized).unwrap_or_else(|e| {
        ui::error(&format!("Failed to write config: {e}"));
        std::process::exit(1);
    });
    restrict_file_permissions(&config_path);

    ui::success(&format!("Removed key: {key}"));
}

fn cmd_config_set_key(provider: &str) {
    let env_var = provider_to_env_var(provider);

    let key = prompt_input(&format!("  Paste your {provider} API key: "));
    if key.is_empty() {
        ui::error("No key provided. Cancelled.");
        return;
    }

    match dotenv::save_env_key(&env_var, &key) {
        Ok(()) => {
            ui::success(&format!("Saved {env_var} to ~/.openfang/.env"));
            // Test the key
            print!("  Testing key... ");
            io::stdout().flush().unwrap();
            if test_api_key(provider, &env_var) {
                println!("{}", "OK".bright_green());
            } else {
                println!("{}", "could not verify (may still work)".bright_yellow());
            }
        }
        Err(e) => {
            ui::error(&format!("Failed to save key: {e}"));
            std::process::exit(1);
        }
    }
}

fn cmd_config_delete_key(provider: &str) {
    let env_var = provider_to_env_var(provider);

    match dotenv::remove_env_key(&env_var) {
        Ok(()) => ui::success(&format!("Removed {env_var} from ~/.openfang/.env")),
        Err(e) => {
            ui::error(&format!("Failed to remove key: {e}"));
            std::process::exit(1);
        }
    }
}

fn cmd_config_test_key(provider: &str) {
    let env_var = provider_to_env_var(provider);

    if std::env::var(&env_var).is_err() {
        ui::error(&format!("{env_var} not set"));
        ui::hint(&format!("Set it: openfang config set-key {provider}"));
        std::process::exit(1);
    }

    print!("  Testing {provider} ({env_var})... ");
    io::stdout().flush().unwrap();
    if test_api_key(provider, &env_var) {
        println!("{}", "OK".bright_green());
    } else {
        println!("{}", "FAILED (401/403)".bright_red());
        ui::hint(&format!("Update key: openfang config set-key {provider}"));
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Quick chat (OpenClaw alias)
// ---------------------------------------------------------------------------

fn cmd_quick_chat(config: Option<PathBuf>, agent: Option<String>) {
    tui::chat_runner::run_chat_tui(config, agent);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn openfang_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| {
            eprintln!("Error: Could not determine home directory");
            std::process::exit(1);
        })
        .join(".openfang")
}

fn prompt_input(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().unwrap();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).unwrap_or(0);
    line.trim().to_string()
}

pub(crate) fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) {
    std::fs::create_dir_all(dst).unwrap();
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dest_path = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir_recursive(&path, &dest_path);
            } else {
                let _ = std::fs::copy(&path, &dest_path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Integration commands (openfang add/remove/integrations)
// ---------------------------------------------------------------------------

fn cmd_integration_add(name: &str, key: Option<&str>) {
    let home = openfang_home();
    let mut registry = openfang_extensions::registry::IntegrationRegistry::new(&home);
    registry.load_bundled();
    let _ = registry.load_installed();

    // Check template exists
    let template = match registry.get_template(name) {
        Some(t) => t.clone(),
        None => {
            ui::error(&format!("Unknown integration: '{name}'"));
            println!("\nAvailable integrations:");
            for t in registry.list_templates() {
                println!("  {} {} — {}", t.icon, t.id, t.description);
            }
            std::process::exit(1);
        }
    };

    // Set up credential resolver
    let dotenv_path = home.join(".env");
    let vault_path = home.join("vault.enc");
    let vault = if vault_path.exists() {
        let mut v = openfang_extensions::vault::CredentialVault::new(vault_path);
        if v.unlock().is_ok() {
            Some(v)
        } else {
            None
        }
    } else {
        None
    };
    let mut resolver =
        openfang_extensions::credentials::CredentialResolver::new(vault, Some(&dotenv_path))
            .with_interactive(true);

    // Build provided keys map
    let mut provided_keys = std::collections::HashMap::new();
    if let Some(key_value) = key {
        // Auto-detect which env var to use (first required_env that's a secret)
        if let Some(env_var) = template.required_env.iter().find(|e| e.is_secret) {
            provided_keys.insert(env_var.name.clone(), key_value.to_string());
        }
    }

    match openfang_extensions::installer::install_integration(
        &mut registry,
        &mut resolver,
        name,
        &provided_keys,
    ) {
        Ok(result) => {
            match &result.status {
                openfang_extensions::IntegrationStatus::Ready => {
                    ui::success(&result.message);
                }
                openfang_extensions::IntegrationStatus::Setup => {
                    println!("{}", result.message.yellow());
                    println!("\nTo add credentials:");
                    for env in &template.required_env {
                        if env.is_secret {
                            println!("  openfang vault set {}  # {}", env.name, env.help);
                            if let Some(ref url) = env.get_url {
                                println!("  Get it here: {url}");
                            }
                        }
                    }
                }
                _ => println!("{}", result.message),
            }

            // If daemon is running, trigger hot-reload
            if let Some(base_url) = find_daemon() {
                let client = daemon_client();
                let _ = client
                    .post(format!("{base_url}/api/integrations/reload"))
                    .send();
            }
        }
        Err(e) => {
            ui::error(&e.to_string());
            std::process::exit(1);
        }
    }
}

fn cmd_integration_remove(name: &str) {
    let home = openfang_home();
    let mut registry = openfang_extensions::registry::IntegrationRegistry::new(&home);
    registry.load_bundled();
    let _ = registry.load_installed();

    match openfang_extensions::installer::remove_integration(&mut registry, name) {
        Ok(msg) => {
            ui::success(&msg);
            // Hot-reload daemon
            if let Some(base_url) = find_daemon() {
                let client = daemon_client();
                let _ = client
                    .post(format!("{base_url}/api/integrations/reload"))
                    .send();
            }
        }
        Err(e) => {
            ui::error(&e.to_string());
            std::process::exit(1);
        }
    }
}

fn cmd_integrations_list(query: Option<&str>) {
    let home = openfang_home();
    let mut registry = openfang_extensions::registry::IntegrationRegistry::new(&home);
    registry.load_bundled();
    let _ = registry.load_installed();

    let dotenv_path = home.join(".env");
    let resolver =
        openfang_extensions::credentials::CredentialResolver::new(None, Some(&dotenv_path));

    let entries = if let Some(q) = query {
        openfang_extensions::installer::search_integrations(&registry, q)
    } else {
        openfang_extensions::installer::list_integrations(&registry, &resolver)
    };

    if entries.is_empty() {
        if let Some(q) = query {
            println!("No integrations matching '{q}'.");
        } else {
            println!("No integrations available.");
        }
        return;
    }

    // Group by category
    let mut by_category: std::collections::BTreeMap<
        String,
        Vec<&openfang_extensions::installer::IntegrationListEntry>,
    > = std::collections::BTreeMap::new();
    for entry in &entries {
        by_category
            .entry(entry.category.clone())
            .or_default()
            .push(entry);
    }

    for (category, items) in &by_category {
        println!("\n{}", format!("  {category}").bold());
        for item in items {
            let status_badge = match &item.status {
                openfang_extensions::IntegrationStatus::Ready => "[Ready]".green().to_string(),
                openfang_extensions::IntegrationStatus::Setup => "[Setup]".yellow().to_string(),
                openfang_extensions::IntegrationStatus::Available => {
                    "[Available]".dimmed().to_string()
                }
                openfang_extensions::IntegrationStatus::Error(msg) => {
                    format!("[Error: {msg}]").red().to_string()
                }
                openfang_extensions::IntegrationStatus::Disabled => {
                    "[Disabled]".dimmed().to_string()
                }
            };
            println!(
                "    {} {:<20} {:<12} {}",
                item.icon, item.id, status_badge, item.description
            );
        }
    }
    println!();
    println!(
        "  {} integrations ({} installed)",
        entries.len(),
        entries
            .iter()
            .filter(|e| matches!(
                e.status,
                openfang_extensions::IntegrationStatus::Ready
                    | openfang_extensions::IntegrationStatus::Setup
            ))
            .count()
    );
    println!("  Use `openfang add <name>` to install an integration.");
}

// ---------------------------------------------------------------------------
// Vault commands (openfang vault init/set/list/remove)
// ---------------------------------------------------------------------------

fn cmd_vault_init() {
    let home = openfang_home();
    let vault_path = home.join("vault.enc");
    let mut vault = openfang_extensions::vault::CredentialVault::new(vault_path);

    match vault.init() {
        Ok(()) => ui::success("Credential vault initialized."),
        Err(e) => {
            ui::error(&e.to_string());
            std::process::exit(1);
        }
    }
}

fn cmd_vault_set(key: &str) {
    use zeroize::Zeroizing;

    let home = openfang_home();
    let vault_path = home.join("vault.enc");
    let mut vault = openfang_extensions::vault::CredentialVault::new(vault_path);

    if !vault.exists() {
        ui::error("Vault not initialized. Run: openfang vault init");
        std::process::exit(1);
    }

    if let Err(e) = vault.unlock() {
        ui::error(&format!("Could not unlock vault: {e}"));
        std::process::exit(1);
    }

    let value = prompt_input(&format!("Enter value for {key}: "));
    if value.is_empty() {
        ui::error("Empty value — not stored.");
        std::process::exit(1);
    }

    match vault.set(key.to_string(), Zeroizing::new(value)) {
        Ok(()) => ui::success(&format!("Stored '{key}' in vault.")),
        Err(e) => {
            ui::error(&format!("Failed to store: {e}"));
            std::process::exit(1);
        }
    }
}

fn cmd_vault_list() {
    let home = openfang_home();
    let vault_path = home.join("vault.enc");
    let mut vault = openfang_extensions::vault::CredentialVault::new(vault_path);

    if !vault.exists() {
        println!("Vault not initialized. Run: openfang vault init");
        return;
    }

    if let Err(e) = vault.unlock() {
        ui::error(&format!("Could not unlock vault: {e}"));
        std::process::exit(1);
    }

    let keys = vault.list_keys();
    if keys.is_empty() {
        println!("Vault is empty.");
    } else {
        println!("Stored credentials ({}):", keys.len());
        for key in keys {
            println!("  {key}");
        }
    }
}

fn cmd_vault_remove(key: &str) {
    let home = openfang_home();
    let vault_path = home.join("vault.enc");
    let mut vault = openfang_extensions::vault::CredentialVault::new(vault_path);

    if !vault.exists() {
        ui::error("Vault not initialized.");
        std::process::exit(1);
    }
    if let Err(e) = vault.unlock() {
        ui::error(&format!("Could not unlock vault: {e}"));
        std::process::exit(1);
    }

    match vault.remove(key) {
        Ok(true) => ui::success(&format!("Removed '{key}' from vault.")),
        Ok(false) => println!("Key '{key}' not found in vault."),
        Err(e) => {
            ui::error(&format!("Failed to remove: {e}"));
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Scaffold commands (openfang new skill/integration)
// ---------------------------------------------------------------------------

fn cmd_scaffold(kind: ScaffoldKind) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let result = match kind {
        ScaffoldKind::Skill => {
            openfang_extensions::installer::scaffold_skill(&cwd.join("my-skill"))
        }
        ScaffoldKind::Integration => {
            openfang_extensions::installer::scaffold_integration(&cwd.join("my-integration"))
        }
    };
    match result {
        Ok(msg) => ui::success(&msg),
        Err(e) => {
            ui::error(&e.to_string());
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// New command handlers
// ---------------------------------------------------------------------------

fn cmd_models_list(provider_filter: Option<&str>, json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let url = match provider_filter {
            Some(p) => format!("{base}/api/models?provider={p}"),
            None => format!("{base}/api/models"),
        };
        let body = daemon_json(client.get(&url).send());
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            return;
        }
        if let Some(arr) = body.as_array() {
            if arr.is_empty() {
                println!("No models found.");
                return;
            }
            println!("{:<40} {:<16} {:<8} CONTEXT", "MODEL", "PROVIDER", "TIER");
            println!("{}", "-".repeat(80));
            for m in arr {
                println!(
                    "{:<40} {:<16} {:<8} {}",
                    m["id"].as_str().unwrap_or("?"),
                    m["provider"].as_str().unwrap_or("?"),
                    m["tier"].as_str().unwrap_or("?"),
                    m["context_window"].as_u64().unwrap_or(0),
                );
            }
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
        }
    } else {
        // Standalone: use ModelCatalog directly
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        let models = catalog.list_models();
        if json {
            let arr: Vec<serde_json::Value> = models
                .iter()
                .filter(|m| provider_filter.is_none_or(|p| m.provider == p))
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "provider": m.provider,
                        "tier": format!("{:?}", m.tier),
                        "context_window": m.context_window,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&arr).unwrap_or_default());
            return;
        }
        if models.is_empty() {
            println!("No models in catalog.");
            return;
        }
        println!("{:<40} {:<16} {:<8} CONTEXT", "MODEL", "PROVIDER", "TIER");
        println!("{}", "-".repeat(80));
        for m in models {
            if let Some(p) = provider_filter {
                if m.provider != p {
                    continue;
                }
            }
            println!(
                "{:<40} {:<16} {:<8} {}",
                m.id,
                m.provider,
                format!("{:?}", m.tier),
                m.context_window,
            );
        }
    }
}

fn cmd_models_aliases(json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(client.get(format!("{base}/api/models/aliases")).send());
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            return;
        }
        if let Some(obj) = body.as_object() {
            println!("{:<30} RESOLVES TO", "ALIAS");
            println!("{}", "-".repeat(60));
            for (alias, target) in obj {
                println!("{:<30} {}", alias, target.as_str().unwrap_or("?"));
            }
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
        }
    } else {
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        let aliases = catalog.list_aliases();
        if json {
            let obj: serde_json::Map<String, serde_json::Value> = aliases
                .iter()
                .map(|(a, t)| (a.to_string(), serde_json::Value::String(t.to_string())))
                .collect();
            println!("{}", serde_json::to_string_pretty(&obj).unwrap_or_default());
            return;
        }
        println!("{:<30} RESOLVES TO", "ALIAS");
        println!("{}", "-".repeat(60));
        for (alias, target) in aliases {
            println!("{:<30} {}", alias, target);
        }
    }
}

fn cmd_models_providers(json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(client.get(format!("{base}/api/providers")).send());
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            return;
        }
        if let Some(arr) = body.as_array() {
            println!(
                "{:<20} {:<12} {:<10} BASE URL",
                "PROVIDER", "AUTH", "MODELS"
            );
            println!("{}", "-".repeat(70));
            for p in arr {
                println!(
                    "{:<20} {:<12} {:<10} {}",
                    p["id"].as_str().unwrap_or("?"),
                    p["auth_status"].as_str().unwrap_or("?"),
                    p["model_count"].as_u64().unwrap_or(0),
                    p["base_url"].as_str().unwrap_or(""),
                );
            }
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
        }
    } else {
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        let providers = catalog.list_providers();
        if json {
            let arr: Vec<serde_json::Value> = providers
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "auth_status": format!("{:?}", p.auth_status),
                        "model_count": p.model_count,
                        "base_url": p.base_url,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&arr).unwrap_or_default());
            return;
        }
        println!(
            "{:<20} {:<12} {:<10} BASE URL",
            "PROVIDER", "AUTH", "MODELS"
        );
        println!("{}", "-".repeat(70));
        for p in providers {
            println!(
                "{:<20} {:<12} {:<10} {}",
                p.id,
                format!("{:?}", p.auth_status),
                p.model_count,
                p.base_url,
            );
        }
    }
}

fn cmd_models_set(model: Option<String>) {
    let model = match model {
        Some(m) => m,
        None => pick_model(),
    };
    let base = require_daemon("models set");
    let client = daemon_client();
    // Use the config set approach through the API
    let body = daemon_json(
        client
            .post(format!("{base}/api/config/set"))
            .json(&serde_json::json!({"key": "default_model.model", "value": model}))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed to set model: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Default model set to: {model}"));
    }
}

/// Interactive model picker — shows numbered list, accepts number or model ID.
fn pick_model() -> String {
    let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
    let models = catalog.list_models();

    if models.is_empty() {
        ui::error("No models in catalog.");
        std::process::exit(1);
    }

    // Group by provider for display
    let mut by_provider: std::collections::BTreeMap<
        String,
        Vec<&openfang_types::model_catalog::ModelCatalogEntry>,
    > = std::collections::BTreeMap::new();
    for m in models {
        by_provider.entry(m.provider.clone()).or_default().push(m);
    }

    ui::section("Select a model");
    ui::blank();

    let mut numbered: Vec<&str> = Vec::new();
    let mut idx = 1;
    for (provider, provider_models) in &by_provider {
        println!("  {}:", provider.bold());
        for m in provider_models {
            println!("    {idx:>3}. {:<36} {:?}", m.id, m.tier);
            numbered.push(&m.id);
            idx += 1;
        }
    }
    ui::blank();

    loop {
        let input = prompt_input("  Enter number or model ID: ");
        if input.is_empty() {
            continue;
        }
        // Try as number first
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= numbered.len() {
                return numbered[n - 1].to_string();
            }
            ui::error(&format!("Number out of range (1-{})", numbered.len()));
            continue;
        }
        // Accept direct model ID if it exists in catalog
        if models.iter().any(|m| m.id == input) {
            return input;
        }
        // Accept as alias
        if catalog.resolve_alias(&input).is_some() {
            return input;
        }
        // Accept any string (user might know a model not in catalog)
        return input;
    }
}

fn cmd_approvals_list(json: bool) {
    let base = require_daemon("approvals list");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/approvals")).send());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No pending approvals.");
            return;
        }
        println!("{:<38} {:<16} {:<12} REQUEST", "ID", "AGENT", "TYPE");
        println!("{}", "-".repeat(80));
        for a in arr {
            println!(
                "{:<38} {:<16} {:<12} {}",
                a["id"].as_str().unwrap_or("?"),
                a["agent_name"].as_str().unwrap_or("?"),
                a["approval_type"].as_str().unwrap_or("?"),
                a["description"].as_str().unwrap_or(""),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_approvals_respond(id: &str, approve: bool) {
    let base = require_daemon("approvals");
    let client = daemon_client();
    let endpoint = if approve { "approve" } else { "reject" };
    let body = daemon_json(
        client
            .post(format!("{base}/api/approvals/{id}/{endpoint}"))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Approval {id} {endpoint}d."));
    }
}

fn cmd_cron_list(json: bool) {
    let base = require_daemon("cron list");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/cron/jobs")).send());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No scheduled jobs.");
            return;
        }
        println!(
            "{:<38} {:<16} {:<20} {:<8} PROMPT",
            "ID", "AGENT", "SCHEDULE", "ENABLED"
        );
        println!("{}", "-".repeat(100));
        for j in arr {
            println!(
                "{:<38} {:<16} {:<20} {:<8} {}",
                j["id"].as_str().unwrap_or("?"),
                j["agent_id"].as_str().unwrap_or("?"),
                j["cron_expr"].as_str().unwrap_or("?"),
                if j["enabled"].as_bool().unwrap_or(false) {
                    "yes"
                } else {
                    "no"
                },
                j["prompt"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(40)
                    .collect::<String>(),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_cron_create(agent: &str, spec: &str, prompt: &str) {
    let base = require_daemon("cron create");
    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/cron/jobs"))
            .json(&serde_json::json!({
                "agent_id": agent,
                "cron_expr": spec,
                "prompt": prompt,
            }))
            .send(),
    );
    if let Some(id) = body["id"].as_str() {
        ui::success(&format!("Cron job created: {id}"));
    } else {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    }
}

fn cmd_cron_delete(id: &str) {
    let base = require_daemon("cron delete");
    let client = daemon_client();
    let body = daemon_json(client.delete(format!("{base}/api/cron/jobs/{id}")).send());
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Cron job {id} deleted."));
    }
}

fn cmd_cron_toggle(id: &str, enable: bool) {
    let base = require_daemon("cron");
    let client = daemon_client();
    let endpoint = if enable { "enable" } else { "disable" };
    let body = daemon_json(
        client
            .post(format!("{base}/api/cron/jobs/{id}/{endpoint}"))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Cron job {id} {endpoint}d."));
    }
}

fn cmd_sessions(agent: Option<&str>, json: bool) {
    let base = require_daemon("sessions");
    let client = daemon_client();
    let url = match agent {
        Some(a) => format!("{base}/api/sessions?agent={a}"),
        None => format!("{base}/api/sessions"),
    };
    let body = daemon_json(client.get(&url).send());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No sessions found.");
            return;
        }
        println!("{:<38} {:<16} {:<8} LAST ACTIVE", "ID", "AGENT", "MSGS");
        println!("{}", "-".repeat(80));
        for s in arr {
            println!(
                "{:<38} {:<16} {:<8} {}",
                s["id"].as_str().unwrap_or("?"),
                s["agent_name"].as_str().unwrap_or("?"),
                s["message_count"].as_u64().unwrap_or(0),
                s["last_active"].as_str().unwrap_or("?"),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_logs(lines: usize, follow: bool) {
    let log_path = dirs::home_dir()
        .map(|h| h.join(".openfang").join("tui.log"))
        .unwrap_or_else(|| PathBuf::from("tui.log"));

    if !log_path.exists() {
        ui::error_with_fix(
            "Log file not found",
            &format!("Expected at: {}", log_path.display()),
        );
        std::process::exit(1);
    }

    if follow {
        // Use tail -f equivalent
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("tail")
                .args(["-f", "-n", &lines.to_string()])
                .arg(&log_path)
                .status();
        }
        #[cfg(windows)]
        {
            // On Windows, read in a loop
            let content = std::fs::read_to_string(&log_path).unwrap_or_default();
            let all_lines: Vec<&str> = content.lines().collect();
            let start = all_lines.len().saturating_sub(lines);
            for line in &all_lines[start..] {
                println!("{line}");
            }
            println!("--- Following {} (Ctrl+C to stop) ---", log_path.display());
            let mut last_len = content.len();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if let Ok(new_content) = std::fs::read_to_string(&log_path) {
                    if new_content.len() > last_len {
                        print!("{}", &new_content[last_len..]);
                        last_len = new_content.len();
                    }
                }
            }
        }
    } else {
        let content = std::fs::read_to_string(&log_path).unwrap_or_default();
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        for line in &all_lines[start..] {
            println!("{line}");
        }
    }
}

fn cmd_health(json: bool) {
    match find_daemon() {
        Some(base) => {
            let client = daemon_client();
            let body = daemon_json(client.get(format!("{base}/api/health")).send());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
                return;
            }
            ui::success("Daemon is healthy");
            if let Some(status) = body["status"].as_str() {
                ui::kv("Status", status);
            }
            if let Some(uptime) = body.get("uptime_secs").and_then(|v| v.as_u64()) {
                let hours = uptime / 3600;
                let mins = (uptime % 3600) / 60;
                ui::kv("Uptime", &format!("{hours}h {mins}m"));
            }
        }
        None => {
            if json {
                println!("{}", serde_json::json!({"error": "daemon not running"}));
                std::process::exit(1);
            }
            ui::error("Daemon is not running.");
            ui::hint("Start it with: openfang start");
            std::process::exit(1);
        }
    }
}

fn cmd_security_status(json: bool) {
    let base = require_daemon("security status");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/health/detail")).send());
    if json {
        let data = serde_json::json!({
            "audit_trail": "merkle_hash_chain_sha256",
            "taint_tracking": "information_flow_labels",
            "wasm_sandbox": "dual_metering_fuel_epoch",
            "wire_protocol": "ofp_hmac_sha256_mutual_auth",
            "api_keys": "zeroizing_auto_wipe",
            "manifests": "ed25519_signed",
            "agent_count": body.get("agent_count").and_then(|v| v.as_u64()),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_default()
        );
        return;
    }
    ui::section("Security Status");
    ui::blank();
    ui::kv("Audit trail", "Merkle hash chain (SHA-256)");
    ui::kv("Taint tracking", "Information flow labels");
    ui::kv("WASM sandbox", "Dual metering (fuel + epoch)");
    ui::kv("Wire protocol", "OFP HMAC-SHA256 mutual auth");
    ui::kv("API keys", "Zeroizing<String> (auto-wipe on drop)");
    ui::kv("Manifests", "Ed25519 signed");
    if let Some(agents) = body.get("agent_count").and_then(|v| v.as_u64()) {
        ui::kv("Active agents", &agents.to_string());
    }
}

fn cmd_security_audit(limit: usize, json: bool) {
    let base = require_daemon("security audit");
    let client = daemon_client();
    let body = daemon_json(
        client
            .get(format!("{base}/api/audit/recent?limit={limit}"))
            .send(),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No audit entries.");
            return;
        }
        println!("{:<24} {:<16} {:<12} EVENT", "TIMESTAMP", "AGENT", "TYPE");
        println!("{}", "-".repeat(80));
        for entry in arr {
            println!(
                "{:<24} {:<16} {:<12} {}",
                entry["timestamp"].as_str().unwrap_or("?"),
                entry["agent_name"].as_str().unwrap_or("?"),
                entry["event_type"].as_str().unwrap_or("?"),
                entry["description"].as_str().unwrap_or(""),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_security_verify() {
    let base = require_daemon("security verify");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/audit/verify")).send());
    if body["valid"].as_bool().unwrap_or(false) {
        ui::success("Audit trail integrity verified (Merkle chain valid).");
    } else {
        ui::error("Audit trail integrity check FAILED.");
        if let Some(msg) = body["error"].as_str() {
            ui::hint(msg);
        }
        std::process::exit(1);
    }
}

fn cmd_memory_list(agent: &str, json: bool) {
    let base = require_daemon("memory list");
    let client = daemon_client();
    let body = daemon_json(
        client
            .get(format!("{base}/api/memory/agents/{agent}/kv"))
            .send(),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No memory entries for agent '{agent}'.");
            return;
        }
        println!("{:<30} VALUE", "KEY");
        println!("{}", "-".repeat(60));
        for kv in arr {
            println!(
                "{:<30} {}",
                kv["key"].as_str().unwrap_or("?"),
                kv["value"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(50)
                    .collect::<String>(),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_memory_get(agent: &str, key: &str, json: bool) {
    let base = require_daemon("memory get");
    let client = daemon_client();
    let body = daemon_json(
        client
            .get(format!("{base}/api/memory/agents/{agent}/kv/{key}"))
            .send(),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(val) = body["value"].as_str() {
        println!("{val}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_memory_set(agent: &str, key: &str, value: &str) {
    let base = require_daemon("memory set");
    let client = daemon_client();
    let body = daemon_json(
        client
            .put(format!("{base}/api/memory/agents/{agent}/kv/{key}"))
            .json(&serde_json::json!({"value": value}))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Set {key} for agent '{agent}'."));
    }
}

fn cmd_memory_delete(agent: &str, key: &str) {
    let base = require_daemon("memory delete");
    let client = daemon_client();
    let body = daemon_json(
        client
            .delete(format!("{base}/api/memory/agents/{agent}/kv/{key}"))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Deleted key '{key}' for agent '{agent}'."));
    }
}

fn cmd_devices_list(json: bool) {
    let base = require_daemon("devices list");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/pairing/devices")).send());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No paired devices.");
            return;
        }
        println!("{:<38} {:<20} LAST SEEN", "ID", "NAME");
        println!("{}", "-".repeat(70));
        for d in arr {
            println!(
                "{:<38} {:<20} {}",
                d["id"].as_str().unwrap_or("?"),
                d["name"].as_str().unwrap_or("?"),
                d["last_seen"].as_str().unwrap_or("?"),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_devices_pair() {
    let base = require_daemon("qr");
    let client = daemon_client();
    let body = daemon_json(client.post(format!("{base}/api/pairing/request")).send());
    if let Some(qr) = body["qr_data"].as_str() {
        ui::section("Device Pairing");
        ui::blank();
        // Render a simple text-based QR representation
        println!("  Scan this QR code with the OpenFang mobile app:");
        ui::blank();
        println!("  {qr}");
        ui::blank();
        if let Some(code) = body["pairing_code"].as_str() {
            ui::kv("Pairing code", code);
        }
        if let Some(expires) = body["expires_at"].as_str() {
            ui::kv("Expires", expires);
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_devices_remove(id: &str) {
    let base = require_daemon("devices remove");
    let client = daemon_client();
    let body = daemon_json(
        client
            .delete(format!("{base}/api/pairing/devices/{id}"))
            .send(),
    );
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Device {id} removed."));
    }
}

fn cmd_webhooks_list(json: bool) {
    let base = require_daemon("webhooks list");
    let client = daemon_client();
    let body = daemon_json(client.get(format!("{base}/api/triggers")).send());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    if let Some(arr) = body.as_array() {
        if arr.is_empty() {
            println!("No webhooks configured.");
            return;
        }
        println!("{:<38} {:<16} URL", "ID", "AGENT");
        println!("{}", "-".repeat(80));
        for w in arr {
            println!(
                "{:<38} {:<16} {}",
                w["id"].as_str().unwrap_or("?"),
                w["agent_id"].as_str().unwrap_or("?"),
                w["url"].as_str().unwrap_or(""),
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_webhooks_create(agent: &str, url: &str) {
    let base = require_daemon("webhooks create");
    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/triggers"))
            .json(&serde_json::json!({
                "agent_id": agent,
                "pattern": {"webhook": {"url": url}},
                "prompt_template": "Webhook event: {{event}}",
            }))
            .send(),
    );
    if let Some(id) = body["id"].as_str() {
        ui::success(&format!("Webhook created: {id}"));
    } else {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    }
}

fn cmd_webhooks_delete(id: &str) {
    let base = require_daemon("webhooks delete");
    let client = daemon_client();
    let body = daemon_json(client.delete(format!("{base}/api/triggers/{id}")).send());
    if body.get("error").is_some() {
        ui::error(&format!(
            "Failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    } else {
        ui::success(&format!("Webhook {id} deleted."));
    }
}

fn cmd_webhooks_test(id: &str) {
    let base = require_daemon("webhooks test");
    let client = daemon_client();
    let body = daemon_json(client.post(format!("{base}/api/triggers/{id}/test")).send());
    if body["success"].as_bool().unwrap_or(false) {
        ui::success(&format!("Webhook {id} test payload sent successfully."));
    } else {
        ui::error(&format!(
            "Webhook test failed: {}",
            body["error"].as_str().unwrap_or("?")
        ));
    }
}

fn cmd_message(agent: &str, text: &str, json: bool) {
    let base = require_daemon("message");
    let client = daemon_client();
    let body = daemon_json(
        client
            .post(format!("{base}/api/agents/{agent}/message"))
            .json(&serde_json::json!({"message": text}))
            .send(),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else if let Some(reply) = body["reply"].as_str() {
        println!("{reply}");
    } else if let Some(reply) = body["response"].as_str() {
        println!("{reply}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
}

fn cmd_system_info(json: bool) {
    if let Some(base) = find_daemon() {
        let client = daemon_client();
        let body = daemon_json(client.get(format!("{base}/api/status")).send());
        if json {
            let mut data = body.clone();
            if let Some(obj) = data.as_object_mut() {
                obj.insert(
                    "version".to_string(),
                    serde_json::json!(env!("CARGO_PKG_VERSION")),
                );
                obj.insert("api_url".to_string(), serde_json::json!(base));
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&data).unwrap_or_default()
            );
            return;
        }
        ui::section("OpenFang System Info");
        ui::blank();
        ui::kv("Version", env!("CARGO_PKG_VERSION"));
        ui::kv("Status", body["status"].as_str().unwrap_or("?"));
        ui::kv(
            "Agents",
            &body["agent_count"].as_u64().unwrap_or(0).to_string(),
        );
        ui::kv("Provider", body["default_provider"].as_str().unwrap_or("?"));
        ui::kv("Model", body["default_model"].as_str().unwrap_or("?"));
        ui::kv("API", &base);
        ui::kv("Data dir", body["data_dir"].as_str().unwrap_or("?"));
        ui::kv(
            "Uptime",
            &format!("{}s", body["uptime_seconds"].as_u64().unwrap_or(0)),
        );
    } else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "daemon": "not_running",
                })
            );
            return;
        }
        ui::section("OpenFang System Info");
        ui::blank();
        ui::kv("Version", env!("CARGO_PKG_VERSION"));
        ui::kv_warn("Daemon", "NOT RUNNING");
        ui::hint("Start with: openfang start");
    }
}

fn cmd_system_version(json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION")})
        );
        return;
    }
    println!("openfang {}", env!("CARGO_PKG_VERSION"));
}

fn cmd_reset(confirm: bool) {
    let openfang_dir = match dirs::home_dir() {
        Some(h) => h.join(".openfang"),
        None => {
            ui::error("Could not determine home directory");
            std::process::exit(1);
        }
    };

    if !openfang_dir.exists() {
        println!(
            "Nothing to reset — {} does not exist.",
            openfang_dir.display()
        );
        return;
    }

    if !confirm {
        println!("  This will delete all data in {}", openfang_dir.display());
        println!("  Including: config, database, agent manifests, credentials.");
        println!();
        let answer = prompt_input("  Are you sure? Type 'yes' to confirm: ");
        if answer.trim() != "yes" {
            println!("  Cancelled.");
            return;
        }
    }

    match std::fs::remove_dir_all(&openfang_dir) {
        Ok(()) => ui::success(&format!("Removed {}", openfang_dir.display())),
        Err(e) => {
            ui::error(&format!("Failed to remove {}: {e}", openfang_dir.display()));
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {

    // --- Doctor command unit tests ---

    #[test]
    fn test_doctor_skill_registry_loads_bundled() {
        let skills_dir = std::env::temp_dir().join("openfang-doctor-test-skills");
        let mut skill_reg = openfang_skills::registry::SkillRegistry::new(skills_dir);
        let count = skill_reg.load_bundled();
        assert!(count > 0, "Should load bundled skills");
        assert_eq!(skill_reg.count(), count);
    }

    #[test]
    fn test_doctor_extension_registry_loads_bundled() {
        let tmp = std::env::temp_dir().join("openfang-doctor-test-ext");
        let _ = std::fs::create_dir_all(&tmp);
        let mut ext_reg = openfang_extensions::registry::IntegrationRegistry::new(&tmp);
        let count = ext_reg.load_bundled();
        assert!(count > 0, "Should load bundled integration templates");
        assert_eq!(ext_reg.template_count(), count);
    }

    #[test]
    fn test_doctor_config_deser_default() {
        // Default KernelConfig should serialize/deserialize round-trip
        let config = openfang_types::config::KernelConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: openfang_types::config::KernelConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.api_listen, config.api_listen);
    }

    #[test]
    fn test_doctor_config_include_field() {
        let config_toml = r#"
api_listen = "127.0.0.1:4200"
include = ["providers.toml", "agents.toml"]

[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"
"#;
        let config: openfang_types::config::KernelConfig = toml::from_str(config_toml).unwrap();
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.include[0], "providers.toml");
        assert_eq!(config.include[1], "agents.toml");
    }

    #[test]
    fn test_doctor_exec_policy_field() {
        let config_toml = r#"
api_listen = "127.0.0.1:4200"

[exec_policy]
mode = "allowlist"
safe_bins = ["ls", "cat", "echo"]
timeout_secs = 30

[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"
"#;
        let config: openfang_types::config::KernelConfig = toml::from_str(config_toml).unwrap();
        assert_eq!(
            config.exec_policy.mode,
            openfang_types::config::ExecSecurityMode::Allowlist
        );
        assert_eq!(config.exec_policy.safe_bins.len(), 3);
        assert_eq!(config.exec_policy.timeout_secs, 30);
    }

    #[test]
    fn test_doctor_mcp_transport_validation() {
        let config_toml = r#"
api_listen = "127.0.0.1:4200"

[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"

[[mcp_servers]]
name = "github"
timeout_secs = 30

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
"#;
        let config: openfang_types::config::KernelConfig = toml::from_str(config_toml).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers[0].name, "github");
        match &config.mcp_servers[0].transport {
            openfang_types::config::McpTransportEntry::Stdio { command, args } => {
                assert_eq!(command, "npx");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_doctor_skill_injection_scan_clean() {
        let clean_content = "This is a normal skill prompt with helpful instructions.";
        let warnings = openfang_skills::verify::SkillVerifier::scan_prompt_content(clean_content);
        assert!(warnings.is_empty(), "Clean content should have no warnings");
    }

    #[test]
    fn test_doctor_hook_event_variants() {
        // Verify all 4 hook event types are constructable
        use openfang_types::agent::HookEvent;
        let events = [
            HookEvent::BeforeToolCall,
            HookEvent::AfterToolCall,
            HookEvent::BeforePromptBuild,
            HookEvent::AgentLoopEnd,
        ];
        assert_eq!(events.len(), 4);
    }
}
