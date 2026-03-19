'use client';

import ResearchSummaryCard from './cards/ResearchSummaryCard';
import CatalystCalendarCard from './cards/CatalystCalendarCard';

export default function ResearchTab({ research, catalysts, onOpenThesis, onRefreshResearch, refreshing }) {
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 18 }}>
        <div>
          <h2 style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)', marginBottom: 2 }}>What changed</h2>
          <p style={{ fontSize: 11, color: 'var(--text-dim)' }}>
            Latest research summaries generated for your watchlist.
          </p>
        </div>
        <button
          className="btn btn-ghost btn-sm"
          onClick={onRefreshResearch}
          disabled={refreshing}
        >
          {refreshing ? (
            <span className="spinner" style={{ width: 13, height: 13 }} />
          ) : '↻ Refresh'}
        </button>
      </div>

      {!research || research.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '40px 20px', color: 'var(--text-dim)' }}>
          <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 6 }}>
            No research summaries yet
          </div>
          <p style={{ fontSize: 12 }}>
            Add symbols to your watchlist and the agent will start generating research.
          </p>
        </div>
      ) : (
        <div className="grid grid-2" style={{ gap: 14, marginBottom: 20 }}>
          {research.map((r) => (
            <ResearchSummaryCard key={r.id} research={r} onOpenThesis={onOpenThesis} />
          ))}
        </div>
      )}

      {catalysts && catalysts.length > 0 && (
        <CatalystCalendarCard catalysts={catalysts} />
      )}
    </div>
  );
}
