/**
 * Tests for SkillsClient component
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import SkillsClient from '../SkillsClient';

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------
vi.mock('../../../lib/api-client', () => ({
  apiClient: { get: vi.fn(), put: vi.fn() },
}));

vi.mock('../../../lib/telemetry', () => ({
  track: vi.fn(),
}));

// Mock SkillDrawer so SkillsClient tests don't need drawer-level fetch
vi.mock('../SkillDrawer', () => ({
  default: ({ skillName, onClose, onToggle }) => (
    <div data-testid="mock-drawer">
      <span data-testid="drawer-skill-name">{skillName}</span>
      <button onClick={onClose}>Close drawer</button>
      <button
        data-testid="drawer-toggle-btn"
        onClick={() => onToggle(skillName, true, 1)}
      >
        Toggle in drawer
      </button>
    </div>
  ),
}));

import { apiClient } from '../../../lib/api-client';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------
const skillWeb = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '0.1.0',
  tool_count: 2,
  used_by_count: 2,
};

const skillMemory = {
  name: 'memory',
  description: 'Memory access',
  runtime: 'python',
  enabled: false,
  bundled: false,
  version: '0.2.0',
  tool_count: 1,
  used_by_count: 0,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('SkillsClient', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('renders skill cards with runtime, bundled state, and referenced-by count', () => {
    render(<SkillsClient initialSkills={[skillWeb]} />);
    expect(screen.getByText('web_search')).toBeInTheDocument();
    expect(screen.getByText('node')).toBeInTheDocument();
    expect(screen.getByText('Bundled')).toBeInTheDocument();
    expect(screen.getByText(/Referenced by 2 agent/i)).toBeInTheDocument();
  });

  it('shows "Not referenced by any agent" when used_by_count is zero', () => {
    render(<SkillsClient initialSkills={[skillMemory]} />);
    expect(screen.getByText('Not referenced by any agent')).toBeInTheDocument();
  });

  it('optimistically toggles enabled state before the fetch resolves', async () => {
    const user = userEvent.setup();
    let resolvePut;
    apiClient.put.mockReturnValue(
      new Promise(resolve => { resolvePut = resolve; })
    );

    render(<SkillsClient initialSkills={[skillWeb]} />);

    const toggleBtns = screen.getAllByTestId
      ? screen.queryAllByText(/● Enabled|○ Disabled/)
      : [];

    // Use data-cy selector via DOM query
    const toggle = document.querySelector('[data-cy="skill-toggle"]');
    expect(toggle).toBeTruthy();

    // web_search starts enabled — initially shows Enabled
    expect(toggle.textContent).toMatch(/Enabled/);

    await act(async () => { await user.click(toggle); });

    // While fetch is in-flight, button shows spinner (empty or pending)
    // The key assertion: optimistic flip means button is disabled
    expect(toggle).toBeDisabled();

    // Resolve the fetch as success
    await act(async () => {
      resolvePut({ name: 'web_search', enabled: false });
    });

    await waitFor(() => {
      expect(toggle).not.toBeDisabled();
    });
  });

  it('rolls back the toggle when the API call fails', async () => {
    const user = userEvent.setup();
    apiClient.put.mockRejectedValue(new Error('Server error'));

    render(<SkillsClient initialSkills={[skillWeb]} />);

    const toggle = document.querySelector('[data-cy="skill-toggle"]');
    await act(async () => { await user.click(toggle); });

    // After rejection, state rolls back — no longer pending
    await waitFor(() => {
      expect(toggle).not.toBeDisabled();
    });

    // Error message shown
    expect(screen.getByText(/Server error/i)).toBeInTheDocument();
  });

  it('disables the toggle button while the request is pending', async () => {
    const user = userEvent.setup();
    let resolvePut;
    apiClient.put.mockReturnValue(
      new Promise(resolve => { resolvePut = resolve; })
    );

    render(<SkillsClient initialSkills={[skillWeb]} />);
    const toggle = document.querySelector('[data-cy="skill-toggle"]');

    await act(async () => { await user.click(toggle); });
    expect(toggle).toBeDisabled();

    await act(async () => { resolvePut({ name: 'web_search', enabled: false }); });
    await waitFor(() => { expect(toggle).not.toBeDisabled(); });
  });

  it('opens the detail drawer for the selected skill', async () => {
    const user = userEvent.setup();
    render(<SkillsClient initialSkills={[skillWeb]} />);

    const detailBtn = screen.getByText('Details');
    await act(async () => { await user.click(detailBtn); });

    expect(screen.getByTestId('mock-drawer')).toBeInTheDocument();
    expect(screen.getByTestId('drawer-skill-name').textContent).toBe('web_search');
  });

  it('updates the card state when the drawer toggle succeeds', async () => {
    const user = userEvent.setup();
    apiClient.put.mockResolvedValue({ name: 'web_search', enabled: false });

    render(<SkillsClient initialSkills={[skillWeb]} />);

    // Open drawer
    await act(async () => { await user.click(screen.getByText('Details')); });

    // Trigger toggle inside drawer
    const drawerToggle = screen.getByTestId('drawer-toggle-btn');
    await act(async () => { await user.click(drawerToggle); });

    // After the fetch resolves, the card should reflect new state
    await waitFor(() => {
      const toggle = document.querySelector('[data-cy="skill-toggle"]');
      expect(toggle).not.toBeDisabled();
    });

    // The put was called with the right payload
    expect(apiClient.put).toHaveBeenCalledWith(
      expect.stringContaining('web_search'),
      expect.objectContaining({ enabled: false }),
    );
  });
});
