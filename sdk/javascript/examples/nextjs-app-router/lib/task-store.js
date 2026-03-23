'use strict';

const { mkdir, readFile, writeFile } = require('node:fs/promises');
const { randomUUID } = require('node:crypto');
const path = require('node:path');

const DATA_DIR = path.join(process.cwd(), '.data');
const STORE_PATH = path.join(DATA_DIR, 'founder-tasks.json');
const ALLOWED_TASK_STATUSES = new Set(['pending', 'in_progress', 'completed', 'dismissed']);

/** @type {Map<string, FounderTaskRecord>} */
const taskCache = new Map();
let writeQueue = Promise.resolve();

/**
 * @typedef {'pending'|'in_progress'|'completed'|'dismissed'} FounderTaskStatus
 *
 * @typedef {{
 *   taskId: string,
 *   workspaceId: string,
 *   runId: string,
 *   description: string,
 *   category: string,
 *   status: FounderTaskStatus,
 *   createdAt: string,
 *   updatedAt: string,
 * }} FounderTaskRecord
 */

async function ensureDir() {
  await mkdir(DATA_DIR, { recursive: true });
}

async function readAll() {
  await ensureDir();
  try {
    const raw = await readFile(STORE_PATH, 'utf8');
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') {
      return {};
    }
    return parsed.tasks && typeof parsed.tasks === 'object' ? parsed.tasks : {};
  } catch {
    return {};
  }
}

function scheduleWrite() {
  writeQueue = writeQueue.then(async () => {
    await ensureDir();
    const all = {};
    for (const [taskId, record] of taskCache.entries()) {
      all[taskId] = record;
    }
    await writeFile(STORE_PATH, JSON.stringify({ tasks: all }, null, 2), 'utf8');
  }).catch(() => {});
}

async function warmCache() {
  if (taskCache.size > 0) return;
  const persisted = await readAll();
  for (const [taskId, record] of Object.entries(persisted)) {
    if (!taskCache.has(taskId)) taskCache.set(taskId, record);
  }
}

async function syncFromDisk() {
  const persisted = await readAll();
  for (const [taskId, record] of Object.entries(persisted)) {
    const cached = taskCache.get(taskId);
    if (!cached) {
      taskCache.set(taskId, record);
      continue;
    }

    const cachedUpdatedAt = Date.parse(cached.updatedAt || 0) || 0;
    const persistedUpdatedAt = Date.parse(record.updatedAt || 0) || 0;
    if (persistedUpdatedAt >= cachedUpdatedAt) {
      taskCache.set(taskId, record);
    }
  }
}

function normalizeCategory(value, fallback = 'general') {
  const normalized = String(value ?? '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
  return normalized || fallback;
}

function sanitizeDescription(value) {
  return String(value ?? '')
    .replace(/\s+/g, ' ')
    .trim();
}

function normalizeAction(action, fallbackCategory = 'general') {
  if (typeof action === 'string') {
    return {
      description: sanitizeDescription(action),
      category: fallbackCategory,
    };
  }

  if (!action || typeof action !== 'object' || Array.isArray(action)) {
    return {
      description: '',
      category: fallbackCategory,
    };
  }

  const rawDescription =
    action.description ?? action.title ?? action.action ?? action.text ?? action.value ?? '';
  const rawCategory = action.category ?? action.kind ?? action.type ?? fallbackCategory;

  return {
    description: sanitizeDescription(rawDescription),
    category: normalizeCategory(rawCategory, fallbackCategory),
  };
}

const taskStore = {
  /**
   * @param {{ workspaceId: string, runId: string, actions: Array<string|Record<string, unknown>>, defaultCategory?: string }} opts
   * @returns {Promise<FounderTaskRecord[]>}
   */
  async createTasksFromRun({ workspaceId, runId, actions, defaultCategory = 'general' }) {
    await warmCache();
    await syncFromDisk();

    const normalizedWorkspaceId = String(workspaceId ?? '').trim();
    const normalizedRunId = String(runId ?? '').trim();
    if (!normalizedWorkspaceId || !normalizedRunId || !Array.isArray(actions)) {
      throw new Error('Task creation blocked: missing required relational IDs or payload.');
    }

    const safeCategory = normalizeCategory(defaultCategory, 'general');
    const now = new Date().toISOString();
    const createdTasks = [];

    for (const action of actions) {
      const normalizedAction = normalizeAction(action, safeCategory);
      if (!normalizedAction.description) continue;

      const existing = [...taskCache.values()].find((task) =>
        task.workspaceId === normalizedWorkspaceId &&
        task.runId === normalizedRunId &&
        task.description.toLowerCase() === normalizedAction.description.toLowerCase(),
      );
      if (existing) {
        createdTasks.push(existing);
        continue;
      }

      const record = /** @type {FounderTaskRecord} */ ({
        taskId: randomUUID(),
        workspaceId: normalizedWorkspaceId,
        runId: normalizedRunId,
        description: normalizedAction.description,
        category: normalizedAction.category,
        status: 'pending',
        createdAt: now,
        updatedAt: now,
      });
      taskCache.set(record.taskId, record);
      createdTasks.push(record);
    }

    scheduleWrite();
    return createdTasks;
  },

  /**
   * @param {string} workspaceId
   * @returns {Promise<FounderTaskRecord[]>}
   */
  async getTasksByWorkspace(workspaceId) {
    await warmCache();
    await syncFromDisk();

    const normalizedWorkspaceId = String(workspaceId ?? '').trim();
    if (!normalizedWorkspaceId) {
      throw new Error('Query blocked: workspaceId is required.');
    }

    return [...taskCache.values()]
      .filter((task) => task.workspaceId === normalizedWorkspaceId)
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
  },

  /**
   * @param {string} workspaceId
   * @param {string} taskId
   * @param {string} newStatus
   * @returns {Promise<FounderTaskRecord>}
   */
  async updateTaskStatus(workspaceId, taskId, newStatus) {
    await warmCache();
    await syncFromDisk();

    const normalizedWorkspaceId = String(workspaceId ?? '').trim();
    const normalizedTaskId = String(taskId ?? '').trim();
    const normalizedStatus = String(newStatus ?? '').trim();

    if (!normalizedWorkspaceId || !normalizedTaskId) {
      throw new Error('Update blocked: workspaceId and taskId are required.');
    }
    if (!ALLOWED_TASK_STATUSES.has(normalizedStatus)) {
      throw new Error(`Update blocked: invalid status '${normalizedStatus}'.`);
    }

    const task = taskCache.get(normalizedTaskId);
    if (!task) {
      throw new Error('Task not found.');
    }
    if (task.workspaceId !== normalizedWorkspaceId) {
      throw new Error('Authorization blocked: task does not belong to this workspace.');
    }

    const updated = {
      ...task,
      status: normalizedStatus,
      updatedAt: new Date().toISOString(),
    };
    taskCache.set(updated.taskId, updated);
    scheduleWrite();
    return updated;
  },
};

module.exports = {
  ALLOWED_TASK_STATUSES: [...ALLOWED_TASK_STATUSES],
  taskStore,
};