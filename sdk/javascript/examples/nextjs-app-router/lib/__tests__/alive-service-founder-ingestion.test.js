import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const originalCwd = process.cwd();

function buildPlaybookResponse(nextActionsBlock) {
  return [
    '# Customer Discovery Report',
    '',
    '## Core Assumptions',
    '- Founders have a real customer pain point to validate.',
    '',
    '## Interview Script',
    '1. Ask about the current workflow.',
    '',
    '## Watering Holes',
    '- Founder communities',
    '',
    '## Validation Metric',
    '- 5 qualified interviews booked',
    '',
    '## Anti Patterns',
    '- Building before talking to users',
    '',
    '## Citations',
    '- https://example.com/mom-test',
    '',
    '## Next Actions',
    nextActionsBlock,
  ].join('\n');
}

async function bootstrapHarness({ responseText }) {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'openfang-founder-ingestion-'));
  process.chdir(tempRoot);
  vi.resetModules();

  const runRecords = new Map();
  const workspaceRecords = new Map();

  const runStore = {
    create: vi.fn(async ({ sessionId, agent, input, parentRunId = null, playbookId = null, workspaceId = null, context = null }) => {
      const now = new Date().toISOString();
      const record = {
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
      };
      runRecords.set(record.runId, record);
      return record;
    }),
    appendEvent: vi.fn((runId, event) => {
      const record = runRecords.get(runId);
      if (!record) return;
      record.events.push(event);
      record.updatedAt = new Date().toISOString();
    }),
    setStatus: vi.fn((runId, status, extras = {}) => {
      const record = runRecords.get(runId);
      if (!record) return;
      record.status = status;
      if (extras.output !== undefined) record.output = extras.output;
      if (extras.error !== undefined) record.error = extras.error;
      record.updatedAt = new Date().toISOString();
    }),
    setOutput: vi.fn((runId, output) => {
      const record = runRecords.get(runId);
      if (!record) return;
      record.output = output;
      record.updatedAt = new Date().toISOString();
    }),
    get: vi.fn(async (runId) => runRecords.get(runId) ?? null),
    getChildren: vi.fn(async (parentRunId) => [...runRecords.values()].filter((record) => record.parentRunId === parentRunId)),
    upsertFounderWorkspace: vi.fn(async ({ workspaceId = randomUUID(), clientId, name = '', companyName, idea = '', stage = '', playbookDefaults = null }) => {
      const now = new Date().toISOString();
      const existing = workspaceRecords.get(workspaceId);
      const record = {
        workspaceId,
        clientId,
        name: String(name || `${companyName} Founder Workspace`).trim(),
        companyName: String(companyName ?? '').trim(),
        idea: String(idea ?? '').trim(),
        stage: String(stage ?? '').trim(),
        playbookDefaults,
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
      };
      workspaceRecords.set(workspaceId, record);
      return record;
    }),
    getFounderWorkspace: vi.fn(async (workspaceId) => workspaceRecords.get(workspaceId) ?? null),
  };

  const listAgents = vi.fn().mockResolvedValue([
    { id: 'agt-founder', name: 'founder-advisor', title: 'founder-advisor' },
  ]);
  const streamMessage = vi.fn().mockImplementation(async (_agentId, _message, onEvent) => {
    await onEvent({ event: 'chunk', data: { content: responseText } });
    await onEvent({ event: 'done', data: {} });
  });

  const openfangClient = {
    listAgents,
    streamMessage,
    spawnAgentFromManifest: vi.fn(),
  };
  const eventBus = {
    emit: vi.fn(),
    subscribe: vi.fn(() => () => {}),
    hasSubscribers: vi.fn(() => false),
    size: 0,
  };

  const { aliveService, __setTestDeps, __resetTestDeps } = await import('../alive-service');
  const { taskStore } = await import('../task-store');

  __setTestDeps({ runStore, openfangClient, eventBus, taskStore });

  const workspace = await runStore.upsertFounderWorkspace({
    workspaceId: 'ws-founder-1',
    clientId: 'client-1',
    name: 'Acme Founder Workspace',
    companyName: 'Acme',
    idea: 'AI workflow copilot for operators',
    stage: 'validation',
  });

  return {
    tempRoot,
    aliveService,
    __resetTestDeps,
    runStore,
    taskStore,
    workspace,
    listAgents,
    streamMessage,
  };
}

async function executePlaybookRun(harness, { sessionId }) {
  const parentRun = await harness.runStore.create({
    sessionId,
    agent: 'alive',
    input: 'Validate this founder idea before we build.',
    playbookId: 'customer-discovery',
    workspaceId: harness.workspace.workspaceId,
  });

  await harness.aliveService._execute(
    parentRun,
    sessionId,
    'Validate this founder idea before we build.',
    'customer-discovery',
    harness.workspace.workspaceId,
    null,
  );

  const persistedParent = await harness.runStore.get(parentRun.runId);
  const childRuns = await harness.runStore.getChildren(parentRun.runId);
  return { parentRun: persistedParent, childRuns };
}

describe('aliveService founder task ingestion', () => {
  let tempRoot = null;

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(async () => {
    const { __resetTestDeps } = await import('../alive-service');
    __resetTestDeps();
    vi.resetModules();
    process.chdir(originalCwd);
    if (tempRoot) {
      await new Promise((resolve) => setTimeout(resolve, 20));
      await fs.rm(tempRoot, { recursive: true, force: true });
      tempRoot = null;
    }
  });

  it('persists founder tasks and completes the run on a valid playbook response', async () => {
    const harness = await bootstrapHarness({
      responseText: buildPlaybookResponse([
        '1. Interview 5 target customers this week',
        '2. Document the top 3 objections after each call',
      ].join('\n')),
    });
    tempRoot = harness.tempRoot;

    const { parentRun, childRuns } = await executePlaybookRun(harness, { sessionId: 'session-1' });

    expect(parentRun.status).toBe('completed');
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('completed');

    const tasks = await harness.taskStore.getTasksByWorkspace(harness.workspace.workspaceId);
    expect(tasks).toHaveLength(2);
    expect(tasks.every((task) => task.workspaceId === harness.workspace.workspaceId)).toBe(true);
    expect(tasks.every((task) => task.runId === childRuns[0].runId)).toBe(true);
    expect(tasks.map((task) => task.description)).toEqual([
      'Interview 5 target customers this week',
      'Document the top 3 objections after each call',
    ]);
  });

  it('fails closed when task ingestion throws and does not persist orphaned tasks', async () => {
    const harness = await bootstrapHarness({
      responseText: buildPlaybookResponse('1. Interview 5 target customers this week'),
    });
    tempRoot = harness.tempRoot;

    vi.spyOn(harness.taskStore, 'createTasksFromRun').mockRejectedValueOnce(new Error('Disk write failed'));

    const { parentRun, childRuns } = await executePlaybookRun(harness, { sessionId: 'session-2' });

    expect(parentRun.status).toBe('failed');
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('failed');
    expect(parentRun.error).toContain('Disk write failed');

    const tasks = await harness.taskStore.getTasksByWorkspace(harness.workspace.workspaceId);
    expect(tasks).toHaveLength(0);
  });

  it('fails closed on malformed next_actions content and writes zero tasks', async () => {
    const harness = await bootstrapHarness({
      responseText: buildPlaybookResponse('{"description":"Interview 5 target customers"}'),
    });
    tempRoot = harness.tempRoot;

    const { parentRun, childRuns } = await executePlaybookRun(harness, { sessionId: 'session-3' });

    expect(parentRun.status).toBe('failed');
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('failed');
    expect(parentRun.error).toMatch(/next_actions/i);

    const tasks = await harness.taskStore.getTasksByWorkspace(harness.workspace.workspaceId);
    expect(tasks).toHaveLength(0);
  });
});