'use client';
import CreativeProjectCardSimple from './cards/CreativeProjectCardSimple';

export default function MyProjectsTab({ projects, loading, error, onOpen, onRefresh }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div style={{ fontWeight: 700, fontSize: 15 }}>My creative projects</div>
        <button className="btn btn-ghost btn-sm" onClick={onRefresh} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
      </div>
      {error && <div className="error-state">⚠ {error}</div>}
      {!loading && !error && projects.length === 0 && (
        <div className="empty-state">
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: 32, marginBottom: 8 }}>🎨</div>
            <div>No creative projects yet. Start one from the Recommended tab.</div>
          </div>
        </div>
      )}
      {projects.length > 0 && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))', gap: 12 }}>
          {projects.map(p => (
            <CreativeProjectCardSimple key={p.id} project={p} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}
