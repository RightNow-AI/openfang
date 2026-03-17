/**
 * @openfang/sdk — Official JavaScript client for the OpenFang Agent OS REST API.
 *
 * Usage:
 *   const { OpenFang } = require("@openfang/sdk");
 *   const client = new OpenFang("http://localhost:3000");
 *
 *   const agent = await client.agents.create({ template: "assistant" });
 *   const reply = await client.agents.message(agent.id, "Hello!");
 *   console.log(reply);
 *
 *   // Streaming:
 *   for await (const event of client.agents.stream(agent.id, "Tell me a joke")) {
 *     process.stdout.write(event.delta || "");
 *   }
 */

"use strict";

class OpenFangError extends Error {
  constructor(message, status, body) {
    super(message);
    this.name = "OpenFangError";
    this.status = status;
    this.body = body;
  }
}

class OpenFang {
  /**
   * @param {string} baseUrl - OpenFang server URL (e.g. "http://localhost:3000")
   * @param {object} [opts]
   * @param {Record<string, string>} [opts.headers] - Extra headers for every request
   */
  constructor(baseUrl, opts) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
    this._headers = Object.assign(
      { "Content-Type": "application/json" },
      (opts && opts.headers) || {}
    );
    this.agents = new AgentResource(this);
    this.sessions = new SessionResource(this);
    this.workflows = new WorkflowResource(this);
    this.skills = new SkillResource(this);
    this.mcp = new McpResource(this);
    this.channels = new ChannelResource(this);
    this.tools = new ToolResource(this);
    this.models = new ModelResource(this);
    this.providers = new ProviderResource(this);
    this.memory = new MemoryResource(this);
    this.triggers = new TriggerResource(this);
    this.schedules = new ScheduleResource(this);
  }

  /** Low-level fetch wrapper. */
  async _request(method, path, body) {
    var url = this.baseUrl + path;
    var init = { method: method, headers: Object.assign({}, this._headers) };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    var res = await fetch(url, init);
    if (!res.ok) {
      var text = await res.text().catch(function () {
        return "";
      });
      throw new OpenFangError(
        "HTTP " + res.status + ": " + text,
        res.status,
        text
      );
    }
    var ct = res.headers.get("content-type") || "";
    if (ct.includes("application/json")) {
      return res.json();
    }
    return res.text();
  }

  /** Low-level SSE streaming. Returns an async iterator of parsed events. */
  async *_stream(method, path, body) {
    var url = this.baseUrl + path;
    var headers = Object.assign({}, this._headers, {
      Accept: "text/event-stream",
    });
    var init = { method: method, headers: headers };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    var res = await fetch(url, init);
    if (!res.ok) {
      var text = await res.text().catch(function () {
        return "";
      });
      throw new OpenFangError(
        "HTTP " + res.status + ": " + text,
        res.status,
        text
      );
    }
    var reader = res.body.getReader();
    var decoder = new TextDecoder();
    var buffer = "";
    while (true) {
      var result = await reader.read();
      if (result.done) break;
      buffer += decoder.decode(result.value, { stream: true });
      var lines = buffer.split("\n");
      buffer = lines.pop() || "";
      for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim();
        if (line.startsWith("data: ")) {
          var data = line.slice(6);
          if (data === "[DONE]") return;
          try {
            yield JSON.parse(data);
          } catch (_) {
            yield { raw: data };
          }
        }
      }
    }
  }

  /** Health check. */
  async health() {
    return this._request("GET", "/api/health");
  }

  /** Detailed health. */
  async healthDetail() {
    return this._request("GET", "/api/health/detail");
  }

  /** Server status. */
  async status() {
    return this._request("GET", "/api/status");
  }

  /** Server version. */
  async version() {
    return this._request("GET", "/api/version");
  }

  /** Prometheus metrics (text). */
  async metrics() {
    return this._request("GET", "/api/metrics");
  }

  /** Usage statistics. */
  async usage() {
    return this._request("GET", "/api/usage");
  }

  /** Config. */
  async config() {
    return this._request("GET", "/api/config");
  }
}

// ── Agent Resource ──────────────────────────────────────────────

class AgentResource {
  constructor(client) {
    this._c = client;
  }

  /** List all agents. */
  async list() {
    return this._c._request("GET", "/api/agents");
  }

  /** Get agent by ID. */
  async get(id) {
    return this._c._request("GET", "/api/agents/" + id);
  }

  /** Create (spawn) a new agent.
   * @param {object} opts - e.g. { template: "assistant", name: "My Agent" }
   */
  async create(opts) {
    return this._c._request("POST", "/api/agents", opts);
  }

  /** Delete (kill) an agent. */
  async delete(id) {
    return this._c._request("DELETE", "/api/agents/" + id);
  }

  /** Stop an agent. */
  async stop(id) {
    return this._c._request("POST", "/api/agents/" + id + "/stop");
  }

  /** Clone an agent. */
  async clone(id) {
    return this._c._request("POST", "/api/agents/" + id + "/clone");
  }

  /** Update agent. */
  async update(id, data) {
    return this._c._request("PUT", "/api/agents/" + id + "/update", data);
  }

  /** Set agent mode. */
  async setMode(id, mode) {
    return this._c._request("PUT", "/api/agents/" + id + "/mode", {
      mode: mode,
    });
  }

  /** Set agent model. */
  async setModel(id, model, provider) {
    return this._c._request("PUT", "/api/agents/" + id + "/model", {
      model: model,
      provider: provider,
    });
  }

  /** Send a message and get the full response. */
  async message(id, text, opts) {
    var body = Object.assign({ message: text }, opts || {});
    return this._c._request("POST", "/api/agents/" + id + "/message", body);
  }

  /** Send a message and stream the response (async iterator of SSE events).
   * @example
   *   for await (const evt of client.agents.stream(id, "Hello")) {
   *     if (evt.type === "text_delta") process.stdout.write(evt.delta);
   *   }
   */
  async *stream(id, text, opts) {
    var body = Object.assign({ message: text }, opts || {});
    yield* this._c._stream(
      "POST",
      "/api/agents/" + id + "/message/stream",
      body
    );
  }

  /** Get agent session. */
  async session(id) {
    return this._c._request("GET", "/api/agents/" + id + "/session");
  }

  /** Reset agent session. */
  async resetSession(id) {
    return this._c._request("POST", "/api/agents/" + id + "/session/reset");
  }

  /** Compact session. */
  async compactSession(id) {
    return this._c._request("POST", "/api/agents/" + id + "/session/compact");
  }

  /** List sessions for an agent. */
  async listSessions(id) {
    return this._c._request("GET", "/api/agents/" + id + "/sessions");
  }

  /** Create a new session. */
  async createSession(id, label) {
    return this._c._request("POST", "/api/agents/" + id + "/sessions", {
      label: label,
    });
  }

  /** Switch to a session. */
  async switchSession(id, sessionId) {
    return this._c._request(
      "POST",
      "/api/agents/" + id + "/sessions/" + sessionId + "/switch"
    );
  }

  /** Get agent skills. */
  async getSkills(id) {
    return this._c._request("GET", "/api/agents/" + id + "/skills");
  }

  /** Set agent skills. */
  async setSkills(id, skills) {
    return this._c._request("PUT", "/api/agents/" + id + "/skills", skills);
  }

  // /** Upload a file to agent. */
  // async upload(id, file, filename) {
  //   var url = this._c.baseUrl + "/api/agents/" + id + "/upload";
  //   var form = new FormData();
  //   form.append("file", file, filename);
  //   var res = await fetch(url, { method: "POST", body: form });
  //   if (!res.ok)
  //     throw new OpenFangError("Upload failed: " + res.status, res.status);
  //   return res.json();
  // }

  async upload(agentId, file) {
    var hdrs = {
      "Content-Type": file.type || "application/octet-stream",
      "X-Filename": file.name,
    };
    return fetch(this._c.baseUrl + "/api/agents/" + agentId + "/upload", {
      method: "POST",
      headers: hdrs,
      body: file,
    }).then(function (r) {
      if (!r.ok) throw new Error("Upload failed");
      return r.json();
    });
  }

  /** Update agent identity. */
  async setIdentity(id, identity) {
    return this._c._request(
      "PATCH",
      "/api/agents/" + id + "/identity",
      identity
    );
  }

  /** Patch agent config. */
  async patchConfig(id, config) {
    return this._c._request("PATCH", "/api/agents/" + id + "/config", config);
  }

  /**set Soul MD */
  async setSoulMD(id, soulContent) {
    return this._c._request("PUT", "/api/agents/" + id + "/files/SOUL.md", {
      content: "# Soul\n" + soulContent,
    });
  }
  /**get agent config */
  async getConfig(id) {
    return this._c._request("GET", "/api/agents/" + id);
  }

  /**get agent files */
  async getFiles(id) {
    return this._c._request("GET", "/api/agents/" + id + "/files");
  }

  /**set agent file */
  async setFile(id, filename, content) {
    return this._c._request("PUT", "/api/agents/" + id + "/files/" + filename, {
      content: content,
    });
  }

  /**get agent file */
  async getFile(id, filename) {
    return this._c._request("GET", "/api/agents/" + id + "/files/" + filename);
  }

  /**set agent workspace */
  async setWorkspace(id, path) {
    return this._c._request("PUT", "/api/agents/" + id + "/workspace", {
      path: path,
    });
  }

  async delHistory(id) {
    return this._c._request("DELETE", "/api/agents/" + id + "/history");
  }
}

// ── Session Resource ────────────────────────────────────────────

class SessionResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/sessions");
  }

  async delete(id) {
    return this._c._request("DELETE", "/api/sessions/" + id);
  }

  async setLabel(id, label) {
    return this._c._request("PUT", "/api/sessions/" + id + "/label", {
      label: label,
    });
  }

  async getSession(id) {
    return this._c._request("GET", "/api/sessions/" + id);
  }
}

// ── Workflow Resource ───────────────────────────────────────────

class WorkflowResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/workflows");
  }

  async create(workflow) {
    return this._c._request("POST", "/api/workflows", workflow);
  }

  async run(id, input) {
    return this._c._request("POST", "/api/workflows/" + id + "/run", input);
  }

  async runs(id) {
    return this._c._request("GET", "/api/workflows/" + id + "/runs");
  }
}

// ── Mcp Resource ──────────────────────────────────────────────
class McpResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/mcp/servers");
  }

  async list_available(id) {
    return this._c._request("GET", "/api/agents/" + id + "/mcp_servers");
  }

  async setAvailable(id, mcp_servers = []) {
    return this._c._request("PUT", "/api/agents/" + id + "/mcp_servers", {
      mcp_servers,
    });
  }
}

// ── Skill Resource ──────────────────────────────────────────────
class SkillResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/skills");
  }

  async install(skill) {
    return this._c._request("POST", "/api/skills/install", skill);
  }

  async uninstall(skill) {
    return this._c._request("POST", "/api/skills/uninstall", skill);
  }

  async search(query) {
    return this._c._request(
      "GET",
      "/api/marketplace/search?q=" + encodeURIComponent(query)
    );
  }

  async getClawHubBrowse(query) {
    return this._c._request(
      "GET",
      "/api/clawhub/browse?sort=" + query.sort + "&limit=" + query.limit
    );
  }

  /**search ClawHub */
  async searchClawHub(query) {
    return this._c._request(
      "GET",
      "/api/clawhub/search?q=" + query.q + "&limit=" + query.limit
    );
  }

  // install ClawHub
  async installClawHub(query) {
    return this._c._request("POST", "/api/clawhub/install", query);
  }

  // created skill
  async create(query) {
    return this._c._request("POST", "/api/skills/create", query);
  }
}

// ── Channel Resource ────────────────────────────────────────────

class ChannelResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/channels");
  }

  async configure(name, config) {
    return this._c._request(
      "POST",
      "/api/channels/" + name + "/configure",
      config
    );
  }

  async remove(name) {
    return this._c._request("DELETE", "/api/channels/" + name + "/configure");
  }

  async test(name) {
    return this._c._request("POST", "/api/channels/" + name + "/test");
  }
}

// ── Tool Resource ───────────────────────────────────────────────

class ToolResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/tools");
  }

  async getBuiltinTools(id) {
    return this._c._request("GET", "/api/agents/" + id + "/builtin_tools");
  }

  async setBuiltinTools(id, { allowlist, blocklist } = {}) {
    return this._c._request("PUT", "/api/agents/" + id + "/builtin_tools", {
      tool_allowlist: allowlist || [],
      tool_blocklist: blocklist || [],
    });
  }
}

// ── Model Resource ──────────────────────────────────────────────

class ModelResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/models");
  }

  async get(id) {
    return this._c._request("GET", "/api/models/" + id);
  }

  async aliases() {
    return this._c._request("GET", "/api/models/aliases");
  }

  /**
   * Sync UniGPT custom models into the model catalog.
   * Models present in the array are added/updated; models previously added
   * via this endpoint but absent from the array are removed.
   *
   * @param {Array<{id: string, name?: string, context_window?: number, max_output_tokens?: number}>} models
   * @returns {{ status: string, added_count: number, updated_count: number, removed_count: number }}
   */
  async updateUnigptModels(models) {
    return this._c._request("POST", "/unigpt/models", models);
  }

  async setToken(token) {
    return this._c._request("POST", "/unigpt/set_token", {
      token: token,
    });
  }
}

// ── Provider Resource ───────────────────────────────────────────

class ProviderResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/providers");
  }

  async setKey(name, key) {
    return this._c._request("POST", "/api/providers/" + name + "/key", {
      key: key,
    });
  }

  async deleteKey(name) {
    return this._c._request("DELETE", "/api/providers/" + name + "/key");
  }

  async test(name) {
    return this._c._request("POST", "/api/providers/" + name + "/test");
  }

  async setUrl(name, url) {
    return this._c._request("PUT", "/api/providers/" + name + "/url", {
      base_url: url,
    });
  }
}

// ── Memory Resource ─────────────────────────────────────────────

class MemoryResource {
  constructor(client) {
    this._c = client;
  }

  async getAll(agentId) {
    return this._c._request("GET", "/api/memory/agents/" + agentId + "/kv");
  }

  async get(agentId, key) {
    return this._c._request(
      "GET",
      "/api/memory/agents/" + agentId + "/kv/" + key
    );
  }

  async set(agentId, key, value) {
    return this._c._request(
      "PUT",
      "/api/memory/agents/" + agentId + "/kv/" + key,
      { value: value }
    );
  }

  async delete(agentId, key) {
    return this._c._request(
      "DELETE",
      "/api/memory/agents/" + agentId + "/kv/" + key
    );
  }
}

// ── Trigger Resource ────────────────────────────────────────────

class TriggerResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/triggers");
  }

  async create(trigger) {
    return this._c._request("POST", "/api/triggers", trigger);
  }

  async update(id, trigger) {
    return this._c._request("PUT", "/api/triggers/" + id, trigger);
  }

  async delete(id) {
    return this._c._request("DELETE", "/api/triggers/" + id);
  }
}

// ── Schedule Resource ───────────────────────────────────────────

class ScheduleResource {
  constructor(client) {
    this._c = client;
  }

  async list() {
    return this._c._request("GET", "/api/schedules");
  }

  async create(schedule) {
    return this._c._request("POST", "/api/schedules", schedule);
  }

  async update(id, schedule) {
    return this._c._request("PUT", "/api/schedules/" + id, schedule);
  }

  async delete(id) {
    return this._c._request("DELETE", "/api/schedules/" + id);
  }

  async run(id) {
    return this._c._request("POST", "/api/schedules/" + id + "/run");
  }
}

// ── Exports ─────────────────────────────────────────────────────

module.exports = { OpenFang: OpenFang, OpenFangError: OpenFangError };
