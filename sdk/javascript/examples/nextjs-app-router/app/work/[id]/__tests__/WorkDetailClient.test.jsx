import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

import WorkDetailClient from '../WorkDetailClient';

vi.mock('next/link', () => ({
  default: ({ href, children, ...props }) => <a href={href} {...props}>{children}</a>,
}));

vi.mock('../../../../lib/work-api', () => ({
  workApi: {
    getWorkById: vi.fn(),
    getWorkEvents: vi.fn(),
    getWork: vi.fn(),
    runWork: vi.fn(),
    approveWork: vi.fn(),
    rejectWork: vi.fn(),
    cancelWork: vi.fn(),
    retryWork: vi.fn(),
  },
}));

vi.mock('../../../../lib/planning-api', () => ({
  getPlanningData: vi.fn().mockResolvedValue(null),
  startPlanningRound: vi.fn(),
}));

vi.mock('../../../components/PlanningPanel', () => ({
  default: () => <div data-testid="planning-panel" />,
}));

describe('WorkDetailClient', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        artifacts: [
          {
            artifact_id: 'artifact-456',
            title: 'packet.json',
            kind: 'report_json',
            content_type: 'application/json',
            byte_size: 2048,
            created_at: '2025-03-02T12:00:00.000Z',
          },
        ],
      }),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('renders durable run artifacts using persisted workspace and run ids', async () => {
    render(
      <WorkDetailClient
        id="work-123"
        initialItem={{
          id: 'work-123',
          title: 'Write summary',
          description: 'Summarize findings',
          work_type: 'agent_task',
          source: 'api',
          status: 'completed',
          approval_status: 'not_required',
          assigned_agent_id: 'agent-1',
          assigned_agent_name: 'assistant',
          priority: 128,
          payload: {},
          tags: [],
          created_at: '2025-03-02T11:00:00.000Z',
          updated_at: '2025-03-02T11:00:00.000Z',
          retry_count: 0,
          max_retries: 0,
          run_id: 'run-123',
          workspace_id: 'workspace-123',
        }}
        initialEvents={[]}
        initialChildren={[]}
      />
    );

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        '/api/workspaces/workspace-123/runs/run-123/artifacts',
        { cache: 'no-store' }
      );
    });

    expect(await screen.findByText('Run artifacts')).toBeInTheDocument();
    expect(screen.getByText('packet.json')).toBeInTheDocument();
    expect(screen.getByText('report_json')).toBeInTheDocument();
    expect(screen.getByText('application/json')).toBeInTheDocument();
    expect(screen.getByText('2.0 KB')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Download' })).toHaveAttribute(
      'href',
      '/api/artifacts/artifact-456'
    );
  });
});