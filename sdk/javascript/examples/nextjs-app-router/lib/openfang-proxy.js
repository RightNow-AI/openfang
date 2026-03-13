const { randomUUID } = require("node:crypto");

const { OpenFang } = require("@openfang/sdk");

const { env } = require("./env");
const {
  appendConversationMessage,
  getSession,
  getRuntimeState,
  recordAgentBinding,
  updateConversationMessage,
  updateRuntimeState,
  upsertSession,
} = require("./session-store");

const client = new OpenFang(env.OPENFANG_BASE_URL, {
  headers: env.OPENFANG_API_KEY
    ? { Authorization: `Bearer ${env.OPENFANG_API_KEY}` }
    : {},
});

function authHeaders() {
  return env.OPENFANG_API_KEY
    ? { Authorization: `Bearer ${env.OPENFANG_API_KEY}` }
    : {};
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function isTimeoutError(error) {
  return /timed out/i.test(errorMessage(error));
}

function failureStatus(error) {
  return isTimeoutError(error) ? "timed_out" : "failed";
}

async function recordTurnSuccess(transport) {
  await updateRuntimeState({
    consecutive_failures: 0,
    last_error: null,
    last_failure_at: null,
    last_success_at: new Date().toISOString(),
    last_transport: transport,
  });
}

async function recordTurnFailure(error, transport) {
  const runtimeReadiness = await getRuntimeState();
  const nextError = errorMessage(error);

  await updateRuntimeState({
    consecutive_failures: Number(runtimeReadiness.consecutive_failures || 0) + 1,
    last_error: nextError,
    last_failure_at: new Date().toISOString(),
    last_transport: transport,
  });

  return {
    error: nextError,
    status: failureStatus(error),
  };
}

async function withTimeout(promise, label, timeoutMs = env.OPENFANG_TIMEOUT_MS) {
  let timeoutId;

  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timeoutId = setTimeout(() => {
          reject(new Error(`${label} timed out after ${timeoutMs}ms`));
        }, timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timeoutId);
  }
}

async function fetchWithTimeout(url, init = {}, timeoutMs = env.OPENFANG_TIMEOUT_MS) {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

  try {
    return await fetch(url, {
      ...init,
      signal: controller.signal,
      cache: "no-store",
    });
  } catch (error) {
    if (error?.name === "AbortError") {
      throw new Error(`Request timed out after ${timeoutMs}ms`);
    }

    throw error;
  } finally {
    clearTimeout(timeoutId);
  }
}

async function openfangRequest(path, init = {}, timeoutMs) {
  const response = await fetchWithTimeout(
    `${env.OPENFANG_BASE_URL}${path}`,
    {
      ...init,
      headers: {
        ...authHeaders(),
        ...Object.fromEntries(new Headers(init.headers || {}).entries()),
      },
    },
    timeoutMs,
  );

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${await response.text()}`);
  }

  return response;
}

async function openfangJson(path, init, timeoutMs) {
  const response = await openfangRequest(path, init, timeoutMs);
  return response.json();
}

async function loadTemplateManifestToml(templateName) {
  const template = await openfangJson(
    `/api/templates/${encodeURIComponent(templateName)}`,
  );

  if (!template.manifest_toml) {
    throw new Error(`Template '${templateName}' did not return manifest_toml`);
  }

  return template.manifest_toml;
}

function applyAgentName(manifestToml, agentName) {
  return manifestToml.replace(/^name\s*=\s*".*"$/m, `name = "${agentName}"`);
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
    'tools = []',
    'network = []',
    'memory_read = ["*"]',
    'memory_write = ["self.*"]',
  ].join("\n");
}

async function getOrCreateAgentForUser(userId) {
  const session = await getSession(userId);
  if (session?.agent_id) {
    return session.agent_id;
  }

  const agentName = `user-${userId}`;
  const runningAgents = await withTimeout(client.agents.list(), "Agent list");
  const existingAgent = runningAgents.find((agent) => agent?.name === agentName);

  if (existingAgent?.id) {
    await recordAgentBinding(userId, existingAgent.id);
    return existingAgent.id;
  }

  const manifestToml = makeChatOnlyManifest(
    await loadTemplateManifestToml(env.OPENFANG_DEFAULT_TEMPLATE),
    agentName,
  );
  const created = await withTimeout(
    client.agents.create({ manifest_toml: manifestToml }),
    "Agent creation",
  );
  await recordAgentBinding(userId, created.id);
  return created.id;
}

async function persistTurnStart({ userId, agentId, message, requestId, transport }) {
  const createdAt = new Date().toISOString();
  const userMessage = await appendConversationMessage({
    request_id: requestId,
    user_id: userId,
    agent_id: agentId,
    role: "user",
    content: message,
    status: "complete",
    error: null,
    transport,
    created_at: createdAt,
    updated_at: createdAt,
  });
  const assistantMessage = await appendConversationMessage({
    request_id: requestId,
    user_id: userId,
    agent_id: agentId,
    role: "assistant",
    content: "",
    status: "pending",
    error: null,
    transport,
    created_at: createdAt,
    updated_at: createdAt,
  });

  await upsertSession(userId, {
    agent_id: agentId,
    last_request_id: requestId,
    last_message_at: createdAt,
  });

  return { userMessage, assistantMessage };
}

async function ensureAssistantPlaceholder({
  assistantMessageId,
  userId,
  agentId,
  requestId,
  transport,
}) {
  if (assistantMessageId) {
    return updateConversationMessage(assistantMessageId, {
      agent_id: agentId,
      request_id: requestId,
      transport,
      status: "pending",
      error: null,
    });
  }

  return appendConversationMessage({
    request_id: requestId,
    user_id: userId,
    agent_id: agentId,
    role: "assistant",
    content: "",
    status: "pending",
    error: null,
    transport,
  });
}

async function finalizeAssistantMessage({
  assistantMessageId,
  content,
  status,
  error,
  transport,
  agentId,
  requestId,
}) {
  return updateConversationMessage(assistantMessageId, {
    agent_id: agentId,
    request_id: requestId,
    transport,
    content,
    status,
    error: error || null,
  });
}

async function getHealth() {
  const runtimeReadiness = await getRuntimeState();
  let openfangReachable = false;
  let chatReady = false;
  let lastError = runtimeReadiness.last_error;

  try {
    const openfang = await withTimeout(client.health(), "Health check", 5000);
    openfangReachable = openfang?.status === "ok";

    try {
      await loadTemplateManifestToml(env.OPENFANG_DEFAULT_TEMPLATE);
      chatReady = openfangReachable && !runtimeReadiness.last_error;
    } catch (templateError) {
      chatReady = false;
      lastError = errorMessage(templateError);
    }
  } catch (error) {
    openfangReachable = false;
    chatReady = false;
    lastError = errorMessage(error);
  }

  const status = !openfangReachable
    ? "offline"
    : chatReady
      ? "connected"
      : "degraded";

  return {
    status,
    openfangReachable,
    chatReady,
    lastError,
    timeoutMs: env.OPENFANG_TIMEOUT_MS,
  };
}

async function sendMessage(userId, message, metadata, options = {}) {
  const agentId = options.agentId || (await getOrCreateAgentForUser(userId));
  const requestId = options.requestId || randomUUID();
  const transport = options.transport || "chat";

  let assistantMessage = null;

  if (!options.skipUserMessageWrite) {
    const turn = await persistTurnStart({
      userId,
      agentId,
      message,
      requestId,
      transport,
    });
    assistantMessage = turn.assistantMessage;
  } else {
    assistantMessage = await ensureAssistantPlaceholder({
      assistantMessageId: options.assistantMessageId,
      userId,
      agentId,
      requestId,
      transport,
    });

    await upsertSession(userId, {
      agent_id: agentId,
      last_request_id: requestId,
      last_message_at: new Date().toISOString(),
    });
  }

  try {
    const reply = await withTimeout(
      client.agents.message(
        agentId,
        message,
        metadata ? { metadata } : undefined,
      ),
      "Assistant reply",
    );

    await finalizeAssistantMessage({
      assistantMessageId: assistantMessage.id,
      content: reply?.response || "",
      status: "complete",
      error: null,
      transport,
      agentId,
      requestId,
    });

    await recordTurnSuccess(transport);

    return {
      agentId,
      reply,
      requestId,
      assistantMessageId: assistantMessage.id,
      transport,
    };
  } catch (error) {
    const failure = await recordTurnFailure(error, transport);

    await finalizeAssistantMessage({
      assistantMessageId: assistantMessage.id,
      content: assistantMessage.content || "",
      status: failure.status,
      error: failure.error,
      transport,
      agentId,
      requestId,
    });

    throw error;
  }
}

async function streamMessage(userId, message, metadata) {
  const agentId = await getOrCreateAgentForUser(userId);
  const requestId = randomUUID();

  const upstream = await openfangRequest(
    `/api/agents/${encodeURIComponent(agentId)}/message/stream`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        message,
        ...(metadata ? { metadata } : {}),
      }),
    },
    env.OPENFANG_TIMEOUT_MS,
  );

  if (!upstream.body) {
    throw new Error("OpenFang stream response did not include a body");
  }

  const turn = await persistTurnStart({
    userId,
    agentId,
    message,
    requestId,
    transport: "stream",
  });

  return {
    agentId,
    requestId,
    body: upstream.body,
    userMessageId: turn.userMessage.id,
    assistantMessageId: turn.assistantMessage.id,
  };
}

module.exports = {
  failureStatus,
  getHealth,
  getOrCreateAgentForUser,
  recordTurnFailure,
  recordTurnSuccess,
  sendMessage,
  streamMessage,
};
