'use client';
import Link from 'next/link';

const SETUP_CARDS = [
  { href: '/hands',         icon: '🤝', title: 'Set up Hands',          description: 'Bundle agents, skills, and tools into ready-made operator packs.' },
  { href: '/agent-catalog', icon: '🤖', title: 'Set up agents',         description: 'Choose helpers by job — researcher, planner, assistant, and more.' },
  { href: '/integrations',  icon: '🔌', title: 'Connect tools',          description: 'Link email, files, calendar, social, and other services.' },
  { href: '/comms',         icon: '✉',  title: 'Set up communication',   description: 'Drafts, messages, approvals, and threads in one place.' },
  { href: '/skills',        icon: '⚡', title: 'Set up skills',          description: 'Add capabilities to your agents — web, files, email, and more.' },
  { href: '/workflows',     icon: '▶',  title: 'Set up automation',      description: 'Build workflows that run in steps with optional approval gates.' },
  { href: '/scheduler',     icon: '📅', title: 'Set up schedules',       description: 'Tell the system when things should happen — daily, weekly, or once.' },
];

export default function SetupPage() {
  return (
    <div data-cy="setup-page" style={{ maxWidth: 820, margin: '0 auto', padding: '48px 24px' }}>
      <div style={{ marginBottom: 40, textAlign: 'center' }}>
        <h1 style={{ fontSize: 28, fontWeight: 800, margin: '0 0 10px' }}>Welcome. Let&apos;s get you set up.</h1>
        <p style={{ fontSize: 15, color: 'var(--text-dim)', margin: 0, lineHeight: 1.6 }}>
          Pick where you want to start. You can come back to the others any time.
        </p>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: 16 }}>
        {SETUP_CARDS.map(card => (
          <Link
            key={card.href}
            href={`${card.href}?tab=recommended&view=simple`}
            style={{ textDecoration: 'none', display: 'block' }}
          >
            <div
              data-cy={`setup-card-${card.href.replace('/', '')}`}
              style={{
                border: '1px solid var(--border)',
                borderRadius: 12,
                padding: '20px 22px',
                background: 'var(--surface, #111)',
                cursor: 'pointer',
                transition: 'border-color 0.15s',
                display: 'flex',
                flexDirection: 'column',
                gap: 10,
                height: '100%',
              }}
              onMouseEnter={e => e.currentTarget.style.borderColor = 'var(--accent)'}
              onMouseLeave={e => e.currentTarget.style.borderColor = 'var(--border)'}
            >
              <div style={{ fontSize: 28 }}>{card.icon}</div>
              <div style={{ fontWeight: 700, fontSize: 15 }}>{card.title}</div>
              <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.55 }}>{card.description}</div>
              <div style={{ marginTop: 'auto', paddingTop: 8, fontSize: 13, color: 'var(--accent)', fontWeight: 600 }}>
                Get started →
              </div>
            </div>
          </Link>
        ))}
      </div>

      <div style={{ marginTop: 48, padding: '18px 22px', background: 'rgba(124,58,237,0.07)', border: '1px solid rgba(124,58,237,0.28)', borderRadius: 10, textAlign: 'center' }}>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 6 }}>Already set up?</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 14 }}>Go to the dashboard to see what&apos;s running.</div>
        <Link href="/dashboard" style={{ padding: '8px 20px', borderRadius: 8, background: 'var(--accent)', color: '#fff', textDecoration: 'none', fontWeight: 600, fontSize: 13 }}>
          Open dashboard
        </Link>
      </div>
    </div>
  );
}
