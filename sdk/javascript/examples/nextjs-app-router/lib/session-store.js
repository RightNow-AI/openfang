const { mkdir, readFile, writeFile } = require("node:fs/promises");
const { randomUUID } = require("node:crypto");
const path = require("node:path");

const DATA_DIR = path.join(process.cwd(), ".data");
const STORE_PATH = path.join(DATA_DIR, "openfang-sessions.json");
const VALID_MESSAGE_STATUSES = new Set([
  "pending",
  "complete",
  "failed",
  "timed_out",
]);
const LEGACY_MESSAGE_STATUS_MAP = {
  completed: "complete",
  streaming: "pending",
  fallback: "complete",
  partial: "failed",
};
const VALID_TRANSPORTS = new Set(["stream", "chat"]);

let writeQueue = Promise.resolve();

function emptyStore() {
  return {
    version: 3,
    users: {},
    agent_sessions: {},
    runtime_state: {
      consecutive_failures: 0,
      last_error: null,
      last_failure_at: null,
      last_success_at: null,
      last_transport: null,
    },
    conversation_messages: [],
  };
}

function normalizeUserId(userId) {
  return String(userId || "").trim();
}

function normalizeTimestamp(value, fallback = new Date().toISOString()) {
  return String(value || "").trim() || fallback;
}

function normalizeRole(role) {
  return role === "assistant" ? "assistant" : "user";
}

function normalizeMessageStatus(status, fallback = "complete") {
  const normalized = String(status || "").trim().toLowerCase();
  const mapped = LEGACY_MESSAGE_STATUS_MAP[normalized] || normalized;
  return VALID_MESSAGE_STATUSES.has(mapped) ? mapped : fallback;
}

function normalizeTransport(transport, fallback = "chat") {
  const normalized = String(transport || "").trim().toLowerCase();
  return VALID_TRANSPORTS.has(normalized) ? normalized : fallback;
}

function normalizeConversationMessage(entry = {}) {
  const role = normalizeRole(entry.role);
  const createdAt = normalizeTimestamp(entry.created_at);
  const requestId = String(entry.request_id || "").trim() || randomUUID();
  return {
    id: String(entry.id || "").trim() || randomUUID(),
    user_id: normalizeUserId(entry.user_id),
    agent_id: String(entry.agent_id || "").trim() || null,
    role,
    content: String(entry.content || ""),
    status: normalizeMessageStatus(
      entry.status,
      role === "assistant" ? "pending" : "complete",
    ),
    error: entry.error == null ? null : String(entry.error),
    request_id: requestId,
    transport: normalizeTransport(entry.transport, "chat"),
    created_at: createdAt,
    updated_at: normalizeTimestamp(entry.updated_at, createdAt),
  };
}

function normalizeRuntimeState(entry = {}) {
  return {
    consecutive_failures: Math.max(0, Number(entry.consecutive_failures || 0)),
    last_error: entry.last_error == null ? null : String(entry.last_error),
    last_failure_at: entry.last_failure_at
      ? normalizeTimestamp(entry.last_failure_at)
      : null,
    last_success_at: entry.last_success_at
      ? normalizeTimestamp(entry.last_success_at)
      : null,
    last_transport: entry.last_transport == null ? null : String(entry.last_transport),
  };
}

async function ensureDataDir() {
  await mkdir(DATA_DIR, { recursive: true });
}

async function readStore() {
  await ensureDataDir();

  try {
    const raw = await readFile(STORE_PATH, "utf8");
    const parsed = JSON.parse(raw);

    if (!parsed || typeof parsed !== "object") {
      return emptyStore();
    }

    return {
      version: Number(parsed.version || 3),
      users: typeof parsed.users === "object" && parsed.users ? parsed.users : {},
      agent_sessions:
        typeof parsed.agent_sessions === "object" && parsed.agent_sessions
          ? parsed.agent_sessions
          : {},
      runtime_state: normalizeRuntimeState(parsed.runtime_state),
      conversation_messages: Array.isArray(parsed.conversation_messages)
        ? parsed.conversation_messages
            .map((entry) => normalizeConversationMessage(entry))
            .filter((entry) => entry.user_id)
        : [],
    };
  } catch (error) {
    if (error && typeof error === "object" && error.code === "ENOENT") {
      return emptyStore();
    }

    throw error;
  }
}

async function writeStore(store) {
  await ensureDataDir();
  await writeFile(STORE_PATH, `${JSON.stringify(store, null, 2)}\n`, "utf8");
}

function runExclusive(operation) {
  const nextWrite = writeQueue.then(operation);
  writeQueue = nextWrite.catch(() => {});
  return nextWrite;
}

async function getSession(userId) {
  const normalizedUserId = normalizeUserId(userId);
  if (!normalizedUserId) {
    throw new Error("userId is required");
  }

  const store = await readStore();
  return store.agent_sessions[normalizedUserId] || null;
}

async function upsertUser(user) {
  const normalizedUserId = normalizeUserId(user?.user_id);
  if (!normalizedUserId) {
    throw new Error("user_id is required");
  }

  return runExclusive(async () => {
    const store = await readStore();
    const now = new Date().toISOString();
    const existing = store.users[normalizedUserId] || {
      user_id: normalizedUserId,
      created_at: now,
    };

    const nextUser = {
      ...existing,
      ...user,
      user_id: normalizedUserId,
      updated_at: now,
    };

    store.users[normalizedUserId] = nextUser;
    await writeStore(store);
    return nextUser;
  });
}

async function upsertSession(userId, updates = {}) {
  const normalizedUserId = normalizeUserId(userId);
  if (!normalizedUserId) {
    throw new Error("userId is required");
  }

  return runExclusive(async () => {
    const store = await readStore();
    const now = new Date().toISOString();
    const existing = store.agent_sessions[normalizedUserId] || {
      user_id: normalizedUserId,
      created_at: now,
    };

    const nextSession = {
      ...existing,
      ...updates,
      user_id: normalizedUserId,
      updated_at: now,
    };

    store.agent_sessions[normalizedUserId] = nextSession;
    await writeStore(store);
    return nextSession;
  });
}

async function recordAgentBinding(userId, agentId) {
  return upsertSession(userId, {
    agent_id: agentId,
    last_bound_at: new Date().toISOString(),
  });
}

async function appendConversationMessage(entry) {
  const userId = normalizeUserId(entry?.user_id);
  if (!userId) {
    throw new Error("user_id is required");
  }

  return runExclusive(async () => {
    const store = await readStore();
    const record = normalizeConversationMessage({
      ...entry,
      user_id: userId,
    });

    store.conversation_messages.push(record);
    await writeStore(store);
    return record;
  });
}

async function updateConversationMessage(messageId, updates = {}) {
  const normalizedMessageId = String(messageId || "").trim();
  if (!normalizedMessageId) {
    throw new Error("messageId is required");
  }

  return runExclusive(async () => {
    const store = await readStore();
    const index = store.conversation_messages.findIndex(
      (message) => message.id === normalizedMessageId,
    );

    if (index === -1) {
      throw new Error(`Conversation message '${normalizedMessageId}' was not found`);
    }

    const existing = store.conversation_messages[index];
    const nextMessage = normalizeConversationMessage({
      ...existing,
      ...updates,
      id: existing.id,
      user_id: existing.user_id,
      role: existing.role,
      created_at: existing.created_at,
      updated_at: new Date().toISOString(),
    });

    store.conversation_messages[index] = nextMessage;
    await writeStore(store);
    return nextMessage;
  });
}

async function listConversationMessages(userId, options = {}) {
  const normalizedUserId = normalizeUserId(userId);
  if (!normalizedUserId) {
    throw new Error("userId is required");
  }

  const limit = Math.max(1, Math.min(Number(options.limit || 20), 100));
  const store = await readStore();

  return store.conversation_messages
    .filter((message) => message.user_id === normalizedUserId)
    .sort((left, right) => {
      const createdOrder = left.created_at.localeCompare(right.created_at);
      return createdOrder || left.updated_at.localeCompare(right.updated_at);
    })
    .slice(-limit);
}

async function getRuntimeState() {
  const store = await readStore();
  return store.runtime_state;
}

async function updateRuntimeState(updates = {}) {
  return runExclusive(async () => {
    const store = await readStore();
    const nextState = normalizeRuntimeState({
      ...store.runtime_state,
      ...updates,
    });

    store.runtime_state = nextState;
    await writeStore(store);
    return nextState;
  });
}

module.exports = {
  appendConversationMessage,
  getSession,
  getRuntimeState,
  listConversationMessages,
  recordAgentBinding,
  updateConversationMessage,
  updateRuntimeState,
  upsertSession,
  upsertUser,
};