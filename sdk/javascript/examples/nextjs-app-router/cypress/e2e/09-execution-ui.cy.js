/**
 * Layer 9 — Execution UI Coverage
 *
 * Verifies that the WorkDetailClient correctly displays ExecutionReport
 * data returned by POST /api/work/:id/run, including:
 *   - Run action triggers the backend call
 *   - ExecutionPanel renders when a report is present
 *   - Verification pass/fail is visible
 *   - Blocked state shows block reason
 *   - Delegated state shows the child link + action button
 *   - Retry state shows retry info
 *   - Timeline events include execution event types with correct labels
 *   - Refresh re-fetches item + events
 *   - Direct route to /work/:id renders the page without error
 *   - Runtime page shows execution activity table
 */

const API = () => Cypress.env('API_BASE');

// ─── Factories ────────────────────────────────────────────────────────────────

function makeWorkItem(overrides = {}) {
  return {
    id: 'wi-exec-1',
    title: 'Exec Test',
    description: 'Execution test work item',
    work_type: 'agent_task',
    source: 'api',
    status: 'pending',
    assigned_agent_id: 'agent-1',
    assigned_agent_name: 'Coder',
    result: null,
    error: null,
    payload: {},
    tags: [],
    created_by: null,
    priority: 128,
    retry_count: 0,
    max_retries: 3,
    parent_id: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  };
}

function makeExecutionReport(overrides = {}) {
  return {
    work_item_id: 'wi-exec-1',
    execution_path: 'fast_path',
    adapter_selection: {
      chosen: 'api',
      rejected: ['cli', 'browser'],
      rationale: 'Task is an API-level operation.',
    },
    objective: {
      target_system: 'test-system',
      intended_action: 'Perform test action',
      success_condition: 'Response is non-empty',
      verification_method: { ResponseNonEmpty: null },
      budget: { max_seconds: 120, max_iterations: 10, max_cost_usd: null },
      fallback_adapter: null,
    },
    action_result: {
      output: 'Hello! This is a test response.',
      tokens_in: 50,
      tokens_out: 20,
      cost_usd: 0.00003,
      iterations: 1,
      action_succeeded: true,
      error: null,
    },
    verification: {
      passed: true,
      method_used: { ResponseNonEmpty: null },
      evidence: 'Output is non-empty (31 chars).',
      verified_at: new Date().toISOString(),
    },
    status: 'completed',
    block_reason: null,
    result_summary: 'Test completed successfully.',
    artifact_refs: [],
    retry_count: 0,
    retry_scheduled: false,
    delegated_to: null,
    cost_usd: 0.00003,
    warnings: [],
    events_emitted: ['adapter_selected', 'execution_started', 'verified_success', 'completed'],
    started_at: new Date().toISOString(),
    finished_at: new Date().toISOString(),
    ...overrides,
  };
}

function listBody(items) {
  return { items, total: items.length };
}

function eventsBody(events = []) {
  return { events };
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function stubWorkDetailPage(item, report = null, children = [], events = []) {
  const completed = report ? makeWorkItem({ ...item, status: report.status }) : item;
  cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
  cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: eventsBody(events) }).as('getEvents');
  cy.intercept('GET', `${API()}/api/work*parent_id*`, { statusCode: 200, body: listBody(children) }).as('getChildren');
  if (report) {
    cy.intercept('POST', `${API()}/api/work/${item.id}/run`, { statusCode: 200, body: report }).as('runWork');
    // After run, the item is updated
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: completed }).as('getItemAfterRun');
  }
  // Planning endpoints — just return empty
  cy.intercept('GET', `${API()}/api/work/*/planning*`, { statusCode: 200, body: {} }).as('getPlanning');
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe('Layer 9 — Execution UI: basic report rendering', () => {
  const item = makeWorkItem({ id: 'wi-exec-1', status: 'pending' });
  const report = makeExecutionReport({ status: 'completed' });

  beforeEach(() => {
    stubWorkDetailPage(item, report);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('work detail page mounts without error', () => {
    cy.get('[data-cy="work-detail-page"]').should('exist');
    cy.get('[data-cy="work-detail-error"]').should('not.exist');
  });

  it('run button is visible for pending items', () => {
    cy.get('[data-cy="action-run"]').should('exist');
  });

  it('run action calls POST /api/work/:id/run', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork').its('request.url').should('include', `${item.id}/run`);
  });

  it('execution panel appears after run completes', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-panel"]').should('exist');
  });

  it('execution status badge is visible', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-status"]').should('contain', 'completed');
  });

  it('execution path is displayed in friendly form', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-path"]').should('contain', 'Fast path');
  });

  it('adapter selection is displayed', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-adapter"]').should('contain', 'api');
  });

  it('verification result is shown', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-verification"]').should('contain', 'Verified');
  });

  it('events emitted section lists execution events with labels', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-events"]').should('contain', 'Adapter selected');
    cy.get('[data-cy="execution-events"]').should('contain', 'Execution started');
    cy.get('[data-cy="execution-events"]').should('contain', 'Verified');
  });
});

describe('Layer 9 — Execution UI: verification failed state', () => {
  const item = makeWorkItem({ id: 'wi-exec-vfail', status: 'pending' });
  const report = makeExecutionReport({
    work_item_id: 'wi-exec-vfail',
    status: 'failed',
    verification: {
      passed: false,
      method_used: { ResponseNonEmpty: null },
      evidence: 'Output was empty.',
      verified_at: new Date().toISOString(),
    },
    events_emitted: ['adapter_selected', 'execution_started', 'verification_failed', 'failed'],
  });

  beforeEach(() => {
    stubWorkDetailPage(item, report);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows verification failed card', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-verification"]').should('contain', 'Verification failed');
  });

  it('timeline includes verification_failed event label', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-events"]').should('contain', 'Verification failed');
  });
});

describe('Layer 9 — Execution UI: blocked state', () => {
  const item = makeWorkItem({ id: 'wi-exec-blocked', status: 'pending' });
  const report = makeExecutionReport({
    work_item_id: 'wi-exec-blocked',
    status: 'blocked',
    block_reason: { MissingPermission: 'write:db' },
    verification: null,
    events_emitted: ['adapter_selected', 'permission_denied'],
  });

  beforeEach(() => {
    stubWorkDetailPage(item, report);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows block reason card', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-block-reason"]').should('exist');
    cy.get('[data-cy="execution-block-reason"]').should('contain', 'write:db');
  });

  it('shows permission_denied event label', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-events"]').should('contain', 'Permission denied');
  });
});

describe('Layer 9 — Execution UI: delegated state', () => {
  const childId = 'wi-child-001';
  const item = makeWorkItem({ id: 'wi-exec-delegated', status: 'pending', tags: ['delegate'] });
  const report = makeExecutionReport({
    work_item_id: 'wi-exec-delegated',
    status: 'delegated_to_subagent',
    delegated_to: childId,
    verification: null,
    events_emitted: ['adapter_selected', 'delegated_to_subagent'],
  });

  beforeEach(() => {
    stubWorkDetailPage(item, report);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows delegated field in execution details', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-delegated"]').should('exist');
    cy.get('[data-cy="execution-delegated"] a').should('have.attr', 'href', `/work/${childId}`);
  });

  it('shows view-delegated action button', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="action-view-delegated"]').should('exist');
    cy.get('[data-cy="action-view-delegated"]').should('have.attr', 'href', `/work/${childId}`);
  });

  it('shows delegated_to_subagent event label', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-events"]').should('contain', 'Delegated to sub-agent');
  });
});

describe('Layer 9 — Execution UI: retry state', () => {
  const item = makeWorkItem({ id: 'wi-exec-retry', status: 'pending' });
  const report = makeExecutionReport({
    work_item_id: 'wi-exec-retry',
    status: 'retry_scheduled',
    retry_count: 1,
    retry_scheduled: true,
    verification: {
      passed: false,
      method_used: { ResponseNonEmpty: null },
      evidence: '',
      verified_at: new Date().toISOString(),
    },
    events_emitted: ['adapter_selected', 'execution_started', 'verification_failed', 'retry_scheduled'],
  });

  beforeEach(() => {
    stubWorkDetailPage(item, report);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows retry info in execution panel', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-retry"]').should('contain', '1');
    cy.get('[data-cy="execution-retry"]').should('contain', 'scheduled');
  });

  it('shows retry_scheduled event label', () => {
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runWork');
    cy.get('[data-cy="execution-events"]').should('contain', 'Retry scheduled');
  });
});

describe('Layer 9 — Execution UI: timeline event labels', () => {
  const item = makeWorkItem({ id: 'wi-exec-timeline', status: 'completed' });
  const events = [
    { id: 'ev-1', event_type: 'execution_started', from_status: 'pending', to_status: 'running', created_at: new Date().toISOString(), detail: null, actor: null },
    { id: 'ev-2', event_type: 'verified_success',  from_status: 'running',  to_status: 'completed', created_at: new Date().toISOString(), detail: null, actor: null },
    { id: 'ev-3', event_type: 'adapter_selected',  from_status: null, to_status: null, created_at: new Date().toISOString(), detail: 'api', actor: null },
  ];

  beforeEach(() => {
    stubWorkDetailPage(item, null, [], events);
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('execution_started event displays friendly label', () => {
    cy.get('[data-cy="timeline-event-type"]').contains('Execution started').should('exist');
  });

  it('verified_success event displays friendly label', () => {
    cy.get('[data-cy="timeline-event-type"]').contains('Verified').should('exist');
  });

  it('adapter_selected event displays friendly label', () => {
    cy.get('[data-cy="timeline-event-type"]').contains('Adapter selected').should('exist');
  });
});

describe('Layer 9 — Execution UI: refresh re-fetches data', () => {
  const item = makeWorkItem({ id: 'wi-exec-refresh', status: 'pending' });

  beforeEach(() => {
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: eventsBody([]) }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
    cy.intercept('GET', `${API()}/api/work/*/planning*`, { statusCode: 200, body: {} }).as('getPlanning');
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('refresh calls GET /api/work/:id again', () => {
    cy.clickRefresh();
    cy.wait('@getItem');
    cy.wait('@getEvents');
  });
});

describe('Layer 9 — Execution UI: direct route', () => {
  it('direct route to /work/:id renders without crashing', () => {
    const item = makeWorkItem({ id: 'wi-exec-direct', status: 'completed' });
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: eventsBody([]) }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
    cy.intercept('GET', `${API()}/api/work/*/planning*`, { statusCode: 200, body: {} }).as('getPlanning');
    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
    cy.get('[data-cy="work-detail-error"]').should('not.exist');
  });
});

describe('Layer 9 — Execution UI: runtime page execution activity', () => {
  const items = [
    makeWorkItem({ id: 'wi-r1', status: 'completed',  title: 'Task 1', completed_at: new Date().toISOString() }),
    makeWorkItem({ id: 'wi-r2', status: 'failed',      title: 'Task 2', completed_at: new Date().toISOString() }),
    makeWorkItem({ id: 'wi-r3', status: 'running',     title: 'Task 3', completed_at: null }),
  ];

  beforeEach(() => {
    cy.intercept('GET', `${API()}/api/health`, { statusCode: 200, body: { status: 'ok', version: '0.9.0', uptime_seconds: 3600 } }).as('health');
    cy.intercept('GET', `${API()}/api/status`, { statusCode: 200, body: { agent_count: 5 } }).as('status');
    cy.intercept('GET', `${API()}/api/network/status`, { statusCode: 200, body: { connected: true, node_id: 'node-1', peer_count: 2 } }).as('network');
    cy.intercept('GET', `${API()}/api/peers`, { statusCode: 200, body: { peers: [] } }).as('peers');
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: { items, total: items.length } }).as('getWork');
    cy.visit('/runtime');
    cy.get('[data-cy="runtime-page"]').should('exist');
  });

  it('execution activity section exists', () => {
    cy.clickRefresh();
    cy.wait('@getWork');
    cy.get('[data-cy="runtime-execution-activity"]').should('exist');
  });

  it('renders executed work items (non-pending) in activity table', () => {
    cy.clickRefresh();
    cy.wait('@getWork');
    cy.get('[data-cy="runtime-exec-item"]').should('have.length.gte', 1);
  });

  it('each execution item has a view link', () => {
    cy.clickRefresh();
    cy.wait('@getWork');
    cy.get('[data-cy="runtime-exec-item-link"]').first().should('have.attr', 'href').and('include', '/work/');
  });
});
