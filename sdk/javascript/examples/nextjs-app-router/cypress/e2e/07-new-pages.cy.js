/**
 * Layer 7 — New / Previously Uncovered Pages Smoke
 *
 * Proves for every page not covered by spec 01 that:
 *  1. The sidebar link navigates to the correct URL.
 *  2. Direct URL access works without crashing.
 *  3. The page root element renders (no blank screen).
 *  4. An <h1> heading exists (minimum structural proof).
 *
 * These tests run against the live Next.js + Rust stack.
 * They do NOT stub any API calls — both servers must be running.
 *
 * Note: clicks use { force: true } because headless Electron intercepts
 * element centre coordinates on the sidebar (a known Cypress/Next.js 15
 * headless quirk). force:true dispatches directly on the <a> element.
 */

const PAGES = [
  {
    label: 'Overview',
    href: '/overview',
    navCy: 'nav-link-overview',
    rootCy: 'overview-page',
    heading: 'Overview',
  },
  {
    label: 'Today',
    href: '/today',
    navCy: 'nav-link-today',
    rootCy: 'today-page',
    heading: 'Today',
  },
  {
    label: 'Analytics',
    href: '/analytics',
    navCy: 'nav-link-analytics',
    rootCy: 'analytics-page',
    heading: 'Analytics',
  },
  {
    label: 'Logs',
    href: '/logs',
    navCy: 'nav-link-logs',
    rootCy: 'logs-page',
    heading: 'Audit Logs',
  },
  {
    label: 'Runtime',
    href: '/runtime',
    navCy: 'nav-link-runtime',
    rootCy: 'runtime-page',
    heading: 'Runtime',
  },
  {
    label: 'Sessions',
    href: '/sessions',
    navCy: 'nav-link-sessions',
    rootCy: 'sessions-page',
    heading: 'Sessions',
  },
  {
    label: 'Settings',
    href: '/settings',
    navCy: 'nav-link-settings',
    rootCy: 'settings-page',
    heading: 'Settings',
  },
  {
    label: 'Skills',
    href: '/skills',
    navCy: 'nav-link-skills',
    rootCy: 'skills-page',
    heading: 'Skills',
  },
  {
    label: 'Channels',
    href: '/channels',
    navCy: 'nav-link-channels',
    rootCy: 'channels-page',
    heading: 'Integrations',
  },
  {
    label: 'Onboarding',
    href: '/onboarding',
    navCy: 'nav-link-onboarding',
    rootCy: 'onboarding-wizard',
    heading: 'Setup Guide',
  },
];

describe('Layer 7 — New Pages Smoke', () => {
  // ── Pre-flight ──────────────────────────────────────────────────────────────
  it('overview is the default landing page (root redirects)', () => {
    cy.visit('/');
    cy.location('pathname', { timeout: 12000 }).should('eq', '/overview');
    cy.get('[data-cy="sidebar"]').should('be.visible');
  });

  // ── Per-page suite ──────────────────────────────────────────────────────────
  PAGES.forEach(({ label, href, navCy, rootCy, heading }) => {
    describe(label, () => {
      it(`sidebar link opens ${href}`, () => {
        cy.visit('/overview');
        cy.get('[data-cy="sidebar"]', { timeout: 12000 }).should('be.visible');
        cy.get(`[data-cy="${navCy}"]`).should('exist').click({ force: true });
        cy.location('pathname', { timeout: 12000 }).should('eq', href);
      });

      it(`direct URL ${href} loads correctly`, () => {
        cy.visit(href);
        cy.get(`[data-cy="${rootCy}"]`, { timeout: 12000 }).should('exist');
        cy.get('h1').should('contain.text', heading);
      });

      it(`${href} does not produce blank screen`, () => {
        cy.visit(href);
        cy.get('body').should('not.be.empty');
        cy.get('[data-cy="sidebar"]', { timeout: 12000 }).should('be.visible');
        cy.get(`[data-cy="${rootCy}"]`).should('exist');
      });
    });
  });
});
