# MCP & A2A Integration Guide

OpenFang implements both the **Model Context Protocol (MCP)** and **Agent-to-Agent (A2A)** protocol, enabling deep interoperability with external tools, IDEs, and other agent frameworks.

---

## Table of Contents

- [Part 1: MCP (Model Context Protocol)](#part-1-mcp-model-context-protocol)
  - [Overview](#mcp-overview)
  - [MCP Client -- Connecting to External Servers](#mcp-client)
  - [MCP Server -- Exposing OpenFang via MCP](#mcp-server)
  - [Configuration Examples](#mcp-configuration-examples)
  - [API Endpoints](#mcp-api-endpoints)
- [Part 2: A2A (Agent-to-Agent Protocol)](#part-2-a2a-agent-to-agent-protocol)
  - [Overview](#a2a-overview)
  - [Agent Card](#agent-card)
  - [A2A Server](#a2a-server)
  - [A2A Client](#a2a-client)
  - [Task Lifecycle](#task-lifecycle)
  - [API Endpoints](#a2a-api-endpoints)
  - [Configuration](#a2a-configuration)
- [Security](#security)

---

## Part 1: MCP (Model Context Protocol)

### MCP Overview

The Model Context Protocol (MCP) is a JSON-RPC 2.0 based protocol that standardizes how LLM applications discover and invoke tools. OpenFang supports MCP in both directions:

- **As a client**: OpenFang connects to external MCP servers (GitHub, filesystem, databases, Puppeteer, etc.) and makes their tools available to all agents.
- **As a server**: OpenFang exposes its own agents as MCP tools, so IDEs like Cursor, VS Code, and Claude Desktop can call OpenFang agents directly.

OpenFang implements MCP protocol version `2024-11-05`.

**Source files:**
- Client: `crates/openfang-runtime/src/mcp.rs`
- Server handler: `crates/openfang-runtime/src/mcp_server.rs`
- CLI server: `crates/openfang-cli/src/mcp.rs`
- Config types: `crates/openfang-types/src/config.rs` (`McpServerConfigEntry`, `McpTransportEntry`)

---

### MCP Client

The MCP client (`McpConnection` in `openfang-runtime`) allows OpenFang to connect to any MCP-compatible server and use its tools as if they were built-in.

#### Configuration

MCP servers are configured in `config.toml` using the `[[mcp_servers]]` array:

```toml
[[mcp_servers]]
name = "github"
timeout_secs = 30
env = ["GITHUB_PERSONAL_ACCESS_TOKEN"]

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

Each entry maps to a `McpServerConfigEntry` struct:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `String` | required | Display name, used in tool namespacing |
| `transport` | `McpTransportEntry` | required | How to connect (stdio or SSE) |
| `timeout_secs` | `u64` | `30` | JSON-RPC request timeout |
| `env` | `Vec<String>` | `[]` | Env vars to pass through to the subprocess |

#### Transport Types

OpenFang supports two MCP transports, defined by `McpTransport`:

**Stdio** -- Spawns a subprocess and communicates via stdin/stdout with newline-delimited JSON-RPC:

```toml
[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

**SSE** -- Connects to a remote HTTP endpoint and sends JSON-RPC via POST:

```toml
[mcp_servers.transport]
type = "sse"
url = "https://mcp.example.com/api"
```

#### Tool Namespacing

All tools discovered from MCP servers are namespaced using the pattern `mcp_{server}_{tool}` to prevent collisions with built-in tools or tools from other servers. Names are normalized to lowercase with hyphens replaced by underscores.

Examples:
- Server `github`, tool `create_issue` becomes `mcp_github_create_issue`
- Server `my-server`, tool `do_thing` becomes `mcp_my_server_do_thing`

Helper functions (exported from `openfang_runtime::mcp`):
- `format_mcp_tool_name(server, tool)` -- builds the namespaced name
- `is_mcp_tool(name)` -- checks if a tool name starts with `mcp_`
- `extract_mcp_server(tool_name)` -- extracts the server name from a namespaced tool

#### Auto-Connection on Kernel Boot

When the kernel starts (`start_background_agents()`), it checks `config.mcp_servers`. If any are configured, it spawns a background task that calls `connect_mcp_servers()`. This method:

1. Iterates each `McpServerConfigEntry` in the config
2. Converts the config-level `McpTransportEntry` into a runtime `McpTransport`
3. Calls `McpConnection::connect()` which:
   - Spawns the subprocess (stdio) or creates an HTTP client (SSE)
   - Sends the `initialize` handshake with client info
   - Sends the `notifications/initialized` notification
   - Calls `tools/list` to discover all available tools
   - Namespaces each tool with `mcp_{server}_{tool}`
4. Caches discovered `ToolDefinition` entries in `kernel.mcp_tools`
5. Stores the live `McpConnection` in `kernel.mcp_connections`

After connection, the kernel logs the total number of MCP tools available.

#### Tool Discovery and Listing

MCP tools are merged into the agent's available tool set via `available_tools()`:

```
built-in tools (23) + skill tools + MCP tools = full tool list
```

When an agent calls an MCP tool during its loop, the tool runner recognizes the `mcp_` prefix, finds the appropriate `McpConnection`, strips the namespace prefix, and forwards the `tools/call` request to the external MCP server.

#### Connection Lifecycle

The `McpConnection` struct manages the lifetime of the connection:

```rust
pub struct McpConnection {
    config: McpServerConfig,
    tools: Vec<ToolDefinition>,
    transport: McpTransportHandle,  // Stdio or SSE
    next_id: u64,                   // JSON-RPC request counter
}
```

When the connection is dropped, stdio subprocesses are automatically killed via `Drop`:

```rust
impl Drop for McpConnection {
    fn drop(&mut self) {
        if let McpTransportHandle::Stdio { ref mut child, .. } = self.transport {
            let _ = child.start_kill();
        }
    }
}
```

---

### MCP Server

OpenFang can also act as an MCP server, exposing its agents as callable tools to external MCP clients.

#### How It Works

Each OpenFang agent becomes an MCP tool named `openfang_agent_{name}` (with hyphens replaced by underscores). The tool accepts a single `message` string parameter and returns the agent's response.

For example, an agent named `code-reviewer` becomes the MCP tool `openfang_agent_code_reviewer`.

#### CLI: `openfang mcp`

The primary way to run the MCP server is the `openfang mcp` command, which starts a stdio-based MCP server:

```bash
openfang mcp
```

This command:
1. Checks if an OpenFang daemon is running (via `find_daemon()`)
2. If found, proxies all tool calls to the daemon via its HTTP API
3. If no daemon is running, boots an in-process kernel as a fallback
4. Reads Content-Length framed JSON-RPC messages from stdin
5. Writes Content-Length framed JSON-RPC responses to stdout

The MCP server uses `McpBackend` which supports two modes:
- `McpBackend::Daemon` -- forwards requests to a running OpenFang daemon via HTTP
- `McpBackend::InProcess` -- boots a full kernel when no daemon is available

#### HTTP MCP Endpoint

OpenFang also exposes an MCP endpoint over HTTP at `POST /mcp`. Unlike the stdio server (which only exposes agents), the HTTP endpoint exposes the full tool set (built-in + skills + MCP tools) and executes tools via the kernel's `execute_tool()` pipeline. This means the HTTP MCP endpoint supports:

- All 23 built-in tools (file_read, web_fetch, etc.)
- All installed skill tools
- All connected MCP server tools

#### Supported JSON-RPC Methods

| Method | Description |
|--------|-------------|
| `initialize` | Handshake; returns server capabilities and info |
| `notifications/initialized` | Client confirmation; no response |
| `tools/list` | Returns all available tools with names, descriptions, and input schemas |
| `tools/call` | Executes a tool and returns the result |

Unknown methods receive a `-32601` (Method not found) error.

#### Protocol Details

**Message Framing** (stdio mode):

```
Content-Length: 123\r\n
\r\n
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
```

Messages are limited to 10 MB (`MAX_MCP_MESSAGE_SIZE`). Oversized messages are drained and rejected.

**Initialize Handshake:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "cursor", "version": "1.0" }
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": { "tools": {} },
    "serverInfo": { "name": "openfang", "version": "0.1.0" }
  }
}
```

**Tool Call:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "openfang_agent_code_reviewer",
    "arguments": {
      "message": "Review this Python function for security issues..."
    }
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [{
      "type": "text",
      "text": "I found 3 potential security issues..."
    }]
  }
}
```

#### Connecting from IDEs

**Cursor / VS Code (with MCP extension):**

Add to your MCP configuration file (e.g., `.cursor/mcp.json` or VS Code MCP settings):

```json
{
  "mcpServers": {
    "openfang": {
      "command": "openfang",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop:**

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "openfang": {
      "command": "openfang",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

After configuration, all OpenFang agents appear as tools in the IDE. For example, you can ask Claude Desktop to "use the openfang code-reviewer agent to review this file."

---

### MCP Configuration Examples

#### GitHub Server (file + issue + PR tools)

```toml
[[mcp_servers]]
name = "github"
timeout_secs = 30
env = ["GITHUB_PERSONAL_ACCESS_TOKEN"]

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

#### Filesystem Server

```toml
[[mcp_servers]]
name = "filesystem"
timeout_secs = 10
env = []

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"]
```

#### PostgreSQL Server

```toml
[[mcp_servers]]
name = "postgres"
timeout_secs = 30
env = ["DATABASE_URL"]

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
```

#### Puppeteer (Browser Automation)

```toml
[[mcp_servers]]
name = "puppeteer"
timeout_secs = 60

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-puppeteer"]
```

#### Remote SSE Server

```toml
[[mcp_servers]]
name = "remote-tools"
timeout_secs = 30

[mcp_servers.transport]
type = "sse"
url = "https://tools.example.com/mcp"
```

#### Multiple Servers

```toml
[[mcp_servers]]
name = "github"
env = ["GITHUB_PERSONAL_ACCESS_TOKEN"]
[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[[mcp_servers]]
name = "filesystem"
[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"]

[[mcp_servers]]
name = "postgres"
env = ["DATABASE_URL"]
[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
```

---

### MCP API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/mcp/servers` | List configured and connected MCP servers with their tools |
| `POST` | `/mcp` | Handle MCP JSON-RPC requests over HTTP (full tool execution) |

**GET /api/mcp/servers** response:

```json
{
  "configured": [
    {
      "name": "github",
      "transport": { "type": "stdio", "command": "npx", "args": [...] },
      "timeout_secs": 30,
      "env": ["GITHUB_PERSONAL_ACCESS_TOKEN"]
    }
  ],
  "connected": [
    {
      "name": "github",
      "tools_count": 12,
      "tools": [
        { "name": "mcp_github_create_issue", "description": "[MCP:github] Create a GitHub issue" },
        { "name": "mcp_github_search_repos", "description": "[MCP:github] Search repositories" }
      ],
      "connected": true
    }
  ]
}
```

---

## Part 2: A2A (Agent-to-Agent Protocol)

### A2A Overview

The Agent-to-Agent (A2A) protocol, originally specified by Google, enables cross-framework agent interoperability. It allows agents built with different frameworks to discover each other's capabilities and exchange tasks.

OpenFang implements A2A in both directions:

- **As a server**: Publishes Agent Cards describing each agent's capabilities, accepts task submissions, and tracks task lifecycle.
- **As a client**: Discovers external A2A agents at boot time, sends tasks to them, and polls for results.

**Source files:**
- Protocol types and logic: `crates/openfang-runtime/src/a2a.rs`
- API routes: `crates/openfang-api/src/routes.rs`
- Config types: `crates/openfang-types/src/config.rs` (`A2aConfig`, `ExternalAgent`)

---

### Agent Card

An Agent Card is a JSON document that describes an agent's identity, capabilities, and supported interaction modes. It is served at the well-known path `/.well-known/agent.json` per the A2A specification.

The `AgentCard` struct:

```rust
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,                         // endpoint URL (e.g., "http://host/a2a")
    pub version: String,                     // protocol version
    pub capabilities: AgentCapabilities,
    pub skills: Vec<AgentSkill>,             // A2A skill descriptors
    pub default_input_modes: Vec<String>,    // e.g., ["text"]
    pub default_output_modes: Vec<String>,   // e.g., ["text"]
}
```

**AgentCapabilities:**

```rust
pub struct AgentCapabilities {
    pub streaming: bool,                 // true -- OpenFang supports streaming
    pub push_notifications: bool,        // false -- not currently implemented
    pub state_transition_history: bool,  // true -- task status history available
}
```

**AgentSkill** (not the same as OpenFang skills -- these are A2A capability descriptors):

```rust
pub struct AgentSkill {
    pub id: String,           // matches the OpenFang tool name
    pub name: String,         // human-readable (underscores replaced with spaces)
    pub description: String,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
}
```

Agent Cards are built from OpenFang agent manifests via `build_agent_card()`. Each tool in the agent's capability list becomes an A2A skill descriptor. Example card:

```json
{
  "name": "code-reviewer",
  "description": "Reviews code for bugs, security issues, and style",
  "url": "http://127.0.0.1:4200/a2a",
  "version": "0.1.0",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "stateTransitionHistory": true
  },
  "skills": [
    {
      "id": "file_read",
      "name": "file read",
      "description": "Can use the file_read tool",
      "tags": ["tool"],
      "examples": []
    }
  ],
  "defaultInputModes": ["text"],
  "defaultOutputModes": ["text"]
}
```

---

### A2A Server

OpenFang serves A2A requests through the REST API. The server-side implementation involves:

1. **Agent Card publication** at `/.well-known/agent.json`
2. **Agent listing** at `/a2a/agents`
3. **Task submission and tracking** via the `A2aTaskStore`

#### A2aTaskStore

The `A2aTaskStore` is an in-memory, bounded store for tracking A2A task lifecycle:

```rust
pub struct A2aTaskStore {
    tasks: Mutex<HashMap<String, A2aTask>>,
    max_tasks: usize,  // default: 1000
}
```

Key properties:
- **Bounded**: When the store reaches `max_tasks`, it evicts the oldest completed/failed/cancelled task (FIFO)
- **Thread-safe**: Uses `Mutex<HashMap>` for concurrent access
- **Kernel field**: Stored as `kernel.a2a_task_store`

Methods on `A2aTaskStore`:
- `insert(task)` -- add a new task, evicting old ones if at capacity
- `get(task_id)` -- retrieve a task by ID
- `update_status(task_id, status)` -- change a task's status
- `complete(task_id, response, artifacts)` -- mark as completed with response
- `fail(task_id, error_message)` -- mark as failed with error
- `cancel(task_id)` -- mark as cancelled

#### Task Submission Flow

When `POST /a2a/tasks/send` is called:

1. Extract the message text from the A2A request format (parts with type "text")
2. Find the target agent (currently uses the first registered agent)
3. Create an `A2aTask` with status `Working` and insert into the task store
4. Send the message to the agent via `kernel.send_message()`
5. On success: complete the task with the agent's response
6. On failure: fail the task with the error message
7. Return the final task state

---

### A2A Client

The `A2aClient` struct discovers and interacts with external A2A agents:

```rust
pub struct A2aClient {
    client: reqwest::Client,  // 30-second timeout
}
```

**Methods:**

- `discover(url)` -- fetches `{url}/.well-known/agent.json` and parses the Agent Card
- `send_task(url, message, session_id)` -- sends a JSON-RPC task submission
- `get_task(url, task_id)` -- polls for task status

#### Auto-Discovery at Boot

When the kernel starts and A2A is enabled with external agents configured, it spawns a background task that calls `discover_external_agents()`. This function:

1. Creates an `A2aClient`
2. Iterates each configured `ExternalAgent`
3. Fetches each agent's card from `{url}/.well-known/agent.json`
4. Logs successful discoveries (name, URL, skill count)
5. Stores discovered `(name, AgentCard)` pairs in `kernel.a2a_external_agents`

Failed discoveries are logged as warnings but do not prevent boot.

#### Sending Tasks to External Agents

```rust
let client = A2aClient::new();
let task = client.send_task(
    "https://other-agent.example.com/a2a",
    "Analyze this dataset for anomalies",
    Some("session-123"),
).await?;
println!("Task {}: {:?}", task.id, task.status);
```

The client sends a JSON-RPC request:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tasks/send",
  "params": {
    "message": {
      "role": "user",
      "parts": [{ "type": "text", "text": "Analyze this dataset..." }]
    },
    "sessionId": "session-123"
  }
}
```

---

### Task Lifecycle

An `A2aTask` tracks the full lifecycle of a cross-agent interaction:

```rust
pub struct A2aTask {
    pub id: String,
    pub session_id: Option<String>,
    pub status: A2aTaskStatus,
    pub messages: Vec<A2aMessage>,
    pub artifacts: Vec<A2aArtifact>,
}
```

#### Task States

| Status | Description |
|--------|-------------|
| `Submitted` | Task received but not yet started |
| `Working` | Task is being actively processed by the agent |
| `InputRequired` | Agent needs more information from the caller |
| `Completed` | Task finished successfully |
| `Cancelled` | Task was cancelled by the caller |
| `Failed` | Task encountered an error |

#### Message Format

Messages use an A2A-specific format with typed content parts:

```rust
pub struct A2aMessage {
    pub role: String,          // "user" or "agent"
    pub parts: Vec<A2aPart>,
}

pub enum A2aPart {
    Text { text: String },
    File { name: String, mime_type: String, data: String },  // base64
    Data { mime_type: String, data: serde_json::Value },
}
```

#### Artifacts

Tasks can produce artifacts (files, structured data) alongside messages:

```rust
pub struct A2aArtifact {
    pub name: String,
    pub parts: Vec<A2aPart>,
}
```

---

### A2A API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/.well-known/agent.json` | Public | Agent Card for the primary agent |
| `GET` | `/a2a/agents` | Public | List all agent cards |
| `POST` | `/a2a/tasks/send` | Public | Submit a task to an agent |
| `GET` | `/a2a/tasks/{id}` | Public | Get task status and messages |
| `POST` | `/a2a/tasks/{id}/cancel` | Public | Cancel a running task |

#### GET /.well-known/agent.json

Returns the Agent Card for the first registered agent. If no agents are spawned, returns a placeholder card.

#### GET /a2a/agents

Lists all registered agents as Agent Cards:

```json
{
  "agents": [
    {
      "name": "code-reviewer",
      "description": "Reviews code for bugs and security issues",
      "url": "http://127.0.0.1:4200/a2a",
      "version": "0.1.0",
      "capabilities": { "streaming": true, "pushNotifications": false, "stateTransitionHistory": true },
      "skills": [...],
      "defaultInputModes": ["text"],
      "defaultOutputModes": ["text"]
    }
  ],
  "total": 1
}
```

#### POST /a2a/tasks/send

Submit a task. Request body follows JSON-RPC 2.0 format:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tasks/send",
  "params": {
    "message": {
      "role": "user",
      "parts": [{ "type": "text", "text": "Review this code for security issues" }]
    },
    "sessionId": "optional-session-id"
  }
}
```

Response (completed task):

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "sessionId": "optional-session-id",
  "status": "completed",
  "messages": [
    {
      "role": "user",
      "parts": [{ "type": "text", "text": "Review this code for security issues" }]
    },
    {
      "role": "agent",
      "parts": [{ "type": "text", "text": "I found 2 potential issues..." }]
    }
  ],
  "artifacts": []
}
```

#### GET /a2a/tasks/{id}

Poll for task status. Returns `404` if the task is not found or has been evicted.

#### POST /a2a/tasks/{id}/cancel

Cancel a running task. Sets its status to `Cancelled`. Returns `404` if the task is not found.

---

### A2A Configuration

A2A is configured in `config.toml` under the `[a2a]` section:

```toml
[a2a]
enabled = true
listen_path = "/a2a"

[[a2a.external_agents]]
name = "research-agent"
url = "https://research.example.com"

[[a2a.external_agents]]
name = "data-analyst"
url = "https://data.example.com"
```

The `A2aConfig` struct:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Whether A2A endpoints are active |
| `listen_path` | `String` | `"/a2a"` | Base path for A2A endpoints |
| `external_agents` | `Vec<ExternalAgent>` | `[]` | External agents to discover at boot |

Each `ExternalAgent`:

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Display name for this external agent |
| `url` | `String` | Base URL where the agent's card is published |

If `a2a` is `None` (not present in config), all A2A features are disabled. The A2A endpoints are always registered in the router but the discovery and task store functionality requires `enabled = true`.

---

## Security

### MCP Security

**Subprocess Sandboxing**: Stdio MCP servers run with `env_clear()` -- the subprocess environment is completely cleared. Only explicitly whitelisted environment variables (listed in the `env` field) plus `PATH` are passed through. This prevents leaking secrets to untrusted MCP server processes.

**Path Traversal Prevention**: The command path for stdio MCP servers is validated to reject `..` sequences.

**SSRF Protection**: SSE transport URLs are checked against known metadata endpoints (169.254.169.254, metadata.google) to prevent SSRF attacks.

**Request Timeout**: All MCP requests have a configurable timeout (default 30 seconds) to prevent hung connections.

**Message Size Limit**: The stdio MCP server enforces a 10 MB maximum message size to prevent out-of-memory attacks. Oversized messages are drained and rejected.

### A2A Security

**Rate Limiting**: A2A endpoints go through the same GCRA rate limiter as all other API endpoints.

**API Authentication**: When `api_key` is set in the kernel config, all API endpoints (including A2A) require a `Authorization: Bearer <key>` header. The exception is `/.well-known/agent.json` and the health endpoint which are typically public.

**Task Store Bounds**: The `A2aTaskStore` is bounded (default 1000 tasks) with FIFO eviction of completed/failed/cancelled tasks, preventing memory exhaustion from task accumulation.

**External Agent Discovery**: The `A2aClient` uses a 30-second timeout and sends a `User-Agent: OpenFang/0.1 A2A` header. Failed discoveries are logged but do not block kernel boot.

### Kernel-Level Protection

Both MCP and A2A tool execution flows through the same security pipeline as all other tool calls:
- Capability-based access control (agents only get tools they are authorized for)
- Tool result truncation (50K character hard cap)
- Universal 60-second tool execution timeout
- Loop guard detection (blocks repetitive tool call patterns)
- Taint tracking on data flowing between tools
