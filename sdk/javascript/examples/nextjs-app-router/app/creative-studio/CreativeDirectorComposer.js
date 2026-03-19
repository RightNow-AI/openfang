'use client';
import { useState, useRef } from 'react';

export default function CreativeDirectorComposer({ disabled, sending, onSend }) {
  const [text, setText] = useState('');
  const textareaRef     = useRef(null);

  const handleSend = () => {
    if (!text.trim() || disabled) return;
    onSend({ text: text.trim(), imageIds: [], referenceUrls: [] });
    setText('');
    textareaRef.current?.focus();
  };

  const handleKeyDown = (e) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) handleSend();
  };

  const canSend = text.trim().length > 0 && !disabled;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <div
        style={{ display: 'flex', gap: 8, alignItems: 'flex-end', border: '1px solid var(--border)', borderRadius: 10, padding: '10px 12px', background: 'var(--bg-elevated)', transition: 'border-color .15s' }}
        onFocusCapture={e => e.currentTarget.style.borderColor = 'var(--accent)'}
        onBlurCapture={e => e.currentTarget.style.borderColor = 'var(--border)'}
      >
        <textarea
          ref={textareaRef}
          value={text}
          onChange={e => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={disabled}
          placeholder="Tell the director about your brief, or ask for direction…"
          rows={3}
          style={{ flex: 1, resize: 'none', background: 'transparent', border: 'none', outline: 'none', color: 'var(--text-primary)', fontSize: 14, lineHeight: 1.5, fontFamily: 'inherit' }}
        />
        <button
          data-cy="director-send-btn"
          onClick={handleSend}
          disabled={!canSend}
          style={{ padding: '8px 18px', borderRadius: 7, background: canSend ? 'var(--accent)' : 'var(--border)', color: '#fff', border: 'none', cursor: canSend ? 'pointer' : 'not-allowed', fontWeight: 700, fontSize: 13, flexShrink: 0, alignSelf: 'flex-end', transition: 'background .15s' }}
        >
          {sending ? '…' : 'Send'}
        </button>
      </div>
      <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>⌘↵ to send</div>
    </div>
  );
}
