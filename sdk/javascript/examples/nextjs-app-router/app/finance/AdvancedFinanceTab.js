'use client';

const INTEGRATION_LINKS = [
  { href: '/agency', label: 'Agency', hint: 'Client billing and project revenue' },
  { href: '/growth', label: 'Growth', hint: 'Ad spend and marketing ROI' },
  { href: '/school', label: 'School', hint: 'Course and cohort revenue' },
  { href: '/workflows', label: 'Workflows', hint: 'Automation costs and billing events' },
  { href: '/agents', label: 'Agents', hint: 'LLM usage and API cost tracking' },
  { href: '/sales', label: 'Sales', hint: 'Deal pipeline and offer revenue' },
];

export default function AdvancedFinanceTab({ onOpenWizard }) {
  return (
    <div data-cy="tab-advanced">
      <div style={{ marginBottom: 28 }}>
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 6 }}>
          Finance integrations
        </div>
        <p style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 16, lineHeight: 1.55 }}>
          Finance pulls data from these modules. Visit each to configure revenue and cost tracking.
        </p>
        <div className="grid grid-2" style={{ gap: 10 }}>
          {INTEGRATION_LINKS.map((link) => (
            <a
              key={link.href}
              href={link.href}
              style={{
                display: 'block',
                padding: '12px 14px',
                borderRadius: 10,
                border: '1px solid var(--border)',
                background: 'var(--bg-elevated)',
                textDecoration: 'none',
                transition: 'border-color 0.15s',
              }}
            >
              <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)', marginBottom: 3 }}>{link.label}</div>
              <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>{link.hint}</div>
            </a>
          ))}
        </div>
      </div>

      <div style={{ marginBottom: 28 }}>
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 6 }}>
          Re-run finance setup
        </div>
        <p style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 12, lineHeight: 1.55 }}>
          Changed your business model or priorities? Run the wizard again to update your configuration.
        </p>
        <button className="btn btn-ghost" onClick={onOpenWizard} data-cy="advanced-open-wizard">
          Open setup wizard
        </button>
      </div>
    </div>
  );
}
