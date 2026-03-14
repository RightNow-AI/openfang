/**
 * Layer 1 – Sidebar Route Smoke Coverage
 *
 * Proves for every sidebar page that:
 *  1. The sidebar link navigates to the correct URL.
 *  2. Direct URL access works without crashing.
 *  3. The page root element renders (no blank screen).
 *  4. The app shell (Sidebar) stays intact after navigation.
 *  5. An <h1> heading exists (minimum structural proof).
 *  6. No uncaught runtime exception bubbles up.
 *
 * These tests run against the live Next.js + Rust stack.
 * They do NOT stub any API calls — they require both servers to be running.
 */

const PAGES = [
  {
    label: 'Chat',
    href: '/chat',
    navCy: 'nav-link-chat',
    rootCy: 'chat-page',
    heading: 'Chat',
  },
  {
    label: 'Inbox',
    href: '/inbox',
    navCy: 'nav-link-inbox',
    rootCy: 'inbox-page',
    heading: 'Inbox',
  },
  {
    label: 'Agent Catalog',
    href: '/agent-catalog',
    navCy: 'nav-link-agent-catalog',
    rootCy: 'catalog-page',
    heading: 'Agent Catalog',
  },
  {
    label: 'Approvals',
    href: '/approvals',
    navCy: 'nav-link-approvals',
    rootCy: 'approvals-page',
    heading: 'Approvals',
  },
  {
    label: 'Comms',
    href: '/comms',
    navCy: 'nav-link-comms',
    rootCy: 'comms-page',
    heading: 'Comms',
  },
  {
    label: 'Workflows',
    href: '/workflows',
    navCy: 'nav-link-workflows',
    rootCy: 'workflows-page',
    heading: 'Workflows',
  },
  {
    label: 'Scheduler',
    href: '/scheduler',
    navCy: 'nav-link-scheduler',
    rootCy: 'scheduler-page',
    heading: 'Scheduler',
  },
  {
    label: 'Hands',
    href: '/hands',
    navCy: 'nav-link-hands',
    rootCy: 'hands-page',
    heading: 'Hands',
  },
];

describe('Layer 1 — Sidebar Route Smoke', () => {
  // ── Pre-flight ──────────────────────────────────────────────────────────────
  it('home page loads and sidebar is present', () => {
    cy.visit('/');
    cy.get('[data-cy="sidebar"]', { timeout: 12000 }).should('be.visible');
    cy.get('body').should('not.be.empty');
  });

  // ── Per-page suite ──────────────────────────────────────────────────────────
  PAGES.forEach(({ label, href, navCy, rootCy, heading }) => {
    describe(label, () => {
      /**
       * Test 1: Sidebar link navigates correctly.
       * Starts from home, clicks the sidebar link, checks URL and root element.
       */
      it(`sidebar link opens ${href}`, () => {
        cy.visit('/');
        cy.get('[data-cy="sidebar"]').should('be.visible');

        // Sidebar link must exist and be clickable
        cy.get(`[data-cy="${navCy}"]`)
          .should('be.visible')
          .click();

        // URL must resolve to the expected path
        cy.location('pathname').should('eq', href);

        // Page root must exist — proves no crash and correct component mount
        cy.get(`[data-cy="${rootCy}"]`, { timeout: 12000 }).should('exist');

        // Sidebar must remain visible — proves app shell is intact
        cy.get('[data-cy="sidebar"]').should('be.visible');

        // Heading must exist
        cy.get('h1').should('contain.text', heading);
      });

      /**
       * Test 2: Direct URL access works.
       * Simulates bookmark / external link — no sidebar click involved.
       */
      it(`direct URL ${href} loads correctly`, () => {
        cy.visit(href);

        cy.get('[data-cy="sidebar"]', { timeout: 12000 }).should('be.visible');
        cy.get(`[data-cy="${rootCy}"]`, { timeout: 12000 }).should('exist');
        cy.get('h1').should('contain.text', heading);
      });

      /**
       * Test 3: Page does not produce a blank screen.
       * Checks that the body is non-empty and the page root has children.
       */
      it(`${href} does not produce blank screen`, () => {
        cy.visit(href);
        cy.get('body').should('not.be.empty');
        cy.get(`[data-cy="${rootCy}"]`)
          .should('exist')
          .children()
          .should('have.length.greaterThan', 0);
      });

      /**
       * Test 4: Navigation from one page to another leaves shell intact.
       * Goes to this page, then navigates away to /overview, then back.
       */
      it(`navigation to ${href} and back does not crash`, () => {
        cy.visit(href);
        cy.get(`[data-cy="${rootCy}"]`).should('exist');

        // Navigate to Overview (always present)
        cy.get('[data-cy="nav-link-overview"]').click();
        cy.location('pathname').should('eq', '/overview');
        cy.get('[data-cy="sidebar"]').should('be.visible');

        // Navigate back
        cy.get(`[data-cy="${navCy}"]`).click();
        cy.location('pathname').should('eq', href);
        cy.get(`[data-cy="${rootCy}"]`).should('exist');
      });
    });
  });

  // ── Sidebar structural tests ────────────────────────────────────────────────
  describe('Sidebar structure', () => {
    it('all expected nav links are present', () => {
      cy.visit('/');
      PAGES.forEach(({ navCy }) => {
        cy.get(`[data-cy="${navCy}"]`).should('exist');
      });
    });

    it('sidebar remains visible after three consecutive navigations', () => {
      cy.visit('/');
      const route1 = PAGES[0];
      const route2 = PAGES[2];
      const route3 = PAGES[4];

      cy.get(`[data-cy="${route1.navCy}"]`).click();
      cy.get('[data-cy="sidebar"]').should('be.visible');

      cy.get(`[data-cy="${route2.navCy}"]`).click();
      cy.get('[data-cy="sidebar"]').should('be.visible');

      cy.get(`[data-cy="${route3.navCy}"]`).click();
      cy.get('[data-cy="sidebar"]').should('be.visible');
    });
  });
});
