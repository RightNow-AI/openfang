import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('next/server', () => ({
  NextResponse: {
    json: (body, init = {}) => ({ body, status: init?.status ?? 200 }),
  },
}));

const originalCwd = process.cwd();

function makeRequest(body) {
  return { json: async () => body };
}

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

async function waitForTerminalStatus(runStore, runId) {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const run = await runStore.get(runId);
    if (run && ['completed', 'failed', 'cancelled'].includes(run.status)) {
      return run;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`Timed out waiting for run ${runId} to reach a terminal state.`);
}

describe('POST /api/runs founder ingestion route flow', () => {
  let tempRoot = null;

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(async () => {
    const { __resetTestDeps } = await import('../../../../lib/alive-service');
    __resetTestDeps();
    vi.resetModules();
    process.chdir(originalCwd);
    if (tempRoot) {
      await new Promise((resolve) => setTimeout(resolve, 20));
      await fs.rm(tempRoot, { recursive: true, force: true });
      tempRoot = null;
    }
  });

  it('creates founder tasks through POST /api/runs and returns them via the workspace task API', async () => {
    tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'openfang-founder-route-'));
    process.chdir(tempRoot);
    vi.resetModules();

    const { runStore } = await import('../../../../lib/run-store');
    const { taskStore } = await import('../../../../lib/task-store');
    const { __setTestDeps } = await import('../../../../lib/alive-service');
    const { POST } = await import('../route');
    const { GET: getFounderTasks } = await import('../../founder/workspaces/[workspaceId]/tasks/route');

    const workspace = await runStore.upsertFounderWorkspace({
      workspaceId: 'ws-founder-route-1',
      clientId: 'client-1',
      name: 'Acme Founder Workspace',
      companyName: 'Acme',
      idea: 'AI workflow copilot for operators',
      stage: 'validation',
    });

    __setTestDeps({
      runStore,
      taskStore,
      eventBus: {
        emit: vi.fn(),
        subscribe: vi.fn(() => () => {}),
        hasSubscribers: vi.fn(() => false),
        size: 0,
      },
      openfangClient: {
        listAgents: vi.fn().mockResolvedValue([
          { id: 'agt-founder', name: 'founder-advisor', title: 'founder-advisor' },
        ]),
        spawnAgentFromManifest: vi.fn(),
        streamMessage: vi.fn().mockImplementation(async (_agentId, _message, onEvent) => {
          await onEvent({
            event: 'chunk',
            data: {
              content: buildPlaybookResponse([
                '1. Interview 5 target customers this week',
                '2. Document the top 3 objections after each call',
              ].join('\n')),
            },
          });
          await onEvent({ event: 'done', data: {} });
        }),
      },
    });

    const response = await POST(makeRequest({
      sessionId: 'session-route-1',
      message: 'Validate this founder idea before we build.',
      playbookId: 'customer-discovery',
      workspaceId: workspace.workspaceId,
      clientId: workspace.clientId,
    }));

    expect(response.status).toBe(201);
    expect(response.body.status).toBe('queued');

    const parentRun = await waitForTerminalStatus(runStore, response.body.runId);
    const childRuns = await runStore.getChildren(response.body.runId);

    expect(parentRun.status).toBe('completed');
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('completed');

    const taskResponse = await getFounderTasks(null, {
      params: Promise.resolve({ workspaceId: workspace.workspaceId }),
    });

    expect(taskResponse.status).toBe(200);
    expect(taskResponse.body.tasks).toHaveLength(2);
    expect(taskResponse.body.tasks.every((task) => task.workspaceId === workspace.workspaceId)).toBe(true);
    expect(taskResponse.body.tasks.every((task) => task.runId === childRuns[0].runId)).toBe(true);
    expect(taskResponse.body.tasks.map((task) => task.description)).toEqual([
      'Interview 5 target customers this week',
      'Document the top 3 objections after each call',
    ]);
  });

  it('fails closed through POST /api/runs on malformed founder output and returns zero workspace tasks', async () => {
    tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'openfang-founder-route-'));
    process.chdir(tempRoot);
    vi.resetModules();

    const { runStore } = await import('../../../../lib/run-store');
    const { taskStore } = await import('../../../../lib/task-store');
    const { __setTestDeps } = await import('../../../../lib/alive-service');
    const { POST } = await import('../route');
    const { GET: getFounderTasks } = await import('../../founder/workspaces/[workspaceId]/tasks/route');

    const workspace = await runStore.upsertFounderWorkspace({
      workspaceId: 'ws-founder-route-2',
      clientId: 'client-2',
      name: 'Beta Founder Workspace',
      companyName: 'Beta',
      idea: 'AI operations analyst for founders',
      stage: 'validation',
    });

    __setTestDeps({
      runStore,
      taskStore,
      eventBus: {
        emit: vi.fn(),
        subscribe: vi.fn(() => () => {}),
        hasSubscribers: vi.fn(() => false),
        size: 0,
      },
      openfangClient: {
        listAgents: vi.fn().mockResolvedValue([
          { id: 'agt-founder', name: 'founder-advisor', title: 'founder-advisor' },
        ]),
        spawnAgentFromManifest: vi.fn(),
        streamMessage: vi.fn().mockImplementation(async (_agentId, _message, onEvent) => {
          await onEvent({
            event: 'chunk',
            data: {
              content: buildPlaybookResponse('{"description":"Interview 5 target customers"}'),
            },
          });
          await onEvent({ event: 'done', data: {} });
        }),
      },
    });

    const response = await POST(makeRequest({
      sessionId: 'session-route-2',
      message: 'Validate this founder idea before we build.',
      playbookId: 'customer-discovery',
      workspaceId: workspace.workspaceId,
      clientId: workspace.clientId,
    }));

    expect(response.status).toBe(201);
    expect(response.body.status).toBe('queued');

    const parentRun = await waitForTerminalStatus(runStore, response.body.runId);
    const childRuns = await runStore.getChildren(response.body.runId);

    expect(parentRun.status).toBe('failed');
    expect(parentRun.error).toMatch(/next_actions/i);
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('failed');

    const taskResponse = await getFounderTasks(null, {
      params: Promise.resolve({ workspaceId: workspace.workspaceId }),
    });

    expect(taskResponse.status).toBe(200);
    expect(taskResponse.body.tasks).toEqual([]);
  });

  it('fails closed through POST /api/runs when task persistence fails and returns zero workspace tasks', async () => {
    tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'openfang-founder-route-'));
    process.chdir(tempRoot);
    vi.resetModules();

    const { runStore } = await import('../../../../lib/run-store');
    const { taskStore } = await import('../../../../lib/task-store');
    const { __setTestDeps } = await import('../../../../lib/alive-service');
    const { POST } = await import('../route');
    const { GET: getFounderTasks } = await import('../../founder/workspaces/[workspaceId]/tasks/route');

    const workspace = await runStore.upsertFounderWorkspace({
      workspaceId: 'ws-founder-route-3',
      clientId: 'client-3',
      name: 'Gamma Founder Workspace',
      companyName: 'Gamma',
      idea: 'AI research assistant for new ventures',
      stage: 'validation',
    });

    vi.spyOn(taskStore, 'createTasksFromRun').mockRejectedValueOnce(new Error('Disk write failed'));

    __setTestDeps({
      runStore,
      taskStore,
      eventBus: {
        emit: vi.fn(),
        subscribe: vi.fn(() => () => {}),
        hasSubscribers: vi.fn(() => false),
        size: 0,
      },
      openfangClient: {
        listAgents: vi.fn().mockResolvedValue([
          { id: 'agt-founder', name: 'founder-advisor', title: 'founder-advisor' },
        ]),
        spawnAgentFromManifest: vi.fn(),
        streamMessage: vi.fn().mockImplementation(async (_agentId, _message, onEvent) => {
          await onEvent({
            event: 'chunk',
            data: {
              content: buildPlaybookResponse('1. Interview 5 target customers this week'),
            },
          });
          await onEvent({ event: 'done', data: {} });
        }),
      },
    });

    const response = await POST(makeRequest({
      sessionId: 'session-route-3',
      message: 'Validate this founder idea before we build.',
      playbookId: 'customer-discovery',
      workspaceId: workspace.workspaceId,
      clientId: workspace.clientId,
    }));

    expect(response.status).toBe(201);
    expect(response.body.status).toBe('queued');

    const parentRun = await waitForTerminalStatus(runStore, response.body.runId);
    const childRuns = await runStore.getChildren(response.body.runId);

    expect(parentRun.status).toBe('failed');
    expect(parentRun.error).toContain('Disk write failed');
    expect(childRuns).toHaveLength(1);
    expect(childRuns[0].status).toBe('failed');

    const taskResponse = await getFounderTasks(null, {
      params: Promise.resolve({ workspaceId: workspace.workspaceId }),
    });

    expect(taskResponse.status).toBe(200);
    expect(taskResponse.body.tasks).toEqual([]);
  });
});