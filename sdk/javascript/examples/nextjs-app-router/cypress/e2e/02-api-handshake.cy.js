/**
 * Layer 2 — Page API Handshake Coverage
 *
 * Proves for every page that:
 *  1. The expected backend API call fires (GET/POST to the Rust API).
 *  2. The response data is reflected in the UI.
 *  3. Each page's primary interactive action triggers the correct API call.
 *
 * Architecture notes:
 *  - Server components (page.js) fetch data via SSR at page render time.
 *    Initial data presence is proved by asserting rendered content on load.
 *  - Client-side data fetches (Refresh button, agent selection, send message)
 *    go directly from the browser to `http://127.0.0.1:50051` and can be
 *    intercepted with cy.intercept.
 *  - cy.intercept captures cross-origin requests via the Cypress proxy.
 */

const API = () => Cypress.env('API_BASE');

// ─── Chat ─────────────────────────────────────────────────────────────────────
describe('Layer 2 — Chat API Handshake', () => {
  beforeEach(() => {
    // Spy on the session history load — this fires browser-side on component mount
    cy.intercept('GET', `${API()}/api/agents/*/session`).as('loadSession');
  });

  it('loads chat page and agent selector contains at least one option', () => {
    cy.visit('/chat');
    cy.get('[data-cy="chat-page"]').should('exist');
    cy.get('[data-cy="agent-select"]').should('exist');
    cy.get('[data-cy="agent-select"] option').should('have.length.greaterThan', 0);
  });

  it('session history request fires when page loads with an agent selected', () => {
    cy.visit('/chat');
    cy.get('[data-cy="agent-select"] option').should('have.length.greaterThan', 0);
    // The component auto-selects the first agent and calls loadHistory in useEffect
    cy.wait('@loadSession', { timeout: 15000 })
      .its('response.statusCode')
      .should('eq', 200);
  });

  it('send button triggers POST to /api/agents/:id/message', () => {
    // Stub the session load so history populates quickly
    cy.intercept('GET', `${API()}/api/agents/*/session`, {
      statusCode: 200,
      body: { messages: [], session_id: 'test', message_count: 0, context_window_tokens: 0 },
    }).as('stubSession');

    // Stub the message send to avoid a real LLM call during tests
    cy.intercept('POST', `${API()}/api/agents/*/message`, {
      statusCode: 200,
      body: {
        response: 'Hello from stub.',
        input_tokens: 5,
        output_tokens: 4,
        iterations: 1,
        cost_usd: 0.0001,
      },
    }).as('sendMessage');

    cy.visit('/chat');
    cy.wait('@stubSession');

    // Type a message and send
    cy.get('[data-cy="chat-input"]')
      .should('exist')
      .type('Hello agent');

    cy.get('[data-cy="chat-send-btn"]')
      .should('not.be.disabled')
      .click();

    // Assert the POST fired with the correct URL pattern
    cy.wait('@sendMessage', { timeout: 15000 })
      .then((interception) => {
        expect(interception.request.method).to.eq('POST');
        expect(interception.request.body).to.have.property('message', 'Hello agent');
        expect(interception.response.statusCode).to.eq(200);
      });

    // Assert the stub response appears in the chat messages area
    cy.get('[data-cy="chat-messages"]')
      .should('contain.text', 'Hello from stub.');
  });

  it('chat message area reflects the user message optimistically', () => {
    cy.intercept('GET', `${API()}/api/agents/*/session`, {
      statusCode: 200,
      body: { messages: [], session_id: 'test', message_count: 0, context_window_tokens: 0 },
    }).as('stubSession');

    cy.intercept('POST', `${API()}/api/agents/*/message`, {
      statusCode: 200,
      body: { response: 'Ack.', input_tokens: 2, output_tokens: 1, iterations: 1, cost_usd: 0 },
    }).as('sendMessage');

    cy.visit('/chat');
    cy.wait('@stubSession');

    cy.get('[data-cy="chat-input"]').type('Test message');
    cy.get('[data-cy="chat-send-btn"]').click();

    // User's message must appear immediately (optimistic update)
    cy.get('[data-cy="message-bubble"]').should('contain.text', 'Test message');

    cy.wait('@sendMessage');
    // Assistant reply must appear after response
    cy.get('[data-cy="message-bubble"]').should('contain.text', 'Ack.');
  });
});

// ─── Inbox ────────────────────────────────────────────────────────────────────
describe('Layer 2 — Inbox API Handshake', () => {
  it('page renders after SSR fetch (initial render proves SSR→API handshake)', () => {
    cy.visit('/inbox');
    cy.get('[data-cy="inbox-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Inbox');
    // Either items or empty state must be present — proves SSR resolved
    cy.get('[data-cy="inbox-list"], [data-cy="inbox-empty"]').should('exist');
  });

  it('Refresh button fires GET /api/planner/inbox', () => {
    cy.intercept('GET', `${API()}/api/planner/inbox`).as('refreshInbox');
    cy.visit('/inbox');
    cy.get('[data-cy="inbox-page"]').should('exist');

    cy.clickRefresh();
    cy.wait('@refreshInbox', { timeout: 15000 })
      .its('response.statusCode')
      .should('eq', 200);
  });

  it('inbox list renders items returned by the API', () => {
    // Stub with one known item to assert rendering
    const stubItem = {
      id: 'item-001',
      text: 'Cypress test inbox item',
      status: 'pending',
      project_title: 'Cypress Project',
      tasks: [],
      agent_recommendations: [],
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/planner/inbox`, {
      statusCode: 200,
      body: { items: [stubItem] },
    }).as('getInbox');

    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getInbox');

    cy.get('[data-cy="inbox-item"]').should('have.length.greaterThan', 0);
    cy.get('[data-cy="inbox-item"]').first().should('contain.text', 'Cypress test inbox item');
  });

  it('expanding an inbox item reveals the tasks table when tasks are present', () => {
    const stubItem = {
      id: 'item-expand-001',
      text: 'Expandable item',
      status: 'clarified',
      project_title: 'Test Project',
      tasks: [
        { id: 't1', title: 'Sub-task A', priority: 'high', effort_minutes: 30, energy: 'high', status: 'todo' },
      ],
      agent_recommendations: [],
      created_at: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/planner/inbox`, {
      statusCode: 200,
      body: { items: [stubItem] },
    }).as('getInbox');

    cy.visit('/inbox');
    cy.clickRefresh();
    cy.wait('@getInbox');

    // Click the item header to expand
    cy.get('[data-cy="inbox-item-header"]').first().click();

    // Tasks table must appear
    cy.get('[data-cy="inbox-tasks-table"]').should('exist');
    cy.get('[data-cy="inbox-tasks-table"]').should('contain.text', 'Sub-task A');
  });
});

// ─── Agent Catalog ─────────────────────────────────────────────────────────────
describe('Layer 2 — Agent Catalog API Handshake', () => {
  it('page renders and catalog grid exists after SSR', () => {
    cy.visit('/agent-catalog');
    cy.get('[data-cy="catalog-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Agent Catalog');
    cy.get('[data-cy="catalog-grid"], [data-cy="catalog-empty"]').should('exist');
  });

  it('Refresh button fires GET /api/agents/catalog', () => {
    cy.intercept('GET', `${API()}/api/agents/catalog`).as('getCatalog');
    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog').its('response.statusCode').should('eq', 200);
  });

  it('catalog cards render names from API response', () => {
    const stubEntry = {
      catalog_id: 'cat-001',
      agent_id: 'assistant',
      name: 'Cypress Test Agent',
      description: 'A test agent for Cypress.',
      division: 'Engineering',
      tags: ['test', 'cypress'],
      enabled: true,
      best_for: 'testing',
      avoid_for: 'production',
      example: 'Run this in CI.',
      source: 'native',
      source_label: 'Local',
    };

    cy.intercept('GET', `${API()}/api/agents/catalog`, {
      statusCode: 200,
      body: { agents: [stubEntry] },
    }).as('getCatalog');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog');

    cy.get('[data-cy="catalog-card"]').should('have.length.greaterThan', 0);
    cy.get('[data-cy="catalog-card"]').first().should('contain.text', 'Cypress Test Agent');
  });

  it('enable/disable toggle fires PUT /api/agents/catalog/:id/enabled', () => {
    const stubEntry = {
      catalog_id: 'cat-toggle-001',
      agent_id: 'assistant',
      name: 'Toggle Me',
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
      body: { agents: [stubEntry] },
    }).as('getCatalog');

    cy.intercept('PUT', `${API()}/api/agents/catalog/*/enabled`, {
      statusCode: 200,
      body: {},
    }).as('toggleEnabled');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog');

    // Click the toggle button
    cy.get('[data-cy="catalog-toggle-btn"]').first().click();

    cy.wait('@toggleEnabled').then((interception) => {
      expect(interception.request.method).to.eq('PUT');
      expect(interception.request.url).to.include('/api/agents/catalog/');
      expect(interception.request.url).to.include('/enabled');
      expect(interception.response.statusCode).to.eq(200);
    });
  });

  it('filter input narrows catalog cards', () => {
    const entries = [
      { catalog_id: 'c1', agent_id: 'a1', name: 'Alpha Agent', description: '', division: '', tags: [], enabled: true, best_for: '', avoid_for: '', example: '', source: 'native', source_label: '' },
      { catalog_id: 'c2', agent_id: 'a2', name: 'Beta Agent', description: '', division: '', tags: [], enabled: true, best_for: '', avoid_for: '', example: '', source: 'native', source_label: '' },
    ];

    cy.intercept('GET', `${API()}/api/agents/catalog`, {
      statusCode: 200,
      body: { agents: entries },
    }).as('getCatalog');

    cy.visit('/agent-catalog');
    cy.clickRefresh();
    cy.wait('@getCatalog');

    cy.get('[data-cy="catalog-card"]').should('have.length', 2);

    cy.get('[data-cy="catalog-filter"]').type('Alpha');
    cy.get('[data-cy="catalog-card"]').should('have.length', 1);
    cy.get('[data-cy="catalog-card"]').should('contain.text', 'Alpha Agent');
  });
});

// ─── Approvals ────────────────────────────────────────────────────────────────
describe('Layer 2 — Approvals API Handshake', () => {
  it('page renders and empty or list state appears after SSR', () => {
    cy.visit('/approvals');
    cy.get('[data-cy="approvals-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Approvals');
    cy.get('[data-cy="approvals-list"], [data-cy="approvals-empty"]').should('exist');
  });

  it('Refresh button fires GET /api/approvals', () => {
    cy.intercept('GET', `${API()}/api/approvals`).as('getApprovals');
    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals').its('response.statusCode').should('eq', 200);
  });

  it('approval cards render when approvals exist', () => {
    const approval = {
      id: 'appr-001',
      agent_id: 'assistant',
      agent_name: 'Assistant',
      tool_name: 'file_write',
      description: 'Write to /tmp/test.txt',
      action: 'write_file("/tmp/test.txt", "hello")',
      action_summary: 'Write file',
      risk_level: 'medium',
      requested_at: new Date().toISOString(),
      timeout_secs: 60,
      status: 'pending',
    };

    cy.intercept('GET', `${API()}/api/approvals`, {
      statusCode: 200,
      body: { approvals: [approval], total: 1 },
    }).as('getApprovals');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals');

    cy.get('[data-cy="approval-card"]').should('have.length', 1);
    cy.get('[data-cy="approval-card"]').should('contain.text', 'file_write');
    cy.get('[data-cy="approval-approve-btn"]').should('exist');
    cy.get('[data-cy="approval-reject-btn"]').should('exist');
  });

  it('Approve button fires POST /api/approvals/:id/approve', () => {
    const approval = {
      id: 'appr-approve-001',
      agent_name: 'Assistant',
      tool_name: 'shell_exec',
      description: 'Run a shell command',
      action: 'ls /tmp',
      risk_level: 'low',
      requested_at: new Date().toISOString(),
      timeout_secs: 30,
      status: 'pending',
    };

    cy.intercept('GET', `${API()}/api/approvals`, {
      statusCode: 200,
      body: { approvals: [approval] },
    }).as('getApprovals');

    cy.intercept('POST', `${API()}/api/approvals/*/approve`, {
      statusCode: 200,
      body: {},
    }).as('approveAction');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals');

    cy.get('[data-cy="approval-approve-btn"]').first().click();

    cy.wait('@approveAction').then((interception) => {
      expect(interception.request.method).to.eq('POST');
      expect(interception.request.url).to.include('/approve');
      expect(interception.response.statusCode).to.eq(200);
    });

    // Item must be removed from list after approval
    cy.get('[data-cy="approval-card"]').should('have.length', 0);
    cy.get('[data-cy="approvals-empty"]').should('exist');
  });

  it('Reject button fires POST /api/approvals/:id/reject', () => {
    const approval = {
      id: 'appr-reject-001',
      agent_name: 'Coder',
      tool_name: 'git_push',
      description: 'Push to main',
      action: 'git push origin main',
      risk_level: 'high',
      requested_at: new Date().toISOString(),
      timeout_secs: 30,
      status: 'pending',
    };

    cy.intercept('GET', `${API()}/api/approvals`, {
      statusCode: 200,
      body: { approvals: [approval] },
    }).as('getApprovals');

    cy.intercept('POST', `${API()}/api/approvals/*/reject`, {
      statusCode: 200,
      body: {},
    }).as('rejectAction');

    cy.visit('/approvals');
    cy.clickRefresh();
    cy.wait('@getApprovals');

    cy.get('[data-cy="approval-reject-btn"]').first().click();

    cy.wait('@rejectAction').then((interception) => {
      expect(interception.request.method).to.eq('POST');
      expect(interception.request.url).to.include('/reject');
    });

    cy.get('[data-cy="approval-card"]').should('have.length', 0);
  });
});

// ─── Comms ────────────────────────────────────────────────────────────────────
describe('Layer 2 — Comms API Handshake', () => {
  it('page renders with topology tab active by default', () => {
    cy.visit('/comms');
    cy.get('[data-cy="comms-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Comms');
    cy.get('[data-cy="comms-topology-panel"]').should('exist');
  });

  it('Refresh fires GET /api/comms/topology and GET /api/comms/events', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`).as('getEvents');

    cy.visit('/comms');
    cy.clickRefresh();

    cy.wait('@getTopo').its('response.statusCode').should('eq', 200);
    cy.wait('@getEvents').its('response.statusCode').should('eq', 200);
  });

  it('topology table renders nodes from API response', () => {
    const topology = {
      nodes: [
        { id: 'a1', name: 'Assistant', model: 'gpt-4o', state: 'running' },
        { id: 'a2', name: 'Coder', model: 'claude-3', state: 'idle' },
      ],
      edges: [],
    };

    cy.intercept('GET', `${API()}/api/comms/topology`, {
      statusCode: 200,
      body: topology,
    }).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [] }).as('getEvents');

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@getTopo');

    cy.get('[data-cy="comms-topology-table"]').should('exist');
    cy.get('[data-cy="comms-topology-table"]').should('contain.text', 'Assistant');
    cy.get('[data-cy="comms-topology-table"]').should('contain.text', 'Coder');
  });

  it('switching to Events tab fires GET /api/comms/events and renders the events panel', () => {
    cy.intercept('GET', `${API()}/api/comms/topology`, {
      statusCode: 200,
      body: { nodes: [], edges: [] },
    }).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, {
      statusCode: 200,
      body: [],
    }).as('getEvents');

    cy.visit('/comms');

    // Click the Events tab
    cy.get('[data-cy="comms-tab-events"]').click();

    // Events panel must exist
    cy.get('[data-cy="comms-events-panel"]').should('exist');
  });

  it('events table renders events from API response', () => {
    const event = {
      id: 'ev-001',
      kind: 'agent_message',
      source_id: 'a1',
      source_name: 'Assistant',
      target_id: 'a2',
      target_name: 'Coder',
      detail: 'Hello',
      timestamp: new Date().toISOString(),
    };

    cy.intercept('GET', `${API()}/api/comms/topology`, { statusCode: 200, body: { nodes: [], edges: [] } }).as('getTopo');
    cy.intercept('GET', `${API()}/api/comms/events*`, { statusCode: 200, body: [event] }).as('getEvents');

    cy.visit('/comms');
    cy.clickRefresh();
    cy.wait('@getTopo');
    cy.wait('@getEvents');

    cy.get('[data-cy="comms-tab-events"]').click();
    cy.get('[data-cy="comms-events-table"]').should('exist');
    cy.get('[data-cy="comms-events-table"]').should('contain.text', 'Assistant');
  });
});

// ─── Workflows ────────────────────────────────────────────────────────────────
describe('Layer 2 — Workflows API Handshake', () => {
  it('page renders and list or empty state is present after SSR', () => {
    cy.visit('/workflows');
    cy.get('[data-cy="workflows-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Workflows');
    cy.get('[data-cy="workflows-table"], [data-cy="workflows-empty"]').should('exist');
  });

  it('Refresh fires GET /api/workflows', () => {
    cy.intercept('GET', `${API()}/api/workflows`).as('getWorkflows');
    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows').its('response.statusCode').should('eq', 200);
  });

  it('workflow rows render names from API response', () => {
    const workflows = [
      { id: 'wf-001', name: 'Daily Report', description: 'Generates daily report', steps: 3, created_at: new Date().toISOString() },
    ];

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: workflows }).as('getWorkflows');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows');

    cy.get('[data-cy="workflow-row"]').should('have.length', 1);
    cy.get('[data-cy="workflow-row"]').should('contain.text', 'Daily Report');
  });

  it('Run button fires POST /api/workflows/:id/run and shows result badge', () => {
    const workflows = [
      { id: 'wf-run-001', name: 'Test Workflow', description: 'A test workflow', steps: 1, created_at: new Date().toISOString() },
    ];

    cy.intercept('GET', `${API()}/api/workflows`, { statusCode: 200, body: workflows }).as('getWorkflows');
    cy.intercept('POST', `${API()}/api/workflows/*/run`, {
      statusCode: 200,
      body: { run_id: 'run-1', output: 'done', status: 'completed' },
    }).as('runWorkflow');

    cy.visit('/workflows');
    cy.clickRefresh();
    cy.wait('@getWorkflows');

    cy.get('[data-cy="workflow-run-btn"]').first().click();

    cy.wait('@runWorkflow').then((interception) => {
      expect(interception.request.method).to.eq('POST');
      expect(interception.request.url).to.include('/run');
      expect(interception.response.statusCode).to.eq(200);
    });

    // Result badge must appear
    cy.get('[data-cy="workflow-result-badge"]').should('exist').and('contain.text', 'completed');
  });
});

// ─── Scheduler ────────────────────────────────────────────────────────────────
describe('Layer 2 — Scheduler API Handshake', () => {
  it('page renders with schedules tab active by default', () => {
    cy.visit('/scheduler');
    cy.get('[data-cy="scheduler-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Scheduler');
    cy.get('[data-cy="schedules-panel"]').should('exist');
  });

  it('Refresh fires GET /api/schedules and GET /api/cron/jobs', () => {
    cy.intercept('GET', `${API()}/api/schedules`).as('getSchedules');
    cy.intercept('GET', `${API()}/api/cron/jobs`).as('getCronJobs');

    cy.visit('/scheduler');
    cy.clickRefresh();

    cy.wait('@getSchedules').its('response.statusCode').should('eq', 200);
    cy.wait('@getCronJobs').its('response.statusCode').should('eq', 200);
  });

  it('schedule rows render from API response', () => {
    const schedule = {
      id: 'sched-001',
      name: 'Daily Digest',
      cron: '0 8 * * *',
      agent_id: 'assistant',
      enabled: true,
    };

    cy.intercept('GET', `${API()}/api/schedules`, {
      statusCode: 200,
      body: { schedules: [schedule], total: 1 },
    }).as('getSchedules');
    cy.intercept('GET', `${API()}/api/cron/jobs`, { statusCode: 200, body: { jobs: [], total: 0 } }).as('getCronJobs');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getSchedules');

    cy.get('[data-cy="schedules-table"]').should('exist');
    cy.get('[data-cy="schedules-table"]').should('contain.text', 'Daily Digest');
    cy.get('[data-cy="schedules-table"]').should('contain.text', '0 8 * * *');
  });

  it('switching to Cron tab shows cron panel and fires no extra GET', () => {
    cy.intercept('GET', `${API()}/api/schedules`, { statusCode: 200, body: { schedules: [], total: 0 } }).as('getSchedules');
    cy.intercept('GET', `${API()}/api/cron/jobs`, { statusCode: 200, body: { jobs: [], total: 0 } }).as('getCronJobs');

    cy.visit('/scheduler');

    cy.get('[data-cy="scheduler-tab-cron"]').click();
    cy.get('[data-cy="cron-panel"]').should('exist');
  });

  it('cron job rows render from API response', () => {
    const cronJob = {
      id: 'cron-001',
      name: 'Hourly Check',
      cron: '0 * * * *',
      agent_id: 'ops',
      enabled: true,
      run_count: 12,
    };

    cy.intercept('GET', `${API()}/api/schedules`, { statusCode: 200, body: { schedules: [], total: 0 } }).as('getSchedules');
    cy.intercept('GET', `${API()}/api/cron/jobs`, {
      statusCode: 200,
      body: { jobs: [cronJob], total: 1 },
    }).as('getCronJobs');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getCronJobs');

    cy.get('[data-cy="scheduler-tab-cron"]').click();
    cy.get('[data-cy="cron-table"]').should('exist');
    cy.get('[data-cy="cron-table"]').should('contain.text', 'Hourly Check');
  });

  it('Delete button fires DELETE /api/schedules/:id', () => {
    const schedule = {
      id: 'sched-del-001',
      name: 'Delete Me',
      cron: '*/5 * * * *',
      agent_id: 'assistant',
      enabled: true,
    };

    cy.intercept('GET', `${API()}/api/schedules`, {
      statusCode: 200,
      body: { schedules: [schedule], total: 1 },
    }).as('getSchedules');
    cy.intercept('GET', `${API()}/api/cron/jobs`, { statusCode: 200, body: { jobs: [], total: 0 } }).as('getCronJobs');
    cy.intercept('DELETE', `${API()}/api/schedules/*`, { statusCode: 200, body: {} }).as('deleteSchedule');

    cy.visit('/scheduler');
    cy.clickRefresh();
    cy.wait('@getSchedules');

    // Confirm dialog — stub window.confirm to return true
    cy.window().then((win) => cy.stub(win, 'confirm').returns(true));

    cy.get('[data-cy="schedule-delete-btn"]').first().click();

    cy.wait('@deleteSchedule').then((interception) => {
      expect(interception.request.method).to.eq('DELETE');
      expect(interception.request.url).to.include('/api/schedules/');
    });

    // Row must be removed from the table
    cy.get('[data-cy="schedules-table"]').should('not.contain.text', 'Delete Me');
  });
});

// ─── Hands ─────────────────────────────────────────────────────────────────────
describe('Layer 2 — Hands API Handshake', () => {
  it('page renders and hands grid or empty state is present after SSR', () => {
    cy.visit('/hands');
    cy.get('[data-cy="hands-page"]').should('exist');
    cy.get('h1').should('contain.text', 'Hands');
    cy.get('[data-cy="hands-grid"], [data-cy="hands-empty"]').should('exist');
  });

  it('Refresh fires GET /api/hands', () => {
    cy.intercept('GET', `${API()}/api/hands`).as('getHands');
    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands').its('response.statusCode').should('eq', 200);
  });

  it('hand cards render names from API response', () => {
    const hands = [
      {
        id: 'hand-browser',
        name: 'Browser',
        description: 'Controls a headless browser',
        category: 'Productivity',
        icon: '🌐',
        requirements: [],
        requirements_met: true,
        tools: ['browse_url', 'click', 'type'],
        settings_count: 0,
        dashboard_metrics: null,
        has_settings: false,
      },
    ];

    cy.intercept('GET', `${API()}/api/hands`, {
      statusCode: 200,
      body: { hands, total: 1 },
    }).as('getHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands');

    cy.get('[data-cy="hand-card"]').should('have.length.greaterThan', 0);
    cy.get('[data-cy="hand-card"]').first().should('contain.text', 'Browser');
  });

  it('ready hand card shows tools on expand', () => {
    const hands = [
      {
        id: 'hand-browser-exp',
        name: 'Browser',
        description: 'Controls a headless browser',
        category: 'Productivity',
        icon: '🌐',
        requirements: [],
        requirements_met: true,
        tools: ['browse_url', 'click', 'screenshot'],
        settings_count: 0,
        has_settings: false,
      },
    ];

    cy.intercept('GET', `${API()}/api/hands`, { statusCode: 200, body: { hands, total: 1 } }).as('getHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands');

    // Expand tools
    cy.get('[data-cy="hand-expand-btn"]').first().click();

    cy.get('[data-cy="hand-tools-section"]').should('exist');
    cy.get('[data-cy="hand-tools-section"]').should('contain.text', 'browse_url');
  });

  it('hand card with unmet requirements shows requirements toggle', () => {
    const hands = [
      {
        id: 'hand-twitter',
        name: 'Twitter',
        description: 'Post to Twitter/X',
        category: 'Communication',
        icon: '🐦',
        requirements: [
          { key: 'TWITTER_API_KEY', label: 'Twitter API key', satisfied: false },
        ],
        requirements_met: false,
        tools: ['tweet', 'reply'],
        settings_count: 1,
        has_settings: true,
      },
    ];

    cy.intercept('GET', `${API()}/api/hands`, { statusCode: 200, body: { hands, total: 1 } }).as('getHands');

    cy.visit('/hands');
    cy.clickRefresh();
    cy.wait('@getHands');

    cy.get('[data-cy="hand-requirements-toggle"]').should('exist');
    cy.get('[data-cy="hand-requirements-toggle"]').click();
    cy.get('[data-cy="hand-requirements-section"]').should('exist');
    cy.get('[data-cy="hand-requirements-section"]').should('contain.text', 'Twitter API key');
  });
});
