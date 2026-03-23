describe('Layer 11 — Founder client shell', () => {
  it('renders clean founder empty state when no founder workspace exists', () => {
    const uniqueSuffix = `${Date.now()}-${Cypress._.random(1000, 9999)}`;
    const emptyClientName = `Founder Empty Client ${uniqueSuffix}`;

    let emptyClientId;

    cy.request('POST', '/api/clients', {
      business_name: emptyClientName,
      industry: 'Services',
      main_goal: 'Keep founder workspace empty',
      offer: 'Client delivery system',
      customer: 'Operations teams',
      approval_mode: 'conditional',
      approvers: [{ name: 'Empty Reviewer', email: 'empty-reviewer@example.com' }],
    }).then(({ body }) => {
      emptyClientId = body.client.id;
    });

    cy.then(() => {
      cy.intercept('GET', '/api/playbooks', {
        statusCode: 200,
        body: {
          playbooks: [
            { id: 'customer-discovery', title: 'Customer Discovery' },
          ],
        },
      }).as('getFounderPlaybooks');

      cy.intercept('GET', `/api/founder/workspaces?clientId=${emptyClientId}`, {
        statusCode: 200,
        body: {
          workspaces: [],
        },
      }).as('getEmptyFounderWorkspaces');

      cy.visit(`/clients/${emptyClientId}`);
      cy.get('[data-cy="client-home-page"]', { timeout: 12000 }).should('be.visible');
      cy.wait('@getEmptyFounderWorkspaces');
      cy.wait('@getFounderPlaybooks');

      cy.get('[data-cy="founder-workspace-panel"]').should('be.visible');
      cy.get('[data-cy="founder-empty-state"]').should('be.visible');
      cy.get('[data-cy="founder-workspace-panel"]').should('contain.text', 'Founder workspace');
      cy.get('[data-cy="founder-empty-state"]').should('contain.text', 'No founder workspace yet');
      cy.get('[data-cy="founder-empty-state"]').should('contain.text', 'The guided setup takes a minute');

      cy.get('[data-cy="founder-empty-state-cta"]').scrollIntoView().should('be.visible').and('contain.text', 'Start founder setup');
      cy.get('[data-cy="founder-empty-state-cta"]').should('have.attr', 'href').then((href) => {
        const target = new URL(href, 'http://localhost:3002');
        expect(target.pathname).to.eq(`/clients/${emptyClientId}/founder/start`);
        expect(target.search).to.eq('');
      });

      cy.get('[data-cy="founder-workspace-summary"]').should('not.exist');
      cy.get('[data-cy="founder-latest-run-card"]').should('not.exist');
      cy.get('[data-cy="founder-recent-runs-panel"]').should('not.exist');
      cy.contains('Recent founder runs').should('not.exist');
      cy.contains('Latest founder run').should('not.exist');
      cy.verifyNoConsoleErrors();
    });
  });

  it('keeps founder state isolated across clients after navigation and reload', () => {
    const uniqueSuffix = `${Date.now()}-${Cypress._.random(1000, 9999)}`;
    const clientAName = `Founder Isolation A ${uniqueSuffix}`;
    const clientBName = `Founder Isolation B ${uniqueSuffix}`;
    const playbookId = 'customer-discovery';
    const runSummary = `Top risk is weak ICP clarity ${uniqueSuffix}`;
    const nextAction = `Interview 5 founders ${uniqueSuffix}`;
    const runId = `founder-run-${uniqueSuffix}`;

    let clientAId;
    let clientBId;
    let workspaceIdA;

    cy.request('POST', '/api/clients', {
      business_name: clientAName,
      industry: 'SaaS',
      main_goal: 'Validate ICP before scaling',
      offer: 'AI founder operating system',
      customer: 'Founders',
      approval_mode: 'conditional',
      approvers: [{ name: 'Client A Reviewer', email: 'client-a-reviewer@example.com' }],
    }).then(({ body }) => {
      clientAId = body.client.id;
      workspaceIdA = `client-${clientAId}`;
    });

    cy.request('POST', '/api/clients', {
      business_name: clientBName,
      industry: 'Agency',
      main_goal: 'Keep founder state empty',
      offer: 'Delivery operations',
      customer: 'Services teams',
      approval_mode: 'conditional',
      approvers: [{ name: 'Client B Reviewer', email: 'client-b-reviewer@example.com' }],
    }).then(({ body }) => {
      clientBId = body.client.id;
    });

    cy.then(() => {
      cy.request('POST', '/api/founder/workspaces', {
        clientId: clientAId,
        workspaceId: workspaceIdA,
        name: `${clientAName} Founder Workspace`,
        companyName: clientAName,
        idea: 'AI founder operating system',
        stage: 'validation',
        playbookDefaults: { defaultPlaybookId: playbookId },
      });

      cy.request('POST', `/api/founder/workspaces/${workspaceIdA}/runs`, {
        runId,
        playbookId,
        playbookLabel: 'Customer Discovery',
        prompt: 'Find the biggest founder risk to validate first.',
        status: 'completed',
        summary: runSummary,
        citations: ['https://example.com/mom-test'],
        nextActions: [nextAction],
        createdAt: '2026-03-22T09:00:00.000Z',
      });

      cy.visit(`/clients/${clientAId}`);
      cy.get('[data-cy="client-home-page"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-workspace-summary"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-latest-run-card"]').should('be.visible').and('contain.text', runSummary);
      cy.get('[data-cy="founder-playbook-badge"]').should('contain.text', 'Customer Discovery');
      cy.get('[data-cy="founder-next-actions-panel"]').should('contain.text', nextAction);
      cy.get('[data-cy="founder-recent-runs-panel"]').should('be.visible');
      cy.get('[data-cy="founder-recent-run-link"]').should('have.length', 1).first().should('contain.text', runSummary);
      cy.get('[data-cy="founder-reopen-latest-run-cta"]').should('contain.text', 'Open latest result');
      cy.get('[data-cy="founder-reopen-latest-run-cta"]').should('have.attr', 'href').then((href) => {
        const target = new URL(href, 'http://localhost:3002');
        expect(target.pathname).to.eq('/deep-research');
        expect(target.searchParams.get('clientId')).to.eq(clientAId);
        expect(target.searchParams.get('clientName')).to.eq(clientAName);
        expect(target.searchParams.get('workspaceId')).to.eq(workspaceIdA);
        expect(target.searchParams.get('runId')).to.eq(runId);
        expect(target.searchParams.get('playbookId')).to.eq(playbookId);
      });

      cy.visit(`/clients/${clientBId}`);
      cy.get('[data-cy="client-home-page"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-workspace-panel"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-empty-state"]').should('be.visible');
      cy.get('[data-cy="founder-empty-state"]').should('contain.text', 'No founder workspace yet');
      cy.get('[data-cy="founder-empty-state"]').should('contain.text', 'The guided setup takes a minute');
      cy.get('[data-cy="founder-empty-state-cta"]').scrollIntoView().should('be.visible').and('contain.text', 'Start founder setup');
      cy.get('[data-cy="founder-empty-state-cta"]').should('have.attr', 'href').then((href) => {
        const target = new URL(href, 'http://localhost:3002');
        expect(target.pathname).to.eq(`/clients/${clientBId}/founder/start`);
        expect(target.search).to.eq('');
      });

      cy.get('[data-cy="founder-workspace-summary"]').should('not.exist');
      cy.get('[data-cy="founder-latest-run-card"]').should('not.exist');
      cy.get('[data-cy="founder-recent-runs-panel"]').should('not.exist');
      cy.get('[data-cy="founder-reopen-latest-run-cta"]').should('not.exist');
      cy.contains(runSummary).should('not.exist');
      cy.contains('Customer Discovery').should('not.exist');
      cy.contains(nextAction).should('not.exist');

      cy.reload();
      cy.get('[data-cy="client-home-page"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-empty-state"]').should('be.visible');
      cy.get('[data-cy="founder-empty-state-cta"]').scrollIntoView().should('be.visible');
      cy.get('[data-cy="founder-workspace-summary"]').should('not.exist');
      cy.get('[data-cy="founder-latest-run-card"]').should('not.exist');
      cy.get('[data-cy="founder-recent-runs-panel"]').should('not.exist');
      cy.contains(runSummary).should('not.exist');
      cy.contains('Customer Discovery').should('not.exist');
      cy.contains(nextAction).should('not.exist');

      cy.visit(`/clients/${clientAId}`);
      cy.get('[data-cy="client-home-page"]', { timeout: 12000 }).should('be.visible');
      cy.get('[data-cy="founder-latest-run-card"]').should('contain.text', runSummary);
      cy.get('[data-cy="founder-playbook-badge"]').should('contain.text', 'Customer Discovery');
      cy.get('[data-cy="founder-next-actions-panel"]').should('contain.text', nextAction);
      cy.get('[data-cy="founder-recent-run-link"]').should('have.length', 1);
      cy.verifyNoConsoleErrors();
    });
  });
});