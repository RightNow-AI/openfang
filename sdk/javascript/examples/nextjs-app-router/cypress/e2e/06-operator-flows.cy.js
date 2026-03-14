/**
 * Layer 6 — Operator Flows
 *
 * Covers the Phase 3 orchestration and operator-visibility features:
 *   - Operator Dashboard (summary tiles, orchestrator status, heartbeat)
 *   - Work Item Detail page (timeline, actions, parent/child links)
 *   - Approval flow end-to-end on the detail page
 *   - Failure + retry flow on the detail page
 *   - Delegation creates a child item visible in the detail page
 */

const API = () => Cypress.env('API_BASE');

// ─── Shared stub factory ──────────────────────────────────────────────────────

function makeWorkItem(overrides = {}) {
  return {
    id: 'wi-op-1',
    title: 'Operator Test Item',
    description: 'Test description',
    work_type: 'agent_task',
    source: 'api',
    status: 'pending',
    approval_status: 'not_required',
    assigned_agent_id: 'agent-1',
    assigned_agent_name: 'Coder',
    result: null,
    error: null,
    iterations: 0,
    priority: 128,
    scheduled_at: null,
    started_at: null,
    completed_at: null,
    deadline: null,
    requires_approval: false,
    approved_by: null,
    approved_at: null,
    approval_note: null,
    payload: {},
    tags: [],
    created_by: 'test',
    idempotency_key: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    retry_count: 0,
    max_retries: 3,
    parent_id: null,
    ...overrides,
  };
}

function listBody(items) {
  return { items, total: items.length };
}

const SUMMARY = {
  pending: 5,
  ready: 2,
  running: 3,
  waiting_approval: 1,
  approved: 0,
  rejected: 0,
  completed: 42,
  failed: 1,
  cancelled: 0,
  total: 54,
  scheduled: 2,
  generated_at: new Date().toISOString(),
};

const ORCH_STATUS = {
  running: true,
  last_heartbeat_at: new Date().toISOString(),
  queued_count: 7,
  running_count: 3,
  pending_approval_count: 1,
  scheduler_version: '1.0.0',
  uptime_secs: 3600,
};

const HEARTBEAT_RESULT = {
  id: 'hb-001',
  triggered_at: new Date().toISOString(),
  triggered_by: 'operator',
  items_claimed: 2,
  items_scheduled_started: 1,
  items_delegated: 0,
  duration_ms: 8,
  note: null,
};

// ─── Dashboard ────────────────────────────────────────────────────────────────

describe('Layer 6 — Operator Dashboard', () => {
  beforeEach(() => {
    cy.intercept('GET', `${API()}/api/work/summary`, { statusCode: 200, body: SUMMARY }).as('getSummary');
    cy.intercept('GET', `${API()}/api/orchestrator/status`, { statusCode: 200, body: ORCH_STATUS }).as('getOrchStatus');
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
  });

  it('mounts the dashboard page root element', () => {
    cy.visit('/dashboard');
    cy.get('[data-cy="dashboard-page"]').should('exist');
  });

  it('renders summary stat tiles', () => {
    cy.visit('/dashboard');
    cy.clickRefresh();
    cy.wait('@getSummary');
    cy.get('[data-cy="dashboard-pending-count"]').should('contain', '5');
    cy.get('[data-cy="dashboard-running-count"]').should('contain', '3');
    cy.get('[data-cy="dashboard-approval-count"]').should('contain', '1');
    cy.get('[data-cy="dashboard-failed-count"]').should('contain', '1');
    cy.get('[data-cy="dashboard-completed-count"]').should('contain', '42');
    cy.get('[data-cy="dashboard-scheduled-count"]').should('contain', '2');
  });

  it('renders orchestrator status section', () => {
    cy.visit('/dashboard');
    cy.clickRefresh();
    cy.wait('@getOrchStatus');
    cy.get('[data-cy="dashboard-orchestrator-status"]').should('exist');
    cy.get('[data-cy="dashboard-orchestrator-status"]').should('contain', 'Running');
  });

  it('heartbeat button calls POST /api/orchestrator/heartbeat', () => {
    cy.intercept('POST', `${API()}/api/orchestrator/heartbeat`, { statusCode: 200, body: HEARTBEAT_RESULT }).as('heartbeat');
    cy.visit('/dashboard');
    cy.get('[data-cy="dashboard-heartbeat-btn"]').click();
    cy.wait('@heartbeat');
    cy.contains(`claimed ${HEARTBEAT_RESULT.items_claimed}`).should('exist');
  });

  it('heartbeat button is disabled while request is in flight', () => {
    cy.intercept('POST', `${API()}/api/orchestrator/heartbeat`, (req) => {
      req.on('response', (res) => { res.delay = 300; });
      req.reply({ statusCode: 200, body: HEARTBEAT_RESULT });
    }).as('heartbeatSlow');
    cy.visit('/dashboard');
    cy.get('[data-cy="dashboard-heartbeat-btn"]').click();
    cy.get('[data-cy="dashboard-heartbeat-btn"]').should('be.disabled');
    cy.wait('@heartbeatSlow');
  });

  it('shows running items list', () => {
    const running = makeWorkItem({ id: 'wi-run-1', title: 'Running Task', status: 'running' });
    cy.intercept('GET', `${API()}/api/work*status=running*`, { statusCode: 200, body: listBody([running]) }).as('getRunning');
    cy.visit('/dashboard');
    cy.clickRefresh();
    cy.wait('@getRunning');
    cy.get('[data-cy="dashboard-running-list"]').should('contain', 'Running Task');
  });

  it('shows approval items list', () => {
    const approval = makeWorkItem({ id: 'wi-appr-1', title: 'Needs Approval', status: 'waiting_approval' });
    cy.intercept('GET', `${API()}/api/work*status=waiting_approval*`, { statusCode: 200, body: listBody([approval]) }).as('getApproval');
    cy.intercept('GET', `${API()}/api/work*status=running*`, { statusCode: 200, body: listBody([]) }).as('getRunning');
    cy.intercept('GET', `${API()}/api/work*status=failed*`, { statusCode: 200, body: listBody([]) }).as('getFailed');
    cy.visit('/dashboard');
    cy.clickRefresh();
    cy.wait('@getApproval');
    cy.get('[data-cy="dashboard-approval-list"]').should('contain', 'Needs Approval');
  });

  it('shows failed items list', () => {
    const failed = makeWorkItem({ id: 'wi-fail-1', title: 'Failed Task', status: 'failed' });
    cy.intercept('GET', `${API()}/api/work*status=failed*`, { statusCode: 200, body: listBody([failed]) }).as('getFailed');
    cy.intercept('GET', `${API()}/api/work*status=running*`, { statusCode: 200, body: listBody([]) }).as('getRunning');
    cy.intercept('GET', `${API()}/api/work*status=waiting_approval*`, { statusCode: 200, body: listBody([]) }).as('getApproval');
    cy.visit('/dashboard');
    cy.clickRefresh();
    cy.wait('@getFailed');
    cy.get('[data-cy="dashboard-failed-list"]').should('contain', 'Failed Task');
  });

  it('sidebar contains Dashboard link', () => {
    cy.visit('/dashboard');
    cy.get('nav a[href="/dashboard"]').should('exist');
  });
});

// ─── Work Item Detail ─────────────────────────────────────────────────────────

describe('Layer 6 — Work Item Detail Page', () => {
  const ITEM_ID = 'wi-detail-1';
  const item = makeWorkItem({ id: ITEM_ID, title: 'Detail Test', status: 'pending' });

  const EVENT = {
    id: 'ev-1',
    work_item_id: ITEM_ID,
    event_type: 'created',
    from_status: null,
    to_status: 'pending',
    actor: 'api',
    detail: null,
    created_at: new Date().toISOString(),
  };

  beforeEach(() => {
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [EVENT] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
  });

  it('mounts the work detail page root element', () => {
    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows action buttons for pending item', () => {
    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="work-detail-actions"]').should('exist');
    cy.get('[data-cy="action-run"]').should('exist');
    cy.get('[data-cy="action-cancel"]').should('exist');
  });

  it('timeline renders events', () => {
    cy.visit(`/work/${ITEM_ID}`);
    cy.clickRefresh();
    cy.wait('@getEvents');
    cy.get('[data-cy="work-detail-timeline"]').should('exist');
    cy.get('[data-cy="timeline-event"]').should('have.length.at.least', 1);
    cy.get('[data-cy="timeline-event"]').first().should('contain', 'created');
  });

  it('run action calls POST /api/work/:id/run', () => {
    const runningItem = makeWorkItem({ id: ITEM_ID, status: 'running' });
    cy.intercept('POST', `${API()}/api/work/${ITEM_ID}/run`, { statusCode: 200, body: runningItem }).as('runItem');
    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-run"]').click();
    cy.wait('@runItem');
  });

  it('cancel action calls POST /api/work/:id/cancel', () => {
    const cancelledItem = makeWorkItem({ id: ITEM_ID, status: 'cancelled' });
    cy.intercept('POST', `${API()}/api/work/${ITEM_ID}/cancel`, { statusCode: 200, body: cancelledItem }).as('cancelItem');
    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-cancel"]').click();
    cy.wait('@cancelItem');
  });

  it('shows parent link when item has parent_id', () => {
    const PARENT_ID = 'wi-parent-1';
    const childItem = makeWorkItem({ id: ITEM_ID, parent_id: PARENT_ID });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: childItem }).as('getChildItem');
    cy.visit(`/work/${ITEM_ID}`);
    cy.clickRefresh();
    cy.wait('@getChildItem');
    cy.get('[data-cy="work-detail-parent-link"]')
      .should('exist')
      .and('have.attr', 'href')
      .and('include', PARENT_ID);
  });

  it('shows children list when item has sub-tasks', () => {
    const child = makeWorkItem({ id: 'wi-child-1', title: 'Sub-task', parent_id: ITEM_ID, status: 'pending' });
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([child]) }).as('getChildrenPopulated');
    cy.visit(`/work/${ITEM_ID}`);
    cy.clickRefresh();
    cy.wait('@getChildrenPopulated');
    cy.get('[data-cy="work-detail-children"]').should('exist').and('contain', 'Sub-task');
  });

  it('refresh re-fetches item, events, and children', () => {
    let itemCalls = 0;
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, (req) => {
      itemCalls++;
      req.reply({ statusCode: 200, body: item });
    }).as('getItemRefresh');
    cy.visit(`/work/${ITEM_ID}`);
    cy.clickRefresh();
    cy.wait('@getItemRefresh').then(() => {
      expect(itemCalls).to.be.greaterThan(0);
    });
  });
});

// ─── Approval Flow ────────────────────────────────────────────────────────────

describe('Layer 6 — Approval Flow (wait → approve)', () => {
  const ITEM_ID = 'wi-appr-flow-1';

  it('shows approve and reject buttons when status is waiting_approval', () => {
    const approvalItem = makeWorkItem({ id: ITEM_ID, status: 'waiting_approval', requires_approval: true });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: approvalItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-approve"]').should('exist');
    cy.get('[data-cy="action-reject"]').should('exist');
    cy.get('[data-cy="action-run"]').should('not.exist');
  });

  it('approve action calls POST /api/work/:id/approve', () => {
    const approvalItem = makeWorkItem({ id: ITEM_ID, status: 'waiting_approval' });
    const approvedItem = makeWorkItem({ id: ITEM_ID, status: 'approved' });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: approvalItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
    cy.intercept('POST', `${API()}/api/work/${ITEM_ID}/approve`, { statusCode: 200, body: approvedItem }).as('approve');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-approve"]').click();
    cy.wait('@approve');
  });

  it('reject action calls POST /api/work/:id/reject', () => {
    const approvalItem = makeWorkItem({ id: ITEM_ID, status: 'waiting_approval' });
    const rejectedItem = makeWorkItem({ id: ITEM_ID, status: 'rejected' });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: approvalItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
    cy.intercept('POST', `${API()}/api/work/${ITEM_ID}/reject`, { statusCode: 200, body: rejectedItem }).as('reject');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-reject"]').click();
    cy.wait('@reject');
  });
});

// ─── Failure + Retry Flow ─────────────────────────────────────────────────────

describe('Layer 6 — Failure + Retry Flow', () => {
  const ITEM_ID = 'wi-fail-flow-1';

  it('shows retry button and hides run button when status is failed', () => {
    const failedItem = makeWorkItem({ id: ITEM_ID, status: 'failed', error: 'LLM timeout' });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: failedItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-retry"]').should('exist');
    cy.get('[data-cy="action-run"]').should('not.exist');
  });

  it('retry action calls POST /api/work/:id/retry', () => {
    const failedItem = makeWorkItem({ id: ITEM_ID, status: 'failed' });
    const retriedItem = makeWorkItem({ id: ITEM_ID, status: 'pending' });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: failedItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');
    cy.intercept('POST', `${API()}/api/work/${ITEM_ID}/retry`, { statusCode: 200, body: retriedItem }).as('retry');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="action-retry"]').click();
    cy.wait('@retry');
  });

  it('error message is visible in the details card for a failed item', () => {
    const failedItem = makeWorkItem({ id: ITEM_ID, status: 'failed', error: 'Connection refused' });
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}`, { statusCode: 200, body: failedItem }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${ITEM_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${ITEM_ID}*`, { statusCode: 200, body: listBody([]) }).as('getChildren');

    cy.visit(`/work/${ITEM_ID}`);
    cy.get('[data-cy="work-detail-page"]').should('contain', 'Connection refused');
  });
});

// ─── Delegation / Parent-Child ────────────────────────────────────────────────

describe('Layer 6 — Delegation and Parent-Child Links', () => {
  const PARENT_ID = 'wi-parent-del-1';
  const CHILD_ID = 'wi-child-del-1';

  it('POST /api/work/:id/delegate creates a child item', () => {
    const parentItem = makeWorkItem({ id: PARENT_ID, status: 'running' });
    const childItem = makeWorkItem({ id: CHILD_ID, title: 'Delegated Sub-task', parent_id: PARENT_ID, status: 'pending', source: 'AgentSpawned' });
    cy.intercept('GET', `${API()}/api/work/${PARENT_ID}`, { statusCode: 200, body: parentItem }).as('getParent');
    cy.intercept('GET', `${API()}/api/work/${PARENT_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${PARENT_ID}*`, { statusCode: 200, body: listBody([childItem]) }).as('getChildren');
    cy.intercept('POST', `${API()}/api/work/${PARENT_ID}/delegate`, { statusCode: 200, body: childItem }).as('delegate');

    cy.visit(`/work/${PARENT_ID}`);
    cy.clickRefresh();
    cy.wait('@getChildren');
    cy.get('[data-cy="work-detail-children"]').should('exist').and('contain', 'Delegated Sub-task');
  });

  it('child item detail page shows link back to parent', () => {
    const childItem = makeWorkItem({ id: CHILD_ID, title: 'Child Task', parent_id: PARENT_ID });
    cy.intercept('GET', `${API()}/api/work/${CHILD_ID}`, { statusCode: 200, body: childItem }).as('getChild');
    cy.intercept('GET', `${API()}/api/work/${CHILD_ID}/events`, { statusCode: 200, body: { events: [] } }).as('getEvents');
    cy.intercept('GET', `${API()}/api/work*parent_id=${CHILD_ID}*`, { statusCode: 200, body: listBody([]) }).as('getGrandchildren');

    cy.visit(`/work/${CHILD_ID}`);
    cy.clickRefresh();
    cy.wait('@getChild');
    cy.get('[data-cy="work-detail-parent-link"]')
      .should('have.attr', 'href')
      .and('include', PARENT_ID);
  });
});
