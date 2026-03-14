'use client';

/**
 * Composer
 *
 * The single user input that enters everything through alive.
 * Never exposes agent selection to the user.
 *
 * Props:
 *   onSubmit(message: string)  — called when user sends
 *   disabled — boolean
 *   placeholder — string
 */

import { useState, useRef } from 'react';

export default function Composer({ onSubmit, disabled = false, placeholder = 'Ask alive anything…' }) {
  const [input, setInput] = useState('');
  const textareaRef = useRef(null);

  const canSend = input.trim().length > 0 && !disabled;

  function handleKeyDown(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (canSend) send();
    }
  }

  function send() {
    const text = input.trim();
    if (!text || disabled) return;
    setInput('');
    onSubmit(text);
    // Re-focus after React re-render
    setTimeout(() => textareaRef.current?.focus(), 0);
  }

  return (
    <div
      data-cy="composer"
      style={{
        display: 'flex',
        alignItems: 'flex-end',
        gap: 8,
        padding: '10px 12px',
        background: 'var(--bg-elevated)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius)',
      }}
    >
      {/* alive badge */}
      <span
        title="All messages route through alive"
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: 4,
          padding: '4px 8px',
          borderRadius: 99,
          fontSize: 11,
          fontWeight: 700,
          background: 'var(--accent)22',
          color: 'var(--accent)',
          border: '1px solid var(--accent)44',
          fontFamily: 'var(--font-mono, monospace)',
          flexShrink: 0,
          marginBottom: 2,
          cursor: 'default',
          userSelect: 'none',
        }}
      >
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--accent)' }} />
        alive
      </span>

      <textarea
        ref={textareaRef}
        data-cy="composer-input"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        placeholder={placeholder}
        rows={1}
        style={{
          flex: 1,
          resize: 'none',
          border: 'none',
          background: 'transparent',
          color: 'var(--text)',
          fontSize: 14,
          lineHeight: 1.5,
          outline: 'none',
          padding: '4px 0',
          minHeight: '1.5em',
          maxHeight: '8em',
          overflowY: 'auto',
          fontFamily: 'inherit',
        }}
        onInput={(e) => {
          // Auto-grow textarea
          e.target.style.height = 'auto';
          e.target.style.height = Math.min(e.target.scrollHeight, 128) + 'px';
        }}
      />

      <button
        data-cy="composer-send"
        onClick={send}
        disabled={!canSend}
        style={{
          flexShrink: 0,
          padding: '6px 14px',
          borderRadius: 'var(--radius-sm)',
          border: 'none',
          background: canSend ? 'var(--accent)' : 'var(--bg-surface)',
          color: canSend ? '#fff' : 'var(--text-dim)',
          fontSize: 13,
          fontWeight: 600,
          cursor: canSend ? 'pointer' : 'not-allowed',
          transition: 'background 0.15s',
          marginBottom: 2,
        }}
      >
        {disabled ? '…' : 'Send'}
      </button>
    </div>
  );
}
