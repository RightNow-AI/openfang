/**
 * Layer 4 — Failure Behavior
 *
 * Proves for every page that:
 *  1. A 500 error on the browser-side refresh path shows the error state
 *     (data-cy="*-error") and does NOT show the data element.
 *  2. A 200 response with an empty payload shows the empty state
 *     (data-cy="*-empty") and does NOT show the data element.
 *  3. The Refresh button is re-enabled after an error — users can retry.
 *  4. Recovering from an error (stubbing success on second fetch) correctly
 *     removes the error state and renders the data.
 *
 * Architecture notes:
 *  - Only browser-side fetch paths (triggered by the Refresh button or an
 *    explicit user action) can be stubbed by cy.intercept.  The initial SSR
 *    render comes from the server and cannot be intercepted.  Therefore all
 *    failure tests follow the pattern:
 *      visit page → stub endpoint to fail → click Refresh → verify error element.
 *  - cy.forceApiError(method, path, alias?) is a custom command defined in
 *    cypress/support/commands.js.
 */

const API = () => Cypress.env('API_BASE');

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Full failure→recovery flow for a single-endpoint page.
 *
 * 1. Visit the page.
 * 2. Stub the endpoint to fail → click Refresh → assert error element.
 * 3. Stub the endpoint to succeed → click Refresh → assert no error, data visible.
 */
function failThenRecover(path, apiPath, successBody, rootCy, errorCy, dataCy) {
  // Visit with failure stub
  cy.forceApiError('GET', `${API()}${apiPath}`, 'failStub');
  cy.visit(path);
  cy.get(`[data-cy="${rootCy}"]`).should('exist');
  cy.clickRefresh();
  cy.wait('@failStub');
  cy.get(`[data-cy="${errorCy}"]`).should('exist');
  cy.get(`[data-cy="${dataCy}"]`).should('not.exist');

  // Retry with success — error must disappear, data must render
  cy.intercept('GET', `${API()}${apiPath}`, {
    statusCode: 200,
    body: successBody,
  }).as('recoveryStub');
  cy.clickRefresh();
  cy.wait('@recoveryStub');
  cy.get(`[data-cy="${errorCy}"]`).should('not.exist');
  cy.get(`[data-cy="${dataCy}"]`).should('exist');
}

// ─── Chat ─────────────────────────────────────────────────────────────────────
describe('Layer 4 — Chat Failure Behavior', () => {
  it('shows error state when POST /api/agents/:id/message returns 500', () => {
    cy.intercept('GET', `${API()}/api/agents/*/session`, {
      statusCode: 200,
      body: { messages: [], session_id: 'test', message_count: 0, context_window_tokens: 0 },
    }).as('loadSession');

    cy.intercept('POST', `${API()}/api/agents/*/message`, {
      statusCode: 500,
      body: { error: 'Internal server error from Cypress stub' },
    }).as('sendFail');

    cy.visit('/chat');
    cy.wait('@loadSession');

    cy.get('[data-cy="chat-input"]').type('trigger error');
    cy.get('[data-cy="chat-send-btn"]').click();
    cy.wait('@sendFail');

    // Error state must be visible
    cy.get('[data-cy="chat-error"]')
      .should('exist')
      .and('not.have.css', 'display', 'none');
  });

  it('send button is re-enabled after a failed send', () => {
    cy.intercept('GET', `${API()}/api/agents/*/session`, {
      statusCode: 200,
      body: { messages: [], session_id: 'test', message_count: 0, context_window_tokens: 0 },
    }).as('loadSession');

    cy.intercept('POST', `${API()}/api/agents/*/message`, {
      statusCode: 503,
      body: { error: 'Service unavailable' },
    }).as('sendFail');

    cy.visit('/chat');
    cy.wait('@loadSession');

    cy.get('[data-cy="chat-input"]').type('fail me');
    cy.get('[data-cy="chat-send-btn"]').click();
    cy.wait('@sendFail');

    // Send button must be re-enabled so the user can retry
    cy.get('[data-cy="chat-send-btn"]').should('not.be.disabled');
  });
});

// ─── Inbox ────────────────────────────────────────────────────────────────────
describe('Layer 4 — Inbox Failure Behavior', () => {
  it('shows inbox-error when GET /api/planner/inbox returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/planner/inbox`, 'inboxFail');
    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@inboxFail');

    cy.get('[data-cy="inbox-error"]').should('exist');
    cy.get('[data-cy="inbox-list"]').should('not.exist');
  });

  it('shows inbox-empty when API returns an empty items array', () => {
    cy.intercept('GET', `${API()}/api/planner/inbox`, {
      statusCode: 200,
      body: { items: [] },
    }).as('inboxEmpty');

    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@inboxEmpty');

    cy.get('[data-cy="inbox-empty"]').should('exist');
    cy.get('[data-cy="inbox-list"]').should('not.exist');
  });

  it('Refresh button is re-enabled after error', () => {
    cy.forceApiError('GET', `${API()}/api/planner/inbox`, 'inboxFail');
    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@inboxFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('recovers from error to display items on retry', () => {
    const stubItem = {
      id: 'recovery-item',
      text: 'Recovered item',
      status: 'pending',
      project_title: 'Recovery Project',
      tasks: [],
      agent_recommendations: [],
      created_at: new Date().toISOString(),
    };
    failThenRecover(
      '/inbox',
      '/api/planner/inbox',
      { items: [stubItem] },
      'inbox-page',
      'inbox-error',
      'inbox-list',
    );
  });
});

// ─── Agent Catalog ─────────────────────────────────────────────────────────────
describe('Layer 4 — Agent Catalog Failure Behavior', () => {
  it('shows catalog-error when GET /api/agents/catalog returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/agents/catalog`, 'catalogFail');
    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@catalogFail');

    cy.get('[data-cy="catalog-error"]').should('exist');
    cy.get('[data-cy="catalog-grid"]').should('not.exist');
  });

  it('shows catalog-empty when API returns empty agents array', () => {
    cy.intercept('GET', `${API()}/api/agents/catalog`, {
      statusCode: 200,
      body: { agents: [] },
    }).as('catalogEmpty');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@catalogEmpty');

    cy.get('[data-cy="catalog-empty"]').should('exist');
    cy.get('[data-cy="catalog-card"]').should('not.exist');
  });

  it('shows catalog-filter-empty when filter matches no cards', () => {
    const entry = {
      catalog_id: 'cf1',
      agent_id: 'assistant',
      name: 'Alpha',
      description: '',
      division: '',
      tags: [],
      enabled: true,
      best_for: '',
      avoid_for: '',
      example: '',
      source: 'native',
      source_label: '',
    };

    cy.intercept('GET', `${API()}/api/agents/catalog`, {
      statusCode: 200,
      body: { agents: [entry] },
    }).as('getCatalog');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog');

    cy.get('[data-cy="catalog-filter"]').type('ZZZ-DOES-NOT-EXIST');

    cy.get('[data-cy="catalog-filter-empty"]').should('exist');
    cy.get('[data-cy="catalog-card"]').should('not.exist');
  });

  it('Refresh re-enabled after catalog error', () => {
    cy.forceApiError('GET', `${API()}/api/agents/catalog`, 'catalogFail');
    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@catalogFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('recovers from error to display catalog cards on retry', () => {
    const entry = {
      catalog_id: 'rec-1',
      agent_id: 'assistant',
      name: 'Recovery Agent',
      description: '',
      division: '',
      tags: [],
      enabled: true,
      best_for: '',
      avoid_for: '',
      example: '',
      source: 'native',
      source_label: '',
    };
    failThenRecover(
      '/agent-catalog',
      '/api/agents/catalog',
      { agents: [entry] },
      'catalog-page',
      'catalog-error',
      'catalog-grid',
    );
  });
});

// ─── Approvals ────────────────────────────────────────────────────────────────
describe('Layer 4 — Approvals Failure Behavior', () => {
  it('shows approvals-error when GET /api/work returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/work*`, 'approvalsFail');
    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@approvalsFail');

    cy.get('[data-cy="approvals-error"]').should('exist');
    cy.get('[data-cy="approvals-list"]').should('not.exist');
  });

  it('shows approvals-empty when API returns empty items array', () => {
    cy.intercept('GET', `${API()}/api/work*`, {
      statusCode: 200,
      body: { items: [], total: 0 },
    }).as('approvalsEmpty');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@approvalsEmpty');

    cy.get('[data-cy="approvals-empty"]').should('exist');
    cy.get('[data-cy="approval-card"]').should('not.exist');
  });

  it('Refresh re-enabled after approvals error', () => {
    cy.forceApiError('GET', `${API()}/api/work*`, 'approvalsFail');
    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@approvalsFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('failing approve action shows an error and preserves the card', () => {
    const approval = {
      id: 'appr-approve-fail',
      title: 'Approve: delete_file',
      description: 'Delete /tmp/data.csv',
      work_type: 'approval',
      source: 'agent',
      status: 'waiting_approval',
      approval_status: 'pending',
      assigned_agent_name: 'Assistant',
      created_at: new Date().toISOString(),
      scheduled_at: null,
      retry_count: 0,
      max_retries: 3,
      requires_approval: true,
      payload: {},
      tags: [],
      priority: 128,
    };

    cy.intercept('GET', `${API()}/api/work*`, {
      statusCode: 200,
      body: { items: [approval], total: 1 },
    }).as('getApprovals');

    cy.intercept('POST', `${API()}/api/work/*/approve`, {
      statusCode: 500,
      body: { error: 'Could not process approval' },
    }).as('approveFail');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals');

    cy.get('[data-cy="approval-approve-btn"]').first().click();
    cy.wait('@approveFail');

    // Card must remain — it was NOT actually approved
    cy.get('[data-cy="approval-card"]').should('have.length', 1);
  });
});

// ─── Comms ────────────────────────────────────────────────────────────────────
describe('Layer 4 — Comms Failure Behavior', () => {
  it('shows comms-error when GET /api/comms/topology returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/comms/topology`, 'topoFail');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] });

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@topoFail');

    cy.get('[data-cy="comms-error"]').should('exist');
    cy.get('[data-cy="comms-topology-table"]').should('not.exist');
  });

  it('shows comms-error when GET /api/comms/events returns 500', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`, { statusCode: 200, body: { nodes: [], edges: [] } });
    cy.forceApiError('GET', `${API()}/api/comms/events*`, 'eventsFail');

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@eventsFail');

    // Switch to events tab and check error
    cy.get('[data-cy="comms-tab-events"]').click();
    cy.get('[data-cy="comms-error"]').should('exist');
  });

  it('shows comms-empty-topology when topology nodes array is empty', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`, {
      statusCode: 200,
      body: { nodes: [], edges: [] },
    }).as('emptyTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] });

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@emptyTopo');

    cy.get('[data-cy="comms-empty-topology"]').should('exist');
    cy.get('[data-cy="comms-topology-table"]').should('not.exist');
  });

  it('shows comms-empty-events when events array is empty', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`, { statusCode: 200, body: { nodes: [], edges: [] } });
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] }).as('emptyEvents');

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@emptyEvents');

    cy.get('[data-cy="comms-tab-events"]').click();
    cy.get('[data-cy="comms-empty-events"]').should('exist');
    cy.get('[data-cy="comms-events-table"]').should('not.exist');
  });

  it('Refresh re-enabled after comms error', () => {
    cy.forceApiError('GET', `${API()}/api/comms/topology`, 'topoFail');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] });

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@topoFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });
});

// ─── Workflows ────────────────────────────────────────────────────────────────
describe('Layer 4 — Workflows Failure Behavior', () => {
  it('shows workflows-error when GET /api/workflows returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/workflows`, 'wfFail');
    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@wfFail');

    cy.get('[data-cy="workflows-error"]').should('exist');
    cy.get('[data-cy="workflows-table"]').should('not.exist');
  });

  it('shows workflows-empty when API returns empty array', () => {
    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [] }).as('wfEmpty');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@wfEmpty');

    cy.get('[data-cy="workflows-empty"]').should('exist');
    cy.get('[data-cy="workflow-row"]').should('not.exist');
  });

  it('Refresh re-enabled after workflow error', () => {
    cy.forceApiError('GET', `${API()}/api/workflows`, 'wfFail');
    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@wfFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('failing workflow run shows result badge with error status', () => {
    const wf = {
      id: 'wf-run-fail',
      name: 'Failing Workflow',
      description: '',
      steps: 1,
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [wf] }).as('getWorkflows');
    cy.intercept('POST', `${API()}/api/work`, {
      statusCode: 500,
      body: { error: 'Execution failed' },
    }).as('runFail');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows');

    cy.get('[data-cy="workflow-run-btn"]').first().click();
    cy.wait('@runFail');

    // An error or failed badge must appear
    cy.get('[data-cy="workflow-result-badge"]')
      .should('exist')
      .and(($el) => {
        const text = $el.text().toLowerCase();
        expect(text).to.match(/error|fail|failed/);
      });
  });

  it('recovers from error to display workflows on retry', () => {
    const wf = { id: 'wf-rec', name: 'Recovered', description: '', steps: 1, created_at: new Date().toISOString() };
    failThenRecover('/workflows', '/api/workflows', [wf], 'workflows-page', 'workflows-error', 'workflows-table');
  });
});

// ─── Scheduler ────────────────────────────────────────────────────────────────
describe('Layer 4 — Scheduler Failure Behavior', () => {
  it('shows scheduler-error when GET /api/work returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/work*`, 'schedFail');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@schedFail');

    cy.get('[data-cy="scheduler-error"]').should('exist');
    cy.get('[data-cy="scheduled-list"]').should('not.exist');
  });

  it('shows scheduler-empty when items array is empty', () => {
    cy.intercept('GET', `${API()}/api/work*`, {
      statusCode: 200,
      body: { items: [], total: 0 },
    }).as('emptyScheduled');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@emptyScheduled');

    cy.get('[data-cy="scheduler-empty"]').should('exist');
    cy.get('[data-cy="scheduled-list"]').should('not.exist');
  });

  it('Refresh re-enabled after scheduler error', () => {
    cy.forceApiError('GET', `${API()}/api/work*`, 'schedFail');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@schedFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('recovers from error to display scheduled items on retry', () => {
    const sched = {
      id: 'sched-rec',
      title: 'Recovered Schedule',
      description: '',
      work_type: 'agent_task',
      source: 'api',
      status: 'pending',
      approval_status: 'not_required',
      assigned_agent_name: null,
      created_at: new Date().toISOString(),
      scheduled_at: new Date(Date.now() + 3600000).toISOString(),
      retry_count: 0,
      max_retries: 3,
      requires_approval: false,
      payload: {},
      tags: [],
      priority: 128,
    };
    failThenRecover(
      '/scheduler',
      '/api/work*',
      { items: [sched], total: 1 },
      'scheduler-page',
      'scheduler-error',
      'scheduled-list',
    );
  });
});

// ─── Hands ─────────────────────────────────────────────────────────────────────
describe('Layer 4 — Hands Failure Behavior', () => {
  it('shows hands-error when GET /api/hands returns 500', () => {
    cy.forceApiError('GET', `${API()}/api/hands`, 'handsFail');
    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@handsFail');

    cy.get('[data-cy="hands-error"]').should('exist');
    cy.get('[data-cy="hands-grid"]').should('not.exist');
  });

  it('shows hands-empty when API returns empty hands array', () => {
    cy.intercept('GET', `${API()}/api/hands`, {
      statusCode: 200,
      body: { hands: [], total: 0 },
    }).as('emptyHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@emptyHands');

    cy.get('[data-cy="hands-empty"]').should('exist');
    cy.get('[data-cy="hand-card"]').should('not.exist');
  });

  it('Refresh re-enabled after hands error', () => {
    cy.forceApiError('GET', `${API()}/api/hands`, 'handsFail');
    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@handsFail');

    cy.contains('button', /refresh/i).should('not.be.disabled');
  });

  it('recovers from error to display hand cards on retry', () => {
    const hand = {
      id: 'hand-rec',
      name: 'Recovered Hand',
      description: '',
      category: 'Test',
      icon: '🧪',
      requirements: [],
      requirements_met: true,
      tools: ['test_tool'],
      settings_count: 0,
      has_settings: false,
    };
    failThenRecover(
      '/hands',
      '/api/hands',
      { hands: [hand], total: 1 },
      'hands-page',
      'hands-error',
      'hands-grid',
    );
  });
});
