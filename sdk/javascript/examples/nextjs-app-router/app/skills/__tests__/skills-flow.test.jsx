/**
 * Integration smoke test: SkillsClient + SkillDrawer interaction.
 * Tests the full user flow from card list → open drawer → toggle → close.
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

import { apiClient } from '../../../lib/api-client';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------
const initialSkill = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '1.0.0',
  tool_count: 2,
  used_by_count: 3,
};

const detailFixture = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '1.0.0',
  tools: ['search', 'browse'],
  used_by: ['researcher', 'analyst', 'assistant'],
  used_by_count: 3,
  source: 'bundled',
  entrypoint: '',
  prompt_context: '',
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('skills page flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(detailFixture),
    });
  });

  it('renders card → opens drawer → drawer toggle calls apiClient.put → card reflects updated state', async () => {
    const user = userEvent.setup();
    apiClient.put.mockResolvedValue({ name: 'web_search', enabled: false });

    render(<SkillsClient initialSkills={[initialSkill]} />);

    // 1. Card renders with correct agent count
    expect(screen.getByText(/Referenced by 3 agent/i)).toBeInTheDocument();

    // 2. Click "Details" to open drawer
    await act(async () => {
      await user.click(screen.getByText('Details'));
    });

    // 3. Drawer loads detail (fetch was called for the skill)
    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('web_search'),
        expect.any(Object),
      );
    });

    // 4. Tools and agents appear in drawer
    await waitFor(() => {
      expect(screen.getByText('browse')).toBeInTheDocument();
      expect(screen.getByText('researcher')).toBeInTheDocument();
    });

    // 5. Toggle in the drawer (disable the skill)
    const drawerToggle = document.querySelector('[data-cy="skill-drawer-toggle"]');
    expect(drawerToggle).toBeTruthy();
    await act(async () => { await user.click(drawerToggle); });

    // 6. apiClient.put was called with the correct payload
    await waitFor(() => {
      expect(apiClient.put).toHaveBeenCalledWith(
        expect.stringContaining('web_search'),
        expect.objectContaining({ enabled: false }),
      );
    });

    // 7. Close the drawer
    const closeBtn = screen.getByLabelText('Close drawer');
    await act(async () => { await user.click(closeBtn); });

    // Drawer is gone
    expect(screen.queryByText('browse')).not.toBeInTheDocument();

    // 8. used_by_count on the card is unchanged after toggle (count comes from
    //    the cached initialSkills data, not from the toggle response)
    expect(screen.getByText(/Referenced by 3 agent/i)).toBeInTheDocument();
  });
});
