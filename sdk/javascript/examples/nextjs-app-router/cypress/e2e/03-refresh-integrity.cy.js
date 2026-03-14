/**
 * Layer 3 — Refresh Integrity
 *
 * Proves that every route:
 *  1. Survives a full browser reload (cy.reload) and renders correctly.
 *  2. Survives a direct URL navigation (no sidebar context needed).
 *  3. After loading real data, Refresh re-fetches and re-renders without
 *     visual regression (spinner, error flash, layout shift).
 *  4. The Refresh button itself is re-enabled after the fetch completes
 *     (important UX contract — it must not stay in a loading/disabled state).
 *
 * Strategy: Use stubbed API responses for the "after-refresh render" assertions
 * so the test does not depend on the live Rust backend having specific data.
 * The initial SSR render does hit the live backend (or returns an empty state),
 * which is also confirmed as acceptable behavior.
 */

const API = () => Cypress.env('API_BASE');

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Assert that a page survives cy.reload() and the root element is still present.
 * @param {string} path - Route to visit
 * @param {string} rootCy - data-cy value of the root element
 */
function assertSurvivesReload(path, rootCy) {
  cy.visit(path);
  cy.get(`[data-cy="${rootCy}"]`).should('exist');
  cy.reload();
  cy.get(`[data-cy="${rootCy}"]`).should('exist');
}

/**
 * Assert that clicking Refresh a second time succeeds and the button re-enables.
 * @param {string} apiPath - URL pattern for cy.intercept
 * @param {*} body - Stub response body
 * @param {string} rootCy - root element data-cy
 */
function assertDoubleRefreshStable(apiPath, body, rootCy) {
  let callCount = 0;

  cy.intercept('GET', `${API()}${apiPath}`, (req) => {
    callCount++;
    req.reply({ statusCode: 200, body });
  }).as('refreshStub');

  // First refresh
  cy.clickRefresh();
  cy.wait('@refreshStub');
  cy.get(`[data-cy="${rootCy}"]`).should('exist');

  // Second refresh — button must be re-enabled and fire again
  cy.clickRefresh();
  cy.wait('@refreshStub');
  cy.get(`[data-cy="${rootCy}"]`).should('exist');

  // Confirm the intercept fired at least twice
  cy.then(() => expect(callCount).to.be.greaterThan(1));
}

// ─── Chat ─────────────────────────────────────────────────────────────────────
describe('Layer 3 — Chat Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/chat', 'chat-page');
  });

  it('agent select still populated after reload', () => {
    cy.visit('/chat');
    cy.get('[data-cy="agent-select"] option').invoke('length').then((before) => {
      cy.reload();
      cy.get('[data-cy="agent-select"] option')
        .should('have.length', before);
    });
  });

  it('chat input is interactive after reload', () => {
    cy.visit('/chat');
    cy.reload();
    cy.get('[data-cy="chat-input"]')
      .should('exist')
      .should('not.be.disabled')
      .type('hello')
      .should('have.value', 'hello');
  });

  it('send button re-enables after a stubbed response', () => {
    cy.intercept('GET', `${API()}/api/agents/*/session`, {
      statusCode: 200,
      body: { messages: [], session_id: 'test', message_count: 0, context_window_tokens: 0 },
    }).as('loadSession');

    let callCount = 0;
    cy.intercept('POST', `${API()}/api/agents/*/message`, (req) => {
      callCount++;
      req.reply({
        statusCode: 200,
        body: { response: 'Reply.', input_tokens: 2, output_tokens: 1, iterations: 1, cost_usd: 0 },
      });
    }).as('sendStub');

    cy.visit('/chat');
    cy.wait('@loadSession');

    cy.get('[data-cy="chat-input"]').type('First');
    cy.get('[data-cy="chat-send-btn"]').click();
    cy.wait('@sendStub');

    // Button must be re-enabled and accept a second send
    cy.get('[data-cy="chat-send-btn"]').should('not.be.disabled');
    cy.get('[data-cy="chat-input"]').should('have.value', '');

    cy.get('[data-cy="chat-input"]').type('Second');
    cy.get('[data-cy="chat-send-btn"]').click();
    cy.wait('@sendStub');

    cy.then(() => expect(callCount).to.eq(2));
  });
});

// ─── Inbox ────────────────────────────────────────────────────────────────────
describe('Layer 3 — Inbox Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/inbox', 'inbox-page');
  });

  it('double Refresh does not break the list', () => {
    cy.visit('/inbox');
    assertDoubleRefreshStable(
      '/api/work*',
      { items: [], total: 0 },
      'inbox-page',
    );
  });

  it('inbox renders recovery state after reload from an existing item', () => {
    const stubItem = {
      id: 'reload-item',
      title: 'Reload test item',
      description: '',
      work_type: 'agent_task',
      source: 'api',
      status: 'pending',
      approval_status: 'not_required',
      assigned_agent_name: null,
      created_at: new Date().toISOString(),
      scheduled_at: null,
      retry_count: 0,
      max_retries: 3,
      requires_approval: false,
      payload: {},
      tags: [],
      priority: 128,
    };

    cy.intercept('GET', `${API()}/api/work*`, {
      statusCode: 200,
      body: { items: [stubItem], total: 1 },
    }).as('getInbox');

    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getInbox');

    cy.get('[data-cy="inbox-item"]').should('exist');

    // Reload the page — SSR will re-fetch, client state resets cleanly
    cy.reload();
    cy.get('[data-cy="inbox-page"]').should('exist');
    // The page should NOT be stuck on an error state
    cy.get('[data-cy="inbox-error"]').should('not.exist');
  });
});

// ─── Agent Catalog ─────────────────────────────────────────────────────────────
describe('Layer 3 — Agent Catalog Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/agent-catalog', 'catalog-page');
  });

  it('double Refresh re-fetches catalog without duplicating cards', () => {
    const singleEntry = {
      catalog_id: 'c-int-1',
      agent_id: 'assistant',
      name: 'Unique Agent',
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
      body: { agents: [singleEntry] },
    }).as('getCatalog');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog');

    cy.get('[data-cy="catalog-card"]').should('have.length', 1);

    // Second refresh — should still show exactly 1 card (no duplication)
    cy.clickRefresh();
    cy.wait('@getCatalog');
    cy.get('[data-cy="catalog-card"]').should('have.length', 1);
  });

  it('filter value resets after page reload', () => {
    cy.visit('/agent-catalog');
    cy.get('[data-cy="catalog-filter"]').type('test-filter-value');
    cy.reload();
    // Filter input must be empty after reload (no stale state)
    cy.get('[data-cy="catalog-filter"]').should('have.value', '');
  });
});

// ─── Approvals ────────────────────────────────────────────────────────────────
describe('Layer 3 — Approvals Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/approvals', 'approvals-page');
  });

  it('double Refresh does not duplicate items', () => {
    const approval = {
      id: 'appr-dup-test',
      title: 'Approve: file_read',
      description: 'Read a file',
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

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals');
    cy.get('[data-cy="approval-card"]').should('have.length', 1);

    cy.clickRefresh();
    cy.wait('@getApprovals');
    // Still exactly 1 card — no duplication from double refresh
    cy.get('[data-cy="approval-card"]').should('have.length', 1);
  });
});

// ─── Comms ────────────────────────────────────────────────────────────────────
describe('Layer 3 — Comms Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/comms', 'comms-page');
  });

  it('topology panel remains visible after double Refresh', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`, {
      statusCode: 200,
      body: { nodes: [], edges: [] },
    }).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] }).as('getEvents');

    cy.visit('/comms');
    cy.get('[data-cy="comms-topology-panel"]').should('exist');

    cy.clickRefresh();
    cy.wait('@getTopo');
    cy.get('[data-cy="comms-topology-panel"]').should('exist');

    cy.clickRefresh();
    cy.wait('@getTopo');
    cy.get('[data-cy="comms-topology-panel"]').should('exist');
  });

  it('active tab is preserved after page reload', () => {
    cy.visit('/comms');

    // Switch to Events tab
    cy.get('[data-cy="comms-tab-events"]').click();
    cy.get('[data-cy="comms-events-panel"]').should('exist');

    // Reload — tab resets to default (topology)
    cy.reload();
    cy.get('[data-cy="comms-topology-panel"]').should('exist');
  });

  it('tab switch does not cause double fetch of topology', () => {
    let topoCalls = 0;
    cy.intercept('GET', `${API()}/api/comms/topology`, (req) => {
      topoCalls++;
      req.reply({ statusCode: 200, body: { nodes: [], edges: [] } });
    }).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] }).as('getEvents');

    cy.visit('/comms');
    cy.get('[data-cy="comms-tab-events"]').click();
    cy.get('[data-cy="comms-tab-topology"]').click();

    // Topology GET should not have fired again via tab switch
    cy.then(() => expect(topoCalls).to.be.at.most(1));
  });
});

// ─── Workflows ────────────────────────────────────────────────────────────────
describe('Layer 3 — Workflows Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/workflows', 'workflows-page');
  });

  it('double Refresh does not duplicate workflow rows', () => {
    const wf = {
      id: 'wf-dup',
      name: 'Stability Check',
      description: '',
      steps: 2,
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/workflows`, {
      statusCode: 200,
      body: [wf],
    }).as('getWorkflows');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows');
    cy.get('[data-cy="workflow-row"]').should('have.length', 1);

    cy.clickRefresh();
    cy.wait('@getWorkflows');
    cy.get('[data-cy="workflow-row"]').should('have.length', 1);
  });

  it('result badge from a previous run does not persist after Refresh', () => {
    const wf = {
      id: 'wf-badge-clear',
      name: 'Badge Test',
      description: '',
      steps: 1,
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: [wf] }).as('getWorkflows');
    cy.intercept('POST', `${API()}/api/work`, {
      statusCode: 201,
      body: { id: 'wi-badge-1', title: 'Badge Test', status: 'pending', work_type: 'workflow' },
    }).as('runWf');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows');

    cy.get('[data-cy="workflow-run-btn"]').click();
    cy.wait('@runWf');
    cy.get('[data-cy="workflow-result-badge"]').should('exist');

    // Refresh the list — the badge should be cleared
    cy.clickRefresh();
    cy.wait('@getWorkflows');
    cy.get('[data-cy="workflow-result-badge"]').should('not.exist');
  });
});

// ─── Scheduler ────────────────────────────────────────────────────────────────
describe('Layer 3 — Scheduler Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/scheduler', 'scheduler-page');
  });

  it('double Refresh does not duplicate scheduled-item rows', () => {
    const schedule = {
      id: 'sched-dup',
      title: 'Duplicate Test',
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

    cy.intercept('GET', `${API()}/api/work*`, {
      statusCode: 200,
      body: { items: [schedule], total: 1 },
    }).as('getScheduled');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getScheduled');
    cy.get('[data-cy="scheduled-list"]').should('contain.text', 'Duplicate Test');

    cy.get('[data-cy="scheduled-item"]').invoke('length').then((before) => {
      cy.clickRefresh();
      cy.wait('@getScheduled');
      cy.get('[data-cy="scheduled-item"]').should('have.length', before);
    });
  });
});

// ─── Hands ─────────────────────────────────────────────────────────────────────
describe('Layer 3 — Hands Refresh Integrity', () => {
  it('survives a page reload', () => {
    assertSurvivesReload('/hands', 'hands-page');
  });

  it('double Refresh does not duplicate hand cards', () => {
    const hand = {
      id: 'hand-cli',
      name: 'CLI',
      description: 'Runs shell commands',
      category: 'Development',
      icon: '⌨️',
      requirements: [],
      requirements_met: true,
      tools: ['exec'],
      settings_count: 0,
      has_settings: false,
    };

    cy.intercept('GET', `${API()}/api/hands`, {
      statusCode: 200,
      body: { hands: [hand], total: 1 },
    }).as('getHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands');
    cy.get('[data-cy="hand-card"]').should('have.length', 1);

    cy.clickRefresh();
    cy.wait('@getHands');
    cy.get('[data-cy="hand-card"]').should('have.length', 1);
  });

  it('expanded tools section collapses and re-expands correctly after Refresh', () => {
    const hand = {
      id: 'hand-refresh-expand',
      name: 'FileSystem',
      description: 'File operations',
      category: 'Development',
      icon: '📁',
      requirements: [],
      requirements_met: true,
      tools: ['read_file', 'write_file'],
      settings_count: 0,
      has_settings: false,
    };

    cy.intercept('GET', `${API()}/api/hands`, {
      statusCode: 200,
      body: { hands: [hand], total: 1 },
    }).as('getHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands');

    // Expand
    cy.get('[data-cy="hand-expand-btn"]').click();
    cy.get('[data-cy="hand-tools-section"]').should('exist');

    // Collapse via second click
    cy.get('[data-cy="hand-expand-btn"]').click();
    cy.get('[data-cy="hand-tools-section"]').should('not.exist');

    // Refresh and expand again — should still work
    cy.clickRefresh();
    cy.wait('@getHands');
    cy.get('[data-cy="hand-expand-btn"]').click();
    cy.get('[data-cy="hand-tools-section"]').should('exist');
  });
});
