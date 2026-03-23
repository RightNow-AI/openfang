/**
 * lib/run-store.js
 *
 * Durable record store for parent-child run trees.
 *
 * Runs are persisted to `.data/runs.json` (same directory as the session store).
 * An in-memory map is kept as a read-through cache so the SSE route can look
 * up runs without waiting for disk I/O on every event.
 *
 * Run shape:
 *   runId       — uuid
 *   parentRunId — uuid | null    (null = top-level alive run)
 *   sessionId   — string
 *   agent       — string         ('alive' for parents, specialist name for children)
 *   status      — 'queued' | 'running' | 'completed' | 'failed' | 'cancelled'
 *   input       — string
 *   playbookId  — string | null
 *   workspaceId — string | null
 *   context     — object | null
 *   output      — string | null
 *   error       — string | null
 *   events      — RunEvent[]     (full replay buffer)
 *   startedAt   — ISO string
 *   updatedAt   — ISO string
 *
 * @typedef {'queued'|'running'|'completed'|'failed'|'cancelled'} RunStatus
 *
 * @typedef {{
 *   runId: string,
 *   parentRunId: string|null,
 *   sessionId: string,
 *   agent: string,
 *   status: RunStatus,
 *   input: string,
 *   playbookId: string|null,
 *   workspaceId: string|null,
 *   context: Record<string, unknown>|null,
 *   output: string|null,
 *   error: string|null,
 *   events: import('./alive-service').RunEvent[],
 *   startedAt: string,
 *   updatedAt: string,
 * }} RunRecord
 *
 * @typedef {{
 *   workspaceId: string,
 *   clientId: string,
 *   name: string,
 *   companyName: string,
 *   idea: string,
 *   stage: string,
 *   playbookDefaults: Record<string, unknown>|null,
 *   createdAt: string,
 *   updatedAt: string,
 * }} FounderWorkspaceRecord
 *
 * @typedef {{
 *   runId: string,
 *   workspaceId: string,
 *   playbookId: string|null,
 *   prompt: string,
 *   status: RunStatus,
 *   summary: string,
 *   citations: string[],
 *   nextActions: string[],
 *   createdAt: string,
 *   updatedAt: string,
 * }} FounderRunRecord
 */

'use strict';

const { mkdir, readFile, writeFile } = require('node:fs/promises');
const { randomUUID } = require('node:crypto');
const path = require('node:path');

const DATA_DIR = path.join(process.cwd(), '.data');
const STORE_PATH = path.join(DATA_DIR, 'runs.json');
const FOUNDER_STORE_PATH = path.join(DATA_DIR, 'founder.json');

/** @type {Map<string, RunRecord>} */
const cache = new Map();
/** @type {Map<string, FounderWorkspaceRecord>} */
const founderWorkspaceCache = new Map();
/** @type {Map<string, FounderRunRecord>} */
const founderRunCache = new Map();

let writeQueue = Promise.resolve();

async function ensureDir() {
  await mkdir(DATA_DIR, { recursive: true });
}

async function readAll() {
  await ensureDir();
  try {
    const raw = await readFile(STORE_PATH, 'utf8');
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return {};
    return typeof parsed.runs === 'object' ? parsed.runs : {};
  } catch {
    return {};
  }
}

async function readFounderAll() {
  await ensureDir();
  try {
    const raw = await readFile(FOUNDER_STORE_PATH, 'utf8');
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') {
      return { workspaces: {}, runs: {} };
    }
    return {
      workspaces: parsed.workspaces && typeof parsed.workspaces === 'object' ? parsed.workspaces : {},
      runs: parsed.runs && typeof parsed.runs === 'object' ? parsed.runs : {},
    };
  } catch {
    return { workspaces: {}, runs: {} };
  }
}

function founderRunKey(workspaceId, runId) {
  return `${workspaceId}:${runId}`;
}

function scheduleWrite() {
  writeQueue = writeQueue.then(async () => {
    await ensureDir();
    const all = {};
    for (const [id, record] of cache.entries()) {
      all[id] = record;
    }
    await writeFile(STORE_PATH, JSON.stringify({ runs: all }, null, 2), 'utf8');

    const founderWorkspaces = {};
    for (const [id, record] of founderWorkspaceCache.entries()) {
      founderWorkspaces[id] = record;
    }

    const founderRuns = {};
    for (const [id, record] of founderRunCache.entries()) {
      founderRuns[id] = record;
    }

    await writeFile(
      FOUNDER_STORE_PATH,
      JSON.stringify({ workspaces: founderWorkspaces, runs: founderRuns }, null, 2),
      'utf8',
    );
  }).catch(() => {}); // persist errors must never crash the caller
}

async function warmCache() {
  if (cache.size > 0) return;
  const persisted = await readAll();
  for (const [id, rec] of Object.entries(persisted)) {
    if (!cache.has(id)) cache.set(id, rec);
  }
}

async function warmFounderCache() {
  if (founderWorkspaceCache.size > 0 || founderRunCache.size > 0) return;
  const persisted = await readFounderAll();
  for (const [id, rec] of Object.entries(persisted.workspaces)) {
    if (!founderWorkspaceCache.has(id)) founderWorkspaceCache.set(id, rec);
  }
  for (const [id, rec] of Object.entries(persisted.runs)) {
    if (!founderRunCache.has(id)) founderRunCache.set(id, rec);
  }
}

async function syncFromDisk() {
  const persisted = await readAll();
  for (const [id, rec] of Object.entries(persisted)) {
    const cached = cache.get(id);
    if (!cached) {
      cache.set(id, rec);
      continue;
    }

    const cachedUpdatedAt = Date.parse(cached.updatedAt || 0) || 0;
    const persistedUpdatedAt = Date.parse(rec.updatedAt || 0) || 0;
    if (persistedUpdatedAt >= cachedUpdatedAt) {
      cache.set(id, rec);
    }
  }
}

async function syncFounderFromDisk() {
  const persisted = await readFounderAll();

  for (const [id, rec] of Object.entries(persisted.workspaces)) {
    const cached = founderWorkspaceCache.get(id);
    if (!cached) {
      founderWorkspaceCache.set(id, rec);
      continue;
    }

    const cachedUpdatedAt = Date.parse(cached.updatedAt || 0) || 0;
    const persistedUpdatedAt = Date.parse(rec.updatedAt || 0) || 0;
    if (persistedUpdatedAt >= cachedUpdatedAt) {
      founderWorkspaceCache.set(id, rec);
    }
  }

  for (const [id, rec] of Object.entries(persisted.runs)) {
    const cached = founderRunCache.get(id);
    if (!cached) {
      founderRunCache.set(id, rec);
      continue;
    }

    const cachedUpdatedAt = Date.parse(cached.updatedAt || 0) || 0;
    const persistedUpdatedAt = Date.parse(rec.updatedAt || 0) || 0;
    if (persistedUpdatedAt >= cachedUpdatedAt) {
      founderRunCache.set(id, rec);
    }
  }
}

const runStore = {
  /**
   * Create a new run record.
   *
   * @param {{ sessionId: string, agent: string, input: string, parentRunId?: string, playbookId?: string|null, workspaceId?: string|null, context?: Record<string, unknown>|null }} opts
   * @returns {Promise<RunRecord>}
   */
  async create({ sessionId, agent, input, parentRunId = null, playbookId = null, workspaceId = null, context = null }) {
    const now = new Date().toISOString();
    const record = /** @type {RunRecord} */ ({
      runId: randomUUID(),
      parentRunId,
      sessionId,
      agent,
      status: 'queued',
      input,
      playbookId,
      workspaceId,
      context,
      output: null,
      error: null,
      events: [],
      startedAt: now,
      updatedAt: now,
    });
    cache.set(record.runId, record);
    scheduleWrite();
    return record;
  },

  /**
   * Append a normalized event to the run's replay buffer.
   * Does not change status.
   *
   * @param {string} runId
   * @param {import('./alive-service').RunEvent} event
   */
  appendEvent(runId, event) {
    const record = cache.get(runId);
    if (!record) return;
    record.events.push(event);
    record.updatedAt = new Date().toISOString();
    scheduleWrite();
  },

  /**
   * Update a run's status (and optionally output/error).
   *
   * @param {string} runId
   * @param {RunStatus} status
   * @param {{ output?: string, error?: string }} [extras]
   */
  setStatus(runId, status, extras = {}) {
    const record = cache.get(runId);
    if (!record) return;
    record.status = status;
    if (extras.output !== undefined) record.output = extras.output;
    if (extras.error !== undefined) record.error = extras.error;
    record.updatedAt = new Date().toISOString();
    scheduleWrite();
  },

  /**
   * Update only the run output without changing status.
   *
   * @param {string} runId
   * @param {string|null} output
   */
  setOutput(runId, output) {
    const record = cache.get(runId);
    if (!record) return;
    record.output = output;
    record.updatedAt = new Date().toISOString();
    scheduleWrite();
  },

  /**
   * Get a run by ID. Tries cache first, falls back to disk on miss
   * (handles the case where the process restarted).
   *
   * @param {string} runId
   * @returns {Promise<RunRecord|null>}
   */
  async get(runId) {
    await warmCache();
    await syncFromDisk();
    return cache.get(runId) ?? null;
  },

  /**
   * Synchronous lookup from the in-memory cache only.
   * Returns null if not in cache.
   *
   * @param {string} runId
   * @returns {RunRecord|null}
   */
  getSync(runId) {
    return cache.get(runId) ?? null;
  },

  /**
   * List the most recent N top-level runs (parentRunId === null).
   *
   * @param {number} [limit=50]
   * @returns {Promise<RunRecord[]>}
   */
  async listRecent(limit = 50) {
    await warmCache();
    await syncFromDisk();
    const all = [...cache.values()];
    return all
      .filter((r) => r.parentRunId === null)
      .sort((a, b) => b.startedAt.localeCompare(a.startedAt))
      .slice(0, limit);
  },

  /**
   * Get all child runs for a parent run.
   *
   * @param {string} parentRunId
   * @returns {Promise<RunRecord[]>}
   */
  async getChildren(parentRunId) {
    await warmCache();
    await syncFromDisk();
    return [...cache.values()].filter((r) => r.parentRunId === parentRunId);
  },

  /**
   * Create or update a founder workspace.
   *
   * @param {{ workspaceId?: string, clientId: string, name?: string, companyName: string, idea?: string, stage?: string, playbookDefaults?: Record<string, unknown>|null }} opts
   * @returns {Promise<FounderWorkspaceRecord>}
   */
  async upsertFounderWorkspace({
    workspaceId = randomUUID(),
    clientId,
    name = '',
    companyName,
    idea = '',
    stage = '',
    playbookDefaults = null,
  }) {
    await warmFounderCache();
    await syncFounderFromDisk();

    const existing = founderWorkspaceCache.get(workspaceId);
    const now = new Date().toISOString();
    const record = /** @type {FounderWorkspaceRecord} */ ({
      workspaceId,
      clientId,
      name: String(name || `${companyName} Founder Workspace`).trim(),
      companyName: String(companyName ?? '').trim(),
      idea: String(idea ?? '').trim(),
      stage: String(stage ?? '').trim(),
      playbookDefaults: playbookDefaults && typeof playbookDefaults === 'object' && !Array.isArray(playbookDefaults)
        ? playbookDefaults
        : null,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    });

    founderWorkspaceCache.set(workspaceId, record);
    scheduleWrite();
    return record;
  },

  /**
   * @param {string} workspaceId
   * @returns {Promise<FounderWorkspaceRecord|null>}
   */
  async getFounderWorkspace(workspaceId) {
    await warmFounderCache();
    await syncFounderFromDisk();
    return founderWorkspaceCache.get(workspaceId) ?? null;
  },

  /**
   * @param {{ clientId?: string|null }} [opts]
   * @returns {Promise<FounderWorkspaceRecord[]>}
   */
  async listFounderWorkspaces(opts = {}) {
    await warmFounderCache();
    await syncFounderFromDisk();
    const clientId = opts.clientId ? String(opts.clientId).trim() : null;
    return [...founderWorkspaceCache.values()]
      .filter((workspace) => !clientId || workspace.clientId === clientId)
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  },

  /**
   * @param {{ runId: string, workspaceId: string, playbookId?: string|null, prompt?: string, status?: RunStatus, summary?: string, citations?: string[], nextActions?: string[] }} opts
   * @returns {Promise<FounderRunRecord>}
   */
  async saveFounderRun({
    runId,
    workspaceId,
    playbookId = null,
    prompt = '',
    status = 'completed',
    summary = '',
    citations = [],
    nextActions = [],
  }) {
    await warmFounderCache();
    await syncFounderFromDisk();

    const key = founderRunKey(workspaceId, runId);
    const existing = founderRunCache.get(key);
    const now = new Date().toISOString();
    const record = /** @type {FounderRunRecord} */ ({
      runId,
      workspaceId,
      playbookId,
      prompt: String(prompt ?? '').trim(),
      status,
      summary: String(summary ?? '').trim(),
      citations: Array.isArray(citations) ? citations.map((item) => String(item).trim()).filter(Boolean) : [],
      nextActions: Array.isArray(nextActions) ? nextActions.map((item) => String(item).trim()).filter(Boolean) : [],
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    });

    founderRunCache.set(key, record);
    scheduleWrite();
    return record;
  },

  /**
   * @param {string} workspaceId
   * @param {string} runId
   * @returns {Promise<FounderRunRecord|null>}
   */
  async getFounderRun(workspaceId, runId) {
    await warmFounderCache();
    await syncFounderFromDisk();
    return founderRunCache.get(founderRunKey(workspaceId, runId)) ?? null;
  },

  /**
   * @param {string} workspaceId
   * @returns {Promise<FounderRunRecord[]>}
   */
  async listFounderRunsByWorkspace(workspaceId) {
    await warmFounderCache();
    await syncFounderFromDisk();
    return [...founderRunCache.values()]
      .filter((run) => run.workspaceId === workspaceId)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  },
};

module.exports = { runStore };
