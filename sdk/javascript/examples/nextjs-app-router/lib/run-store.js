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
 *   output: string|null,
 *   error: string|null,
 *   events: import('./alive-service').RunEvent[],
 *   startedAt: string,
 *   updatedAt: string,
 * }} RunRecord
 */

'use strict';

const { mkdir, readFile, writeFile } = require('node:fs/promises');
const { randomUUID } = require('node:crypto');
const path = require('node:path');

const DATA_DIR = path.join(process.cwd(), '.data');
const STORE_PATH = path.join(DATA_DIR, 'runs.json');

/** @type {Map<string, RunRecord>} */
const cache = new Map();

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

function scheduleWrite() {
  writeQueue = writeQueue.then(async () => {
    await ensureDir();
    const all = {};
    for (const [id, record] of cache.entries()) {
      all[id] = record;
    }
    await writeFile(STORE_PATH, JSON.stringify({ runs: all }, null, 2), 'utf8');
  }).catch(() => {}); // persist errors must never crash the caller
}

async function warmCache() {
  if (cache.size > 0) return;
  const persisted = await readAll();
  for (const [id, rec] of Object.entries(persisted)) {
    if (!cache.has(id)) cache.set(id, rec);
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

const runStore = {
  /**
   * Create a new run record.
   *
   * @param {{ sessionId: string, agent: string, input: string, parentRunId?: string }} opts
   * @returns {Promise<RunRecord>}
   */
  async create({ sessionId, agent, input, parentRunId = null }) {
    const now = new Date().toISOString();
    const record = /** @type {RunRecord} */ ({
      runId: randomUUID(),
      parentRunId,
      sessionId,
      agent,
      status: 'queued',
      input,
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
};

module.exports = { runStore };
