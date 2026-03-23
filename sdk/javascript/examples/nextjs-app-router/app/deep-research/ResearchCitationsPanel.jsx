'use client';

export default function ResearchCitationsPanel({ urls = [] }) {
  return (
    <div style={{ padding: '16px', border: '1px solid var(--border-light)', borderRadius: 12, background: 'var(--surface2)' }}>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 8 }}>Citations</div>
      {urls.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.6 }}>No citation links were extracted from this result.</div>
      ) : (
        <div style={{ display: 'grid', gap: 10 }}>
          {urls.slice(0, 6).map((url) => {
            let host = url;
            try { host = new URL(url).hostname.replace(/^www\./, ''); } catch {}
            return (
              <a
                key={url}
                href={url}
                target="_blank"
                rel="noopener noreferrer"
                style={{ padding: '10px 12px', borderRadius: 10, border: '1px solid rgba(148,163,184,0.14)', color: 'var(--accent)', textDecoration: 'none', fontSize: 13, lineHeight: 1.5 }}
              >
                {host}
              </a>
            );
          })}
        </div>
      )}
    </div>
  );
}