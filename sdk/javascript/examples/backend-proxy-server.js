/**
 * Thin backend proxy example for product chat.
 *
 * Exposes:
 *   POST /api/ai/chat
 *   POST /api/ai/chat/stream
 *   GET  /api/health
 *
 * Usage:
 *   OPENFANG_BASE_URL=http://127.0.0.1:50051 node backend-proxy-server.js
 */

const http = require("node:http");
const { OpenFang } = require("../index");

const PORT = parseInt(process.env.APP_BACKEND_PORT || "3100", 10);
const OPENFANG_BASE_URL = process.env.OPENFANG_BASE_URL || "http://127.0.0.1:50051";
const OPENFANG_API_KEY = process.env.OPENFANG_API_KEY || "";
const OPENFANG_DEFAULT_TEMPLATE = process.env.OPENFANG_DEFAULT_TEMPLATE || "assistant";

const client = new OpenFang(OPENFANG_BASE_URL, {
  headers: OPENFANG_API_KEY
    ? { Authorization: `Bearer ${OPENFANG_API_KEY}` }
    : {},
});

// Replace this in real applications with a database table keyed by user/tenant/workspace.
const agentIdsByUser = new Map();

async function openfangRequest(path) {
  const response = await fetch(`${OPENFANG_BASE_URL}${path}`, {
    headers: OPENFANG_API_KEY
      ? { Authorization: `Bearer ${OPENFANG_API_KEY}` }
      : {},
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${await response.text()}`);
  }

  return response.json();
}

async function loadTemplateManifestToml(templateName) {
  const template = await openfangRequest(`/api/templates/${encodeURIComponent(templateName)}`);
  if (!template.manifest_toml) {
    throw new Error(`Template '${templateName}' did not return manifest_toml`);
  }
  return template.manifest_toml;
}

function applyAgentName(manifestToml, agentName) {
  return manifestToml.replace(/^name\s*=\s*".*"$/m, `name = "${agentName}"`);
}

function stripCapabilitiesBlock(manifestToml) {
  return manifestToml.replace(/\n\[capabilities\][\s\S]*$/m, "");
}

function extractModelField(manifestToml, fieldName, fallback) {
  const match = manifestToml.match(
    new RegExp(`^${fieldName}\\s*=\\s*"([^"]+)"$`, "m"),
  );
  return match ? match[1] : fallback;
}

function makeChatOnlyManifest(manifestToml, agentName) {
  const renamed = applyAgentName(manifestToml, agentName);
  const provider = extractModelField(renamed, "provider", "default");
  const model = extractModelField(renamed, "model", "default");

  return [
    `name = "${agentName}"`,
    'version = "0.1.0"',
    'description = "Backend-owned product chat agent."',
    'author = "openfang-sdk-example"',
    'module = "builtin:chat"',
    "",
    "[model]",
    `provider = "${provider}"`,
    `model = "${model}"`,
    'max_tokens = 512',
    'temperature = 0.2',
    'system_prompt = "You are a concise product chat assistant. Reply directly. Follow exact format requests exactly. Do not use tools."',
    "",
    "[resources]",
    'max_llm_tokens_per_hour = 120000',
    "",
       "[metadata]",
       'skip_prompt_builder = true',
       "",
       "[capabilities]",
       "tools = []",
       "network = []",
       "memory_read = [\"*\"]",
       "memory_write = [\"self.*\"]",
  ].join("\n");
}

function sendJson(res, statusCode, body) {
  const payload = JSON.stringify(body);
  res.writeHead(statusCode, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(payload),
  });
  res.end(payload);
}

function readJson(req) {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      if (!body) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(body));
      } catch (error) {
        reject(new Error("Invalid JSON body"));
      }
    });
    req.on("error", reject);
  });
}

async function getOrCreateAgentId(userId) {
  const existing = agentIdsByUser.get(userId);
  if (existing) {
    return existing;
  }

  const agentName = `user-${userId}`;
  const runningAgents = await client.agents.list();
  const existingAgent = runningAgents.find((agent) => agent && agent.name === agentName);
  if (existingAgent && existingAgent.id) {
    agentIdsByUser.set(userId, existingAgent.id);
    return existingAgent.id;
  }

  const manifestToml = makeChatOnlyManifest(
    await loadTemplateManifestToml(OPENFANG_DEFAULT_TEMPLATE),
    agentName,
  );

  const created = await client.agents.create({
    manifest_toml: manifestToml,
  });
  agentIdsByUser.set(userId, created.id);
  return created.id;
}

async function handleChat(req, res) {
  const { userId, message, metadata } = await readJson(req);
  if (!userId || !message) {
    sendJson(res, 400, { error: "userId and message are required" });
    return;
  }

  const agentId = await getOrCreateAgentId(String(userId));
  const reply = await client.agents.message(agentId, String(message), metadata ? { metadata } : undefined);

  sendJson(res, 200, {
    agentId,
    reply,
  });
}

async function handleChatStream(req, res) {
  const { userId, message, metadata } = await readJson(req);
  if (!userId || !message) {
    sendJson(res, 400, { error: "userId and message are required" });
    return;
  }

  const agentId = await getOrCreateAgentId(String(userId));

  res.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-cache, no-transform",
    Connection: "keep-alive",
    "X-Accel-Buffering": "no",
  });

  res.write(`data: ${JSON.stringify({ type: "ready", agentId })}\n\n`);

  try {
    for await (const event of client.agents.stream(
      agentId,
      String(message),
      metadata ? { metadata } : undefined,
    )) {
      res.write(`data: ${JSON.stringify(event)}\n\n`);
    }
    res.write(`data: ${JSON.stringify({ type: "done" })}\n\n`);
  } catch (error) {
    res.write(
      `data: ${JSON.stringify({ type: "error", error: error.message || String(error) })}\n\n`,
    );
  } finally {
    res.end();
  }
}

const server = http.createServer(async (req, res) => {
  try {
    if (req.method === "GET" && req.url === "/api/health") {
      const health = await client.health();
      sendJson(res, 200, { backend: "ok", openfang: health });
      return;
    }

    if (req.method === "POST" && req.url === "/api/ai/chat") {
      await handleChat(req, res);
      return;
    }

    if (req.method === "POST" && req.url === "/api/ai/chat/stream") {
      await handleChatStream(req, res);
      return;
    }

    sendJson(res, 404, { error: "Not found" });
  } catch (error) {
    sendJson(res, 500, { error: error.message || String(error) });
  }
});

server.listen(PORT, () => {
  console.log(`Proxy backend listening on http://127.0.0.1:${PORT}`);
  console.log(`OpenFang base URL: ${OPENFANG_BASE_URL}`);
  console.log(`Default template: ${OPENFANG_DEFAULT_TEMPLATE}`);
});