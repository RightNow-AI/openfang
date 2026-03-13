import Link from 'next/link';

// Stub pages share a common layout — each exports a default using this helper.
export function ComingSoon({ title, description, links = [] }) {
  return (
    <div>
      <div className="page-header">
        <h1>{title}</h1>
      </div>
      <div className="page-body">
        <div className="info-card">
          <h4 style={{ margin: '0 0 8px', color: 'var(--text)' }}>{title}</h4>
          <p style={{ margin: '0 0 12px', color: 'var(--text-dim)' }}>{description}</p>
          {links.length > 0 && (
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              {links.map(l => (
                <Link key={l.href} href={l.href} className="btn btn-ghost btn-sm">{l.label}</Link>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
