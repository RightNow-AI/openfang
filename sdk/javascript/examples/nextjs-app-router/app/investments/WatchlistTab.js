'use client';

import { useState } from 'react';
import WatchlistCardSimple from './cards/WatchlistCardSimple';
import WatchlistCardDetailed from './cards/WatchlistCardDetailed';

export default function WatchlistTab({ items, view, onOpenDetail, onAddSymbol }) {
  const [symbolInput, setSymbolInput] = useState('');

  function handleAdd(e) {
    e.preventDefault();
    const trimmed = symbolInput.trim();
    if (!trimmed) return;
    onAddSymbol(trimmed);
    setSymbolInput('');
  }

  return (
    <div>
      {/* Add symbol bar */}
      <form onSubmit={handleAdd} style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
        <input
          type="text"
          placeholder="AAPL, NVDA, BTC, XLK"
          value={symbolInput}
          onChange={(e) => setSymbolInput(e.target.value)}
          style={{
            flex: 1,
            padding: '9px 12px',
            borderRadius: 8,
            border: '1.5px solid var(--border)',
            background: 'var(--surface)',
            color: 'var(--text)',
            fontSize: 13,
          }}
        />
        <button className="btn btn-primary btn-sm" type="submit">Add</button>
      </form>

      {!items || items.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '48px 24px', color: 'var(--text-dim)' }}>
          <div style={{ fontSize: 14, fontWeight: 700, marginBottom: 8 }}>No watchlist yet</div>
          <p style={{ fontSize: 12 }}>
            Add a few symbols, sectors, or themes to begin.
          </p>
        </div>
      ) : (
        <div className="grid grid-2" style={{ gap: 12 }}>
          {items.map((item) =>
            view === 'detailed' ? (
              <WatchlistCardDetailed key={item.id} item={item} onOpenDetail={onOpenDetail} />
            ) : (
              <WatchlistCardSimple key={item.id} item={item} onOpenDetail={onOpenDetail} />
            )
          )}
        </div>
      )}
    </div>
  );
}
