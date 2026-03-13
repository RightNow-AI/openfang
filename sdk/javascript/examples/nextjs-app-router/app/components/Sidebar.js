'use client';

import { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';

const NAV = [
  {
    label: 'Planner',
    items: [
      { href: '/today', label: 'Today', icon: <svg viewBox="0 0 24 24"><path d="M8 2v4"/><path d="M16 2v4"/><rect x="3" y="4" width="18" height="18" rx="2"/><path d="M3 10h18"/><path d="M8 14h3"/><path d="M8 18h8"/></svg> },
      { href: '/inbox', label: 'Inbox', icon: <svg viewBox="0 0 24 24"><path d="M22 12h-4l-3 5-6-10-3 5H2"/></svg> },
      { href: '/agent-catalog', label: 'Agent Catalog', icon: <svg viewBox="0 0 24 24"><path d="M12 12a5 5 0 1 0-5-5 5 5 0 0 0 5 5Z"/><path d="M3 21a9 9 0 0 1 18 0"/></svg> },
    ],
  },
  {
    label: 'Chat',
    items: [
      { href: '/chat', label: 'Chat', icon: <svg viewBox="0 0 24 24"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg> },
    ],
  },
  {
    label: 'Monitor',
    items: [
      { href: '/overview', label: 'Overview', icon: <svg viewBox="0 0 24 24"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 22V12h6v10"/></svg> },
      { href: '/analytics', label: 'Analytics', icon: <svg viewBox="0 0 24 24"><path d="M18 20V10M12 20V4M6 20v-6"/></svg> },
      { href: '/logs', label: 'Logs', icon: <svg viewBox="0 0 24 24"><path d="m4 17 6-6-6-6"/><path d="M12 19h8"/></svg> },
    ],
  },
  {
    label: 'Agents',
    items: [
      { href: '/sessions', label: 'Sessions', icon: <svg viewBox="0 0 24 24"><path d="m12 2-10 5 10 5 10-5z"/><path d="m2 17 10 5 10-5"/><path d="m2 12 10 5 10-5"/></svg> },
      { href: '/approvals', label: 'Approvals', icon: <svg viewBox="0 0 24 24"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/></svg> },
      { href: '/comms', label: 'Comms', icon: <svg viewBox="0 0 24 24"><path d="M21 11.5a8.38 8.38 0 01-.9 3.8 8.5 8.5 0 01-7.6 4.7 8.38 8.38 0 01-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 01-.9-3.8 8.5 8.5 0 014.7-7.6 8.38 8.38 0 013.8-.9h.5a8.48 8.48 0 018 8v.5z"/></svg> },
    ],
  },
  {
    label: 'Automation',
    items: [
      { href: '/workflows', label: 'Workflows', icon: <svg viewBox="0 0 24 24"><path d="M6 3v12M18 9a9 9 0 0 1-9 9"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/></svg> },
      { href: '/scheduler', label: 'Scheduler', icon: <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg> },
    ],
  },
  {
    label: 'Extensions',
    items: [
      { href: '/channels', label: 'Channels', icon: <svg viewBox="0 0 24 24"><path d="M4 9h16M4 15h16M10 3l-2 18M16 3l-2 18"/></svg> },
      { href: '/skills', label: 'Skills', icon: <svg viewBox="0 0 24 24"><path d="m16 18 6-6-6-6M8 6l-6 6 6 6"/></svg> },
      { href: '/hands', label: 'Hands', icon: <svg viewBox="0 0 24 24"><path d="M18 11V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2"/><path d="M14 10V4a2 2 0 0 0-2-2 2 2 0 0 0-2 2v6"/><path d="M10 10.5V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.9-5.7-2.4L3.4 16a2 2 0 0 1 3.2-2.4L8 15"/></svg> },
    ],
  },
  {
    label: 'System',
    items: [
      { href: '/runtime', label: 'Runtime', icon: <svg viewBox="0 0 24 24"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg> },
      { href: '/settings', label: 'Settings', icon: <svg viewBox="0 0 24 24"><path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3"/><path d="M1 14h6M9 8h6M17 16h6"/></svg> },
    ],
  },
];

function NavSection({ section, collapsed: sidebarCollapsed }) {
  const [open, setOpen] = useState(true);
  const pathname = usePathname();

  return (
    <div className="nav-section">
      {!sidebarCollapsed && (
        <div className="nav-section-title" onClick={() => setOpen(o => !o)}>
          <span className="nav-label">{section.label}</span>
          <span className="nav-section-chevron" style={{ transform: open ? 'rotate(90deg)' : '' }}>›</span>
        </div>
      )}
      {(open || sidebarCollapsed) && section.items.map(item => {
        const active = pathname === item.href || (item.href !== '/' && pathname?.startsWith(item.href));
        return (
          <Link
            key={item.href}
            href={item.href}
            className={`nav-item${active ? ' active' : ''}`}
            title={sidebarCollapsed ? item.label : undefined}
          >
            <span className="nav-icon">{item.icon}</span>
            {!sidebarCollapsed && <span className="nav-label">{item.label}</span>}
          </Link>
        );
      })}
    </div>
  );
}

export default function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [connState, setConnState] = useState('connecting'); // 'connected' | 'disconnected' | 'connecting'
  const [agentCount, setAgentCount] = useState(0);
  const [theme, setTheme] = useState('system');
  const pathname = usePathname();

  // Persist sidebar state
  useEffect(() => {
    const saved = localStorage.getItem('openfang-sidebar');
    if (saved === 'collapsed') setCollapsed(true);

    const savedTheme = localStorage.getItem('openfang-theme-mode') || 'system';
    setTheme(savedTheme);
    applyTheme(savedTheme);
  }, []);

  // Close mobile menu on navigation
  useEffect(() => { setMobileOpen(false); }, [pathname]);

  function applyTheme(mode) {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const dark = mode === 'dark' || (mode === 'system' && prefersDark);
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
  }

  function cycleTheme() {
    const modes = ['system', 'light', 'dark'];
    const next = modes[(modes.indexOf(theme) + 1) % modes.length];
    setTheme(next);
    localStorage.setItem('openfang-theme-mode', next);
    applyTheme(next);
  }

  const themeIcon = theme === 'dark' ? '🌙' : theme === 'light' ? '☀️' : '⚙️';

  const toggleCollapse = useCallback(() => {
    setCollapsed(c => {
      const next = !c;
      localStorage.setItem('openfang-sidebar', next ? 'collapsed' : 'expanded');
      return next;
    });
  }, []);

  // Poll daemon health for connection status
  useEffect(() => {
    let timer;
    async function check() {
      try {
        const base = process.env.NEXT_PUBLIC_OPENFANG_BASE_URL || 'http://127.0.0.1:50051';
        const r = await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(4000) });
        if (r.ok) {
          const d = await r.json();
          setConnState('connected');
          // Also fetch agent count
          try {
            const ar = await fetch(`${base}/api/agents`, { signal: AbortSignal.timeout(4000) });
            if (ar.ok) { const agents = await ar.json(); setAgentCount(Array.isArray(agents) ? agents.length : 0); }
          } catch (_) {}
        } else {
          setConnState('disconnected');
        }
      } catch (_) {
        setConnState('disconnected');
      }
      timer = setTimeout(check, 10000);
    }
    check();
    return () => clearTimeout(timer);
  }, []);

  const connLabel = connState === 'connected' ? `${agentCount} agent${agentCount === 1 ? '' : 's'} running`
    : connState === 'connecting' ? 'Connecting…'
    : 'Disconnected';

  return (
    <>
      {/* Mobile overlay */}
      {mobileOpen && (
        <div className="sidebar-overlay mobile-open" onClick={() => setMobileOpen(false)} />
      )}

      {/* Mobile menu button — shown via CSS on small screens */}
      <button
        className="mobile-menu-btn"
        style={{ position: 'fixed', top: 12, left: 12, zIndex: 60 }}
        onClick={() => setMobileOpen(o => !o)}
        aria-label="Toggle navigation"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M4 6h16M4 12h16M4 18h16"/></svg>
      </button>

      <nav className={`sidebar${collapsed ? ' collapsed' : ''}${mobileOpen ? ' mobile-open' : ''}`}>
        {/* Header */}
        <div className="sidebar-header">
          <div className="sidebar-logo">OF</div>
          {!collapsed && (
            <div className="sidebar-header-text">
              <div style={{ fontWeight: 700, fontSize: 14 }}>OpenFang</div>
              <div className="sidebar-tagline">Agent workspace</div>
            </div>
          )}
        </div>

        {/* Connection status */}
        <div className="sidebar-status">
          <span className={`conn-dot${connState === 'disconnected' ? ' offline' : connState === 'connecting' ? ' connecting' : ''}`} />
          {!collapsed && <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>{connLabel}</span>}
        </div>

        {/* Navigation */}
        <div className="sidebar-nav" role="navigation" aria-label="Main navigation">
          {NAV.map(section => (
            <NavSection key={section.label} section={section} collapsed={collapsed} />
          ))}
        </div>

        {/* Footer */}
        {!collapsed && (
          <div className="sidebar-footer">
            <div className="sidebar-footer-title">Quick actions</div>
            <div className="sidebar-footer-copy">⌘K search · ⌘N new agent</div>
            <button
              onClick={cycleTheme}
              className="btn btn-ghost btn-xs"
              style={{ marginTop: 8, width: '100%' }}
              title="Cycle theme"
            >
              {themeIcon} {theme === 'system' ? 'System' : theme === 'light' ? 'Light' : 'Dark'} theme
            </button>
          </div>
        )}

        {/* Collapse toggle */}
        <div className="sidebar-toggle" onClick={toggleCollapse} title={collapsed ? 'Expand' : 'Collapse'}>
          {collapsed ? '›' : '‹'}
        </div>
      </nav>
    </>
  );
}
