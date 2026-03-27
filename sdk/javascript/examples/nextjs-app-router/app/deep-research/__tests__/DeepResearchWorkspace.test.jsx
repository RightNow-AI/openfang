import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

vi.mock('../ResearchCitationsPanel', () => ({
  default: () => <div data-testid="citations-panel" />,
}));

vi.mock('../ResearchDeliverablePanel', () => ({
  default: () => <div data-testid="deliverable-panel" />,
}));

vi.mock('../ResearchMarkdownBlock', () => ({
  default: ({ text }) => <div data-testid="markdown-block">{text}</div>,
}));

vi.mock('../ResearchNextActionsCard', () => ({
  default: () => <div data-testid="next-actions-card" />,
}));

vi.mock('../ResearchStatusCard', () => ({
  default: () => <div data-testid="status-card" />,
}));

import DeepResearchWorkspace from '../DeepResearchWorkspace';

function renderWorkspace(overrides = {}) {
  const props = {
    phase: 'done',
    stageIdx: -1,
    stages: [],
    runId: null,
    query: 'Map the durable artifacts emitted by this research run.',
    founderWorkspace: null,
    autoScrollPinned: true,
    trackingMode: 'live',
    reportPaneRef: { current: null },
    onReportPaneScroll: vi.fn(),
    hasStreamingReport: false,
    rawReply: '## Findings\nDurable artifacts are attached to the completed run.',
    errMsg: '',
    runSnapshot: null,
    onCheckLatestRun: vi.fn(),
    onReset: vi.fn(),
    report: {
      lead: 'Lead summary',
      findings: '## Findings\nDurable artifacts are attached to the completed run.',
      confidence: 'High confidence',
      sourceUrls: [],
      citationUrls: [],
      citations: '',
      nextActions: '',
      openQuestions: '',
      sourcesRaw: '',
    },
    reportTabs: [{ id: 'findings', label: 'Findings' }],
    activeTab: 'findings',
    onChangeTab: vi.fn(),
    onRefreshResearch: vi.fn(),
    onCopyReport: vi.fn(),
    onDownloadMarkdown: vi.fn(),
    onDownloadJson: vi.fn(),
    nextActionItems: [],
    runArtifacts: [],
    founderRuns: [],
    clientId: null,
    clientName: null,
    workspaceId: null,
    history: [],
    followUp: '',
    onFollowUpChange: vi.fn(),
    followLoading: false,
    agentId: null,
    onSubmitFollowUp: vi.fn(),
    bottomRef: { current: null },
    ...overrides,
  };

  return render(<DeepResearchWorkspace {...props} />);
}

describe('DeepResearchWorkspace', () => {
  it('renders the completed-state run artifacts panel with durable download metadata', () => {
    renderWorkspace({
      runArtifacts: [
        {
          artifactId: 'artifact-123',
          title: 'packet.json',
          kind: 'report_json',
          contentType: 'application/json',
          byteSize: 2048,
          createdAt: '2025-03-01T10:00:00.000Z',
        },
      ],
    });

    expect(screen.getByText('Run artifacts')).toBeInTheDocument();
    expect(screen.getByText('Durable files captured for this run.')).toBeInTheDocument();
    expect(screen.getByText('packet.json')).toBeInTheDocument();
    expect(screen.getByText('report_json')).toBeInTheDocument();
    expect(screen.getByText('application/json')).toBeInTheDocument();
    expect(screen.getByText('2.0 KB')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Download' })).toHaveAttribute('href', '/api/artifacts/artifact-123');
  });
});