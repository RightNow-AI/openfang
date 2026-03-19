'use client';
import { useRef, useEffect } from 'react';
import CreativeDirectorMessageCard  from './CreativeDirectorMessageCard';
import CreativeDirectorComposer     from './CreativeDirectorComposer';
import CreativeDirectorQuickActions from './CreativeDirectorQuickActions';

export default function CreativeDirectorPanel({ projectId, messages, isThinking, onSendMessage, onApprovePlan, onRunNextAction }) {
  const bottomRef = useRef(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, isThinking]);

  return (
    <div data-cy="director-panel" style={{ display: 'flex', flexDirection: 'column', gap: 0, height: '100%', minHeight: 0 }}>
      <CreativeDirectorQuickActions disabled={isThinking} onRunAction={onRunNextAction} />

      {/* Message list */}
      <div style={{ flex: 1, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 12, paddingBottom: 8, minHeight: 200 }}>
        {messages.length === 0 && (
          <div style={{ padding: '40px 0', textAlign: 'center', color: 'var(--text-dim)' }}>
            <div style={{ fontSize: 36, marginBottom: 12 }}>🎬</div>
            <div style={{ fontWeight: 600, fontSize: 15, marginBottom: 6 }}>Your Creative Director is ready.</div>
            <div style={{ fontSize: 13 }}>Share your brief and references, or pick a quick action above.</div>
          </div>
        )}
        {messages.map(m => (
          <CreativeDirectorMessageCard key={m.id} message={m} onUseNextAction={onRunNextAction} />
        ))}
        {isThinking && (
          <div style={{ padding: '12px 16px', borderRadius: 10, background: 'var(--surface2)', border: '1px solid var(--border)', maxWidth: 220, alignSelf: 'flex-start' }}>
            <span style={{ color: 'var(--text-dim)', fontSize: 13 }}>Director is thinking…</span>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Composer */}
      <div style={{ borderTop: '1px solid var(--border)', paddingTop: 16, flexShrink: 0 }}>
        <CreativeDirectorComposer disabled={isThinking} sending={isThinking} onSend={onSendMessage} />
      </div>
    </div>
  );
}
