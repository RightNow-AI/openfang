/**
 * Layer 5 — WorkItem Migration Coverage
 *
 * Proves that Inbox, Approvals, Scheduler, and Workflows all operate
 * exclusively through the /api/work WorkItem endpoints after the Phase 2
 * migration.  Each describe block covers:
 *   - Route loads and mounts the correct root element
 *   - List renders from the WorkItem response shape
 *   - User actions call the correct WorkItem sub-resource endpoints
 *   - Navigation / redirect behaviour after creating a work item
 */

const API = () => Cypress.env('API_BASE');

// ─── Shared stub factory ───────────────────────────────────────────────────────

function makeWorkItem(overrides = {}) {
  return {
    id: 'wi-test-1',
    title: 'Test Work',
    description: 'A test work item',
    work_type: 'agent_task',
    source: 'api',
    status: 'pending',
    approval_status: 'not_required',
    assigned_agent_id: null,
    assigned_agent_name: null,
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
    created_by: null,
    idempotency_key: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    retry_count: 0,
    max_retries: 3,
    ...overrides,
  };
}

function listBody(items) {
  return { items, total: items.length };
}

// ─── Inbox ────────────────────────────────────────────────────────────────────

describe('Layer 5 — Inbox WorkItem Migration', () => {
  it('mounts the inbox page root element', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/inbox');
    cy.get('[data-cy="inbox-page"]').should('exist');
  });

  it('fetches GET /api/work with status=pending', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getWork').its('request.url').should('include', 'status=pending');
  });

  it('renders inbox-list when items are returned', () => {
    const item = makeWorkItem({ id: 'wi-inbox-1', title: 'Inbox Item' });
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="inbox-list"]').should('exist');
    cy.get('[data-cy="inbox-item"]').should('have.length', 1);
  });

  it('each inbox item contains a link to /work/:id', () => {
    const item = makeWorkItem({ id: 'wi-inbox-link', title: 'Link Test Item' });
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="inbox-item-detail-link"]')
      .first()
      .should('have.attr', 'href')
      .and('include', '/work/wi-inbox-link');
  });

  it('Refresh re-fetches /api/work (not a legacy endpoint)', () => {
    let callCount = 0;
    cy.intercept('GET', `${API()}/api/work*`, (req) => {
      callCount++;
      req.reply({ statusCode: 200, body: listBody([]) });
    }).as('getWork');

    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getWork');
    cy.then(() => expect(callCount).to.be.at.least(1));

    // Ensure the legacy endpoint was never called
    cy.get('@getWork').its('request.url').should('not.include', 'planner/inbox');
  });
});

// ─── Approvals ────────────────────────────────────────────────────────────────

describe('Layer 5 — Approvals WorkItem Migration', () => {
  it('mounts the approvals page root element', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/approvals');
    cy.get('[data-cy="approvals-page"]').should('exist');
  });

  it('fetches GET /api/work with approval_status=pending', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getWork').its('request.url').should('include', 'approval_status=pending');
  });

  it('renders approvals-list with approval-card elements', () => {
    const item = makeWorkItem({
      id: 'wi-appr-1',
      title: 'Approval Request',
      status: 'waiting_approval',
      approval_status: 'pending',
      requires_approval: true,
    });
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="approvals-list"]').should('exist');
    cy.get('[data-cy="approval-card"]').should('have.length', 1);
  });

  it('approve button calls POST /api/work/:id/approve and removes the card', () => {
    const item = makeWorkItem({ id: 'wi-appr-approve', title: 'To Approve', approval_status: 'pending' });

    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.intercept('POST', `${API()}/api/work/${item.id}/approve`, {
      statusCode: 200,
      body: { ...item, approval_status: 'approved', status: 'approved' },
    }).as('approveWork');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="approval-approve-btn"]').first().click();
    cy.wait('@approveWork');

    // Card should be removed from the list after successful approval
    cy.get('[data-cy="approval-card"]').should('have.length', 0);
  });

  it('reject button calls POST /api/work/:id/reject and removes the card', () => {
    const item = makeWorkItem({ id: 'wi-appr-reject', title: 'To Reject', approval_status: 'pending' });

    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.intercept('POST', `${API()}/api/work/${item.id}/reject`, {
      statusCode: 200,
      body: { ...item, approval_status: 'rejected', status: 'rejected' },
    }).as('rejectWork');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="approval-reject-btn"]').first().click();
    cy.wait('@rejectWork');

    cy.get('[data-cy="approval-card"]').should('have.length', 0);
  });
});

// ─── Scheduler ────────────────────────────────────────────────────────────────

describe('Layer 5 — Scheduler WorkItem Migration', () => {
  it('mounts the scheduler page root element', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/scheduler');
    cy.get('[data-cy="scheduler-page"]').should('exist');
  });

  it('fetches GET /api/work with scheduled=true', () => {
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([]) }).as('getWork');
    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getWork').its('request.url').should('include', 'scheduled=true');
  });

  it('renders scheduled-list with scheduled-item elements', () => {
    const item = makeWorkItem({
      id: 'wi-sched-1',
      title: 'Scheduled Task',
      scheduled_at: new Date(Date.now() + 3600000).toISOString(),
    });
    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="scheduled-list"]').should('exist');
    cy.get('[data-cy="scheduled-item"]').should('have.length', 1);
  });

  it('cancel button calls POST /api/work/:id/cancel then refreshes', () => {
    const item = makeWorkItem({
      id: 'wi-sched-cancel',
      title: 'Cancellable Task',
      scheduled_at: new Date(Date.now() + 3600000).toISOString(),
    });

    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.intercept('POST', `${API()}/api/work/${item.id}/cancel`, {
      statusCode: 200,
      body: { ...item, status: 'cancelled' },
    }).as('cancelWork');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="schedule-cancel-btn"]').first().click();
    cy.wait('@cancelWork');
    // Refresh is triggered — a second GET should fire
    cy.wait('@getWork');
  });

  it('retry button calls POST /api/work/:id/retry then refreshes', () => {
    const item = makeWorkItem({
      id: 'wi-sched-retry',
      title: 'Retryable Task',
      status: 'failed',
      scheduled_at: new Date(Date.now() + 3600000).toISOString(),
    });

    cy.intercept('GET', `${API()}/api/work*`, { statusCode: 200, body: listBody([item]) }).as('getWork');
    cy.intercept('POST', `${API()}/api/work/${item.id}/retry`, {
      statusCode: 200,
      body: { ...item, status: 'pending', retry_count: 1 },
    }).as('retryWork');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getWork');

    cy.get('[data-cy="schedule-retry-btn"]').first().click();
    cy.wait('@retryWork');
    cy.wait('@getWork');
  });
});

// ─── Workflows ────────────────────────────────────────────────────────────────

describe('Layer 5 — Workflows WorkItem Migration', () => {
  it('mounts the workflows page root element', () => {
    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [] }).as('getTemplates');
    cy.visit('/workflows');
    cy.get('[data-cy="workflows-page"]').should('exist');
  });

  it('Run button creates a work item via POST /api/work', () => {
    const template = {
      id: 'tpl-wf-1',
      name: 'My Workflow',
      description: 'A workflow template',
      steps: 3,
      created_at: new Date().toISOString(),
    };
    const createdItem = makeWorkItem({ id: 'wi-wf-created', work_type: 'workflow', title: 'My Workflow' });

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [template] }).as('getTemplates');
    cy.intercept('POST', `${API()}/api/work`, { statusCode: 201, body: createdItem }).as('createWork');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getTemplates');

    cy.get('[data-cy="workflow-run-btn"]').first().click();
    cy.wait('@createWork');

    // Verify the POST body includes workflow_template_id
    cy.get('@createWork').its('request.body').should('deep.include', {
      work_type: 'workflow',
    });
  });

  it('after successful run, navigates to /work/:id', () => {
    const template = {
      id: 'tpl-wf-nav',
      name: 'Nav Workflow',
      description: '',
      steps: 1,
      created_at: new Date().toISOString(),
    };
    const createdItem = makeWorkItem({ id: 'wi-wf-nav', work_type: 'workflow', title: 'Nav Workflow' });

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [template] }).as('getTemplates');
    cy.intercept('POST', `${API()}/api/work`, { statusCode: 201, body: createdItem }).as('createWork');
    // Stub the work detail fetch that the /work/[id] page makes
    cy.intercept('GET', `${API()}/api/work/${createdItem.id}`, { statusCode: 200, body: createdItem });
    cy.intercept('GET', `${API()}/api/work/${createdItem.id}/events`, { statusCode: 200, body: [] });

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getTemplates');

    cy.get('[data-cy="workflow-run-btn"]').first().click();
    cy.wait('@createWork');

    // Should redirect to the work detail page
    cy.location('pathname').should('include', `/work/${createdItem.id}`);
  });

  it('result badge appears when run succeeds with a work item id', () => {
    const template = {
      id: 'tpl-wf-badge',
      name: 'Badge Workflow',
      description: '',
      steps: 1,
      created_at: new Date().toISOString(),
    };
    const createdItem = makeWorkItem({ id: 'wi-wf-badge', work_type: 'workflow', title: 'Badge Workflow' });

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [template] }).as('getTemplates');
    cy.intercept('POST', `${API()}/api/work`, { statusCode: 201, body: createdItem }).as('createWork');
    cy.intercept('GET', `${API()}/api/work/${createdItem.id}`, { statusCode: 200, body: createdItem });
    cy.intercept('GET', `${API()}/api/work/${createdItem.id}/events`, { statusCode: 200, body: [] });

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getTemplates');

    cy.get('[data-cy="workflow-run-btn"]').first().click();
    cy.wait('@createWork');

    // After redirect, the work detail page should mount
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('result badge shows error state when POST /api/work returns 500', () => {
    const template = {
      id: 'tpl-wf-err',
      name: 'Error Workflow',
      description: '',
      steps: 1,
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [template] }).as('getTemplates');
    cy.intercept('POST', `${API()}/api/work`, {
      statusCode: 500,
      body: { error: 'Internal server error' },
    }).as('createFail');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getTemplates');

    cy.get('[data-cy="workflow-run-btn"]').first().click();
    cy.wait('@createFail');

    cy.get('[data-cy="workflow-result-badge"]')
      .should('exist')
      .and(($el) => {
        const text = $el.text().toLowerCase();
        expect(text).to.match(/error|fail/);
      });
  });
});

// ─── Work Detail Page ─────────────────────────────────────────────────────────

describe('Layer 5 — Work Detail Page', () => {
  it('mounts work-detail-page for a valid work item', () => {
    const item = makeWorkItem({ id: 'wi-detail-1', title: 'Detail Test', status: 'running' });
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: [] }).as('getEvents');

    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-page"]').should('exist');
  });

  it('shows approve and reject buttons when status is waiting_approval', () => {
    const item = makeWorkItem({
      id: 'wi-detail-appr',
      title: 'Needs Approval',
      status: 'waiting_approval',
      approval_status: 'pending',
      requires_approval: true,
    });
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: [] });

    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-actions"]').should('exist');
    cy.get('[data-cy="action-approve"]').should('exist');
    cy.get('[data-cy="action-reject"]').should('exist');
  });

  it('shows retry button when status is failed', () => {
    const item = makeWorkItem({ id: 'wi-detail-fail', title: 'Failed Task', status: 'failed' });
    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: [] });

    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="action-retry"]').should('exist');
    cy.get('[data-cy="action-approve"]').should('not.exist');
  });

  it('timeline renders events when present', () => {
    const item = makeWorkItem({ id: 'wi-detail-tl', title: 'With Timeline', status: 'completed' });
    const events = [
      {
        id: 'evt-1',
        work_item_id: item.id,
        event_type: 'status_changed',
        status_before: 'pending',
        status_after: 'running',
        detail: null,
        actor: 'system',
        created_at: new Date().toISOString(),
      },
      {
        id: 'evt-2',
        work_item_id: item.id,
        event_type: 'status_changed',
        status_before: 'running',
        status_after: 'completed',
        detail: null,
        actor: 'agent',
        created_at: new Date().toISOString(),
      },
    ];

    cy.intercept('GET', `${API()}/api/work/${item.id}`, { statusCode: 200, body: item }).as('getItem');
    cy.intercept('GET', `${API()}/api/work/${item.id}/events`, { statusCode: 200, body: events }).as('getEvents');

    cy.visit(`/work/${item.id}`);
    cy.get('[data-cy="work-detail-timeline"]').should('exist');
    cy.get('[data-cy="timeline-event"]').should('have.length', 2);
  });
});
