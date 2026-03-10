# OpenFang CLI - English Language Pack
# This file contains all user-facing strings for the OpenFang CLI.

# ─────────────────────────────────────────────────────────────────────────────
# Application Info
# ─────────────────────────────────────────────────────────────────────────────
app-name = OpenFang Agent OS
app-tagline = The open-source agent operating system

# ─────────────────────────────────────────────────────────────────────────────
# Daemon Commands
# ─────────────────────────────────────────────────────────────────────────────
daemon-starting = Starting daemon...
daemon-stopped = OpenFang daemon stopped.
daemon-already-running = Daemon is already running.
daemon-not-running = No daemon is currently running.
daemon-stopping = Stopping daemon...

# ─────────────────────────────────────────────────────────────────────────────
# Kernel Status
# ─────────────────────────────────────────────────────────────────────────────
kernel-booted = Kernel booted ({ $provider }/{ $model })
kernel-boot-failed = Kernel boot failed
models-available = { $count } models available
agents-loaded = { $count } agent(s) loaded

# ─────────────────────────────────────────────────────────────────────────────
# Agent Operations
# ─────────────────────────────────────────────────────────────────────────────
agent-spawned = Agent spawned successfully!
agent-spawned-id = ID: { $id }
agent-spawned-name = Name: { $name }
agent-spawn-failed = Failed to spawn agent: { $error }
agent-killed = Agent killed: { $id }
agent-not-found = Agent not found: { $id }

# ─────────────────────────────────────────────────────────────────────────────
# Setup & Configuration
# ─────────────────────────────────────────────────────────────────────────────
setup-welcome = Welcome to OpenFang setup!
setup-cancelled = Setup cancelled.
setup-complete = Setup complete!
setup-select-provider = Select your LLM provider:
setup-enter-api-key = Enter your API key:
setup-api-key-saved = API key saved successfully.
setup-config-created = Configuration file created.

# ─────────────────────────────────────────────────────────────────────────────
# Doctor / Diagnostics
# ─────────────────────────────────────────────────────────────────────────────
doctor-title = OpenFang Diagnostics
doctor-checking = Running health checks...
doctor-config-ok = Configuration file found
doctor-config-missing = Configuration file missing
doctor-api-key-ok = API key configured
doctor-api-key-missing = API key not configured
doctor-provider-ok = Provider { $provider } connected
doctor-provider-failed = Provider { $provider } connection failed
doctor-all-ok = All checks passed!
doctor-issues-found = { $count } issue(s) found

# ─────────────────────────────────────────────────────────────────────────────
# Chat
# ─────────────────────────────────────────────────────────────────────────────
chat-welcome = Welcome to OpenFang chat!
chat-type-message = Type your message (or 'exit' to quit):
chat-thinking = Thinking...
chat-error = Error: { $error }
chat-goodbye = Goodbye!

# ─────────────────────────────────────────────────────────────────────────────
# Skills
# ─────────────────────────────────────────────────────────────────────────────
skills-installed = Installed skills:
skills-available = Available skills:
skills-install-success = Skill '{ $name }' installed successfully.
skills-install-failed = Failed to install skill: { $error }
skills-remove-success = Skill '{ $name }' removed.
skills-not-found = Skill not found: { $name }

# ─────────────────────────────────────────────────────────────────────────────
# Channels
# ─────────────────────────────────────────────────────────────────────────────
channels-list = Configured channels:
channel-enabled = Channel '{ $name }' enabled.
channel-disabled = Channel '{ $name }' disabled.
channel-test-success = Channel '{ $name }' test successful.
channel-test-failed = Channel '{ $name }' test failed: { $error }

# ─────────────────────────────────────────────────────────────────────────────
# Errors
# ─────────────────────────────────────────────────────────────────────────────
error-generic = Error: { $message }
error-reading-file = Error reading file: { $path }
error-writing-file = Error writing file: { $path }
error-parsing-config = Error parsing configuration: { $error }
error-network = Network error: { $error }
error-api-key-required = API key required. Run 'openfang setup' to configure.
error-daemon-connection = Could not connect to daemon. Is it running?

# ─────────────────────────────────────────────────────────────────────────────
# UI Labels
# ─────────────────────────────────────────────────────────────────────────────
label-provider = Provider
label-model = Model
label-api = API
label-dashboard = Dashboard
label-status = Status
label-version = Version
label-hint = hint

# ─────────────────────────────────────────────────────────────────────────────
# Status Messages
# ─────────────────────────────────────────────────────────────────────────────
status-connected = Connected
status-disconnected = Disconnected
status-reconnecting = Reconnecting...
status-online = Online
status-offline = Offline

# ─────────────────────────────────────────────────────────────────────────────
# Hints
# ─────────────────────────────────────────────────────────────────────────────
hint-open-dashboard = Open the dashboard in your browser, or run `openfang chat`
hint-stop-daemon = Press Ctrl+C to stop the daemon
hint-run-setup = Run 'openfang setup' to configure your API key

# ─────────────────────────────────────────────────────────────────────────────
# TUI
# ─────────────────────────────────────────────────────────────────────────────
tui-title = OpenFang Terminal UI
tui-chat = Chat with an agent
tui-dashboard = Open dashboard
tui-terminal = Launch terminal UI
tui-desktop = Open desktop app
tui-settings = Settings
tui-all-commands = Show all commands
tui-navigate = navigate
tui-select = select
tui-quit = quit
