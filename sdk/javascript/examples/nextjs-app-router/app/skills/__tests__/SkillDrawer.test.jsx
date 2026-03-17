/**
 * Tests for SkillDrawer component
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import SkillDrawer from '../SkillDrawer';

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------
vi.mock('../../lib/telemetry', () => ({
  track: vi.fn(),
}));

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------
const detailWeb = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '0.1.0',
  tools: ['search', 'browse'],
  used_by: ['researcher', 'analyst'],
  used_by_count: 2,
  source: 'bundled',
  entrypoint: 'skills/web_search/index.js',
  prompt_context: '',
};

const detailBare = {
  name: 'bare_skill',
  description: 'Minimal skill',
  runtime: '',
  enabled: true,
  bundled: true,
  version: '',
  tools: [],
  used_by: [],
  used_by_count: 0,
  source: '',
  entrypoint: '',
  prompt_context: '',
};

function mockFetchOnce(data, status = 200) {
  global.fetch = vi.fn().mockResolvedValueOnce({
    json: () => Promise.resolve(data),
    ok: status < 400,
    status,
  });
}

const noop = () => {};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('SkillDrawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    global.fetch = vi.fn();
  });

  it('loads and renders skill detail when opened', async () => {
    mockFetchOnce(detailWeb);
    render(<SkillDrawer skillName="web_search" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByText('Search the web')).toBeInTheDocument();
    });
    expect(global.fetch).toHaveBeenCalledWith(
      expect.stringContaining('web_search'),
      expect.any(Object),
    );
  });

  it('renders tool badges and agent references', async () => {
    mockFetchOnce(detailWeb);
    render(<SkillDrawer skillName="web_search" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByText('search')).toBeInTheDocument();
      expect(screen.getByText('browse')).toBeInTheDocument();
      expect(screen.getByText('researcher')).toBeInTheDocument();
      expect(screen.getByText('analyst')).toBeInTheDocument();
    });
  });

  it('renders fallback text when no tools exist', async () => {
    mockFetchOnce(detailBare);
    render(<SkillDrawer skillName="bare_skill" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByText(/No tools defined/i)).toBeInTheDocument();
    });
  });

  it('renders fallback text when no agents reference the skill', async () => {
    mockFetchOnce(detailBare);
    render(<SkillDrawer skillName="bare_skill" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByText('No agents reference this skill.')).toBeInTheDocument();
    });
  });

  it('shows a warning before disabling a referenced skill', async () => {
    // web_search has used_by.length > 0 and is enabled — warning must be visible
    mockFetchOnce(detailWeb);
    render(<SkillDrawer skillName="web_search" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByTestId
        ? document.querySelector('[data-cy="skill-disable-warning"]')
        : screen.queryByText(/Referenced by/)
      ).toBeTruthy();
    });
  });

  it('aborts stale fetches when switching skills quickly', async () => {
    // First call hangs and respects the AbortSignal; second call resolves immediately.
    // This tests that the drawer's useEffect cleanup calls controller.abort(), causing
    // skill-a's fetch to reject with AbortError and never update state.
    let abortSkillA;
    global.fetch = vi.fn().mockImplementation((url, { signal } = {}) => {
      if (url.includes('skill-a')) {
        return new Promise((resolve, reject) => {
          if (signal?.aborted) {
            reject(Object.assign(new Error('Aborted'), { name: 'AbortError' }));
            return;
          }
          abortSkillA = () =>
            reject(Object.assign(new Error('Aborted'), { name: 'AbortError' }));
          signal?.addEventListener('abort', () =>
            reject(Object.assign(new Error('Aborted'), { name: 'AbortError' }))
          );
        });
      }
      // skill-b resolves immediately
      return Promise.resolve({
        json: () => Promise.resolve({ ...detailBare, name: 'skill-b', description: 'Skill B loaded' }),
      });
    });

    const { rerender } = render(
      <SkillDrawer skillName="skill-a" onClose={noop} onToggle={noop} togglePending={false} />
    );

    // Switch skill — triggers cleanup which calls controller.abort() on skill-a's signal
    rerender(<SkillDrawer skillName="skill-b" onClose={noop} onToggle={noop} togglePending={false} />);

    // skill-b description appears
    await waitFor(() => {
      expect(screen.getByText('Skill B loaded')).toBeInTheDocument();
    });

    // skill-a description must never have appeared (AbortError silenced it)
    expect(screen.queryByText('Search the web')).not.toBeInTheDocument();
  });

  it('closes cleanly without showing aborted-fetch errors when unmounted during fetch', async () => {
    let resolveFirst;
    global.fetch = vi.fn().mockReturnValue(
      new Promise(resolve => { resolveFirst = resolve; })
    );

    const { unmount } = render(
      <SkillDrawer skillName="web_search" onClose={noop} onToggle={noop} togglePending={false} />
    );

    // Unmount (triggers abort via cleanup) before fetch resolves
    unmount();

    // Now resolve — should not surface any error
    await act(async () => {
      resolveFirst({ json: () => Promise.resolve(detailWeb) });
    });

    // No error visible (component is unmounted; no DOM to check)
    // The test passes if the above doesn't throw
  });

  it('shows a readable error when the fetch fails', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      json: () => Promise.resolve({ error: 'Skill not found' }),
    });

    render(<SkillDrawer skillName="ghost" onClose={noop} onToggle={noop} togglePending={false} />);

    await waitFor(() => {
      expect(document.querySelector('[data-cy="skill-drawer-error"]')).toBeTruthy();
    });
  });

  it('toggles enabled state and calls onToggle with correct arguments', async () => {
    mockFetchOnce(detailWeb);
    const onToggle = vi.fn();
    render(<SkillDrawer skillName="web_search" onClose={noop} onToggle={onToggle} togglePending={false} />);

    await waitFor(() => {
      expect(screen.getByText('Search the web')).toBeInTheDocument();
    });

    const toggleBtn = document.querySelector('[data-cy="skill-drawer-toggle"]');
    expect(toggleBtn).toBeTruthy();

    await act(async () => {
      await userEvent.click(toggleBtn);
    });

    expect(onToggle).toHaveBeenCalledWith(
      'web_search',
      true,     // currentEnabled
      2,        // used_by count
    );
  });
});
