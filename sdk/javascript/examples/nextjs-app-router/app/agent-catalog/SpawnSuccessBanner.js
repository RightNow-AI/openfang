'use client';

export default function SpawnSuccessBanner({ agentId, agentName, onDismiss, onOpenChat }) {
  return (
    <div
      data-cy="spawn-success-banner"
      style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        flexWrap: 'wrap', gap: 10,
        padding: '12px 16px',
        background: 'var(--success)18',
        border: '1px solid var(--success)44',
        borderRadius: 'var(--radius-sm)',
        marginBottom: 16,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ color: 'var(--success)', fontSize: 16 }}>✓</span>
        <span style={{ fontSize: 13, color: 'var(--text)' }}>
          <strong>{agentName}</strong> spawned successfully.
        </span>
        {agentId && (
          <span style={{ fontSize: 11, fontFamily: 'var(--font-mono,monospace)', color: 'var(--text-dim)' }}>
            {agentId.slice(0, 8)}…
          </span>
        )}
      </div>
      <div style={{ display: 'flex', gap: 8 }}>
        <button
          data-cy="spawn-open-chat-btn"
          className="btn btn-primary btn-sm"
          onClick={() => onOpenChat(agentId, agentName)}
        >
          Open Chat →
        </button>
        <button className="btn btn-ghost btn-sm" onClick={onDismiss}>Dismiss</button>
      </div>
    </div>
  );
}
