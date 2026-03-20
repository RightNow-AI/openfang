'use client';

/**
 * DeepResearchClient — MiroFish-inspired deep research interface.
 *
 * Pipeline stages (borrowed from MiroFish's swarm sim workflow, adapted to
 * single-agent research):
 *   1. Decompose  — break question into sub-queries
 *   2. Search     — locate sources
 *   3. Deep Dive  — read sources in full
 *   4. Cross-Ref  — validate & compare
 *   5. Synthesize — build the report
 *
 * API flow:
 *   GET  /api/agents          → find researcher agent by name
 *   POST /api/agents/{id}/chat → run the research (direct, synchronous)
 *   POST /api/agents/{id}/chat → follow-up Q&A (same agent, retains context)
 */

import { useState, useRef, useCallback, useEffect } from 'react';

// ─── Pipeline stages ──────────────────────────────────────────────────────────

const STAGES = [
  { id: 'decompose',  label: 'Decompose',       icon: '🧩', desc: 'Breaking into sub-questions'       },
  { id: 'search',     label: 'Search',           icon: '🔍', desc: 'Locating relevant sources'         },
  { id: 'deepdive',   label: 'Deep Dive',        icon: '📖', desc: 'Reading sources in full'           },
  { id: 'crossref',   label: 'Cross-Reference',  icon: '⚖️',  desc: 'Validating across sources'        },
  { id: 'synthesize', label: 'Synthesize',       icon: '📊', desc: 'Building the research report'     },
];

// Average ms each stage takes (rough pacing to feel realistic while backend runs)
const STAGE_DURATIONS = [4000, 8000, 12000, 6000, 8000];

// ─── Report parser ────────────────────────────────────────────────────────────

function parseReport(text) {
  const sections = { raw: text };

  const findingsM = text.match(/#{1,3}\s*Key Findings([\s\S]*?)(?=#{1,3}|\n---|\*\*Sources|$)/i);
  if (findingsM) sections.findings = findingsM[1].trim();

  const sourcesM = text.match(/#{1,3}\s*Sources Used([\s\S]*?)(?=#{1,3}|\n---|\*\*Confidence|$)/i);
  if (sourcesM) {
    const raw = sourcesM[1].trim();
    const urlRe = /https?:\/\/[^\s)\]>"',]+/g;
    sections.sourceUrls = [...new Set(raw.match(urlRe) || [])];
    sections.sourcesRaw = raw;
  }

  const confM = text.match(/#{1,3}\s*Confidence Level[:\s]*([\s\S]*?)(?=#{1,3}|\n---|\*\*Open|$)/i);
  if (confM) sections.confidence = confM[1].trim().split('\n')[0].trim();

  const openM = text.match(/#{1,3}\s*Open Questions([\s\S]*?)(?=#{1,3}|\n---|$)/i);
  if (openM) sections.openQuestions = openM[1].trim();

  // Lead answer = first non-blank paragraph before any ## heading
  const leadM = text.match(/^(?!#)([\s\S]+?)(?=\n#{1,3}|\n---)/);
  if (leadM) sections.lead = leadM[1].trim();

  return sections;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function ConfidenceBadge({ text }) {
  if (!text) return null;
  const lower = text.toLowerCase();
  const color = lower.includes('high')
    ? '#22c55e'
    : lower.includes('medium') || lower.includes('moderate')
    ? '#f59e0b'
    : '#ef4444';
  return (
    <span style={{
      display: 'inline-block',
      background: color + '22',
      color,
      border: `1px solid ${color}44`,
      borderRadius: 6,
      padding: '2px 10px',
      fontSize: 12,
      fontWeight: 600,
      letterSpacing: '0.04em',
    }}>
      {text}
    </span>
  );
}

function SourceCard({ url }) {
  let domain = url;
  try { domain = new URL(url).hostname.replace(/^www\./, ''); } catch {}
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        padding: '6px 10px',
        background: 'var(--surface2)',
        border: '1px solid var(--border-light)',
        borderRadius: 8,
        color: 'var(--accent)',
        fontSize: 12,
        textDecoration: 'none',
        overflow: 'hidden',
        whiteSpace: 'nowrap',
        textOverflow: 'ellipsis',
        maxWidth: 280,
      }}
    >
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
        <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
      </svg>
      {domain}
    </a>
  );
}

// Minimal markdown → text rendering (bold, numbered lists, bullets)
function MiniMarkdown({ text }) {
  if (!text) return null;
  const lines = text.split('\n');
  const elements = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { elements.push(<br key={i} />); i++; continue; }
    // numbered item
    const numM = line.match(/^(\d+)\.\s+(.*)/);
    if (numM) {
      const bold = numM[2].replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
      elements.push(
        <div key={i} style={{ display: 'flex', gap: 8, marginBottom: 6, lineHeight: 1.6 }}>
          <span style={{ color: 'var(--accent)', fontWeight: 700, minWidth: 20 }}>{numM[1]}.</span>
          <span dangerouslySetInnerHTML={{ __html: bold }} />
        </div>
      );
      i++; continue;
    }
    // bullet
    if (line.startsWith('- ') || line.startsWith('* ')) {
      const bold = line.slice(2).replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
      elements.push(
        <div key={i} style={{ display: 'flex', gap: 8, marginBottom: 4, lineHeight: 1.6 }}>
          <span style={{ color: 'var(--accent)', marginTop: 2 }}>·</span>
          <span dangerouslySetInnerHTML={{ __html: bold }} />
        </div>
      );
      i++; continue;
    }
    // heading inside section
    if (line.startsWith('#')) {
      const bold = line.replace(/^#+\s*/, '').replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
      elements.push(<p key={i} style={{ fontWeight: 700, marginBottom: 4 }} dangerouslySetInnerHTML={{ __html: bold }} />);
      i++; continue;
    }
    const bold = line.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    elements.push(<p key={i} style={{ marginBottom: 4, lineHeight: 1.65 }} dangerouslySetInnerHTML={{ __html: bold }} />);
    i++;
  }
  return <div style={{ fontSize: 14, color: 'var(--text)' }}>{elements}</div>;
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function DeepResearchClient() {
  const [query, setQuery]         = useState('');
  const [seedUrls, setSeedUrls]  = useState('');
  const [phase, setPhase]        = useState('idle'); // idle | running | done | error
  const [stageIdx, setStageIdx]  = useState(-1);
  const [report, setReport]      = useState(null);
  const [rawReply, setRawReply]  = useState('');
  const [errMsg, setErrMsg]      = useState('');
  const [agentId, setAgentId]    = useState(null);
  const [agentName, setAgentName] = useState('Researcher');
  const [history, setHistory]    = useState([]); // Q&A follow-ups
  const [followUp, setFollowUp]  = useState('');
  const [followLoading, setFollowLoading] = useState(false);
  const [activeTab, setActiveTab] = useState('findings'); // findings | sources | open

  const stageTimerRef = useRef(null);
  const abortRef      = useRef(null);
  const bottomRef     = useRef(null);

  // ── Fetch researcher agent on mount ──────────────────────────────────────
  useEffect(() => {
    const base = process.env.NEXT_PUBLIC_OPENFANG_BASE_URL || 'http://127.0.0.1:50051';
    fetch(`${base}/api/agents`)
      .then(r => r.ok ? r.json() : [])
      .then(agents => {
        if (!Array.isArray(agents)) return;
        const a = agents.find(x =>
          (x.name ?? x.id ?? '').toLowerCase().includes('researcher')
        );
        if (a) { setAgentId(a.id); setAgentName(a.name ?? 'Researcher'); }
      })
      .catch(() => {});
  }, []);

  // ── Stage animation while running ────────────────────────────────────────
  function startStageAnimation() {
    setStageIdx(0);
    let current = 0;
    function advance() {
      current++;
      if (current < STAGES.length) {
        setStageIdx(current);
        stageTimerRef.current = setTimeout(advance, STAGE_DURATIONS[current]);
      }
    }
    stageTimerRef.current = setTimeout(advance, STAGE_DURATIONS[0]);
  }

  function stopStageAnimation() {
    clearTimeout(stageTimerRef.current);
    setStageIdx(STAGES.length - 1); // mark all done
  }

  // ── Run research ──────────────────────────────────────────────────────────
  const handleResearch = useCallback(async () => {
    const q = query.trim();
    if (!q || phase === 'running') return;

    setPhase('running');
    setReport(null);
    setRawReply('');
    setErrMsg('');
    setHistory([]);
    setActiveTab('findings');
    startStageAnimation();

    // Build the full message with optional seed URLs
    const urlLines = seedUrls.trim()
      .split('\n')
      .map(l => l.trim())
      .filter(l => l.startsWith('http'));

    const prompt = urlLines.length > 0
      ? `Research the following question thoroughly:\n\n${q}\n\nSeed sources to start from (fetch these first):\n${urlLines.map(u => '- ' + u).join('\n')}`
      : q;

    try {
      abortRef.current = new AbortController();

      let reply = '';
      if (agentId) {
        const r = await fetch(`/api/agents/${encodeURIComponent(agentId)}/chat`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ message: prompt }),
          signal: abortRef.current.signal,
        });
        const data = await r.json().catch(() => ({}));
        if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`);
        reply = data.reply ?? '';
      } else {
        // Fallback: route through alive with researcher-framed request
        const r = await fetch('/api/runs', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ message: `[RESEARCH REQUEST] ${prompt}` }),
          signal: abortRef.current.signal,
        });
        const data = await r.json().catch(() => ({}));
        if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`);
        // For runs, wait and poll... actually just show that it was dispatched
        reply = `Research dispatched via orchestrator (run ID: ${data.runId}). Check Sessions for results.`;
      }

      stopStageAnimation();
      setRawReply(reply);
      setReport(parseReport(reply));
      setPhase('done');
    } catch (err) {
      stopStageAnimation();
      if (err.name === 'AbortError') {
        setPhase('idle');
      } else {
        setErrMsg(err.message || 'Research failed');
        setPhase('error');
      }
    }
  }, [query, seedUrls, phase, agentId]);

  // ── Follow-up Q&A ─────────────────────────────────────────────────────────
  const handleFollowUp = useCallback(async () => {
    const q = followUp.trim();
    if (!q || followLoading || !agentId) return;
    setFollowLoading(true);
    const q2 = q;
    setFollowUp('');
    setHistory(h => [...h, { q: q2, a: null, loading: true }]);
    try {
      const r = await fetch(`/api/agents/${encodeURIComponent(agentId)}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: q2 }),
      });
      const data = await r.json().catch(() => ({}));
      const answer = r.ok ? (data.reply ?? '') : (data.error ?? `HTTP ${r.status}`);
      setHistory(h => h.map((item, i) => i === h.length - 1 ? { ...item, a: answer, loading: false } : item));
    } catch (err) {
      setHistory(h => h.map((item, i) => i === h.length - 1 ? { ...item, a: err.message, loading: false } : item));
    } finally {
      setFollowLoading(false);
      setTimeout(() => bottomRef.current?.scrollIntoView({ behavior: 'smooth' }), 100);
    }
  }, [followUp, followLoading, agentId]);

  const handleStop = () => {
    abortRef.current?.abort();
    clearTimeout(stageTimerRef.current);
    setPhase('idle');
  };

  const handleReset = () => {
    handleStop();
    setQuery('');
    setSeedUrls('');
    setReport(null);
    setRawReply('');
    setHistory([]);
    setStageIdx(-1);
    setErrMsg('');
  };

  // ── Render ────────────────────────────────────────────────────────────────
  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>

      {/* ── Left: Input + Pipeline ─────────────────────────────────────── */}
      <div style={{
        width: 320,
        minWidth: 280,
        flexShrink: 0,
        borderRight: '1px solid var(--border-light)',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        background: 'var(--surface)',
      }}>
        {/* Header */}
        <div style={{ padding: '20px 20px 16px', borderBottom: '1px solid var(--border-light)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
            <span style={{ fontSize: 18 }}>🔬</span>
            <h2 style={{ margin: 0, fontSize: 15, fontWeight: 700 }}>Deep Research</h2>
          </div>
          <p style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)' }}>
            Multi-step agent research with source synthesis
          </p>
          {agentId && (
            <div style={{ marginTop: 8, display: 'flex', alignItems: 'center', gap: 5 }}>
              <span style={{ width: 6, height: 6, borderRadius: '50%', background: '#22c55e', display: 'inline-block' }} />
              <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>{agentName} agent ready</span>
            </div>
          )}
          {!agentId && (
            <div style={{ marginTop: 8, display: 'flex', alignItems: 'center', gap: 5 }}>
              <span style={{ width: 6, height: 6, borderRadius: '50%', background: '#f59e0b', display: 'inline-block' }} />
              <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Fallback mode (no Researcher agent found)</span>
            </div>
          )}
        </div>

        {/* Query input */}
        <div style={{ padding: '16px 20px 0', flexShrink: 0 }}>
          <label style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
            Research Question
          </label>
          <textarea
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="What do you want to research deeply? Be specific — the more context the better."
            disabled={phase === 'running'}
            rows={5}
            style={{
              width: '100%',
              marginTop: 6,
              padding: '10px 12px',
              background: 'var(--surface2)',
              border: '1px solid var(--border-light)',
              borderRadius: 8,
              color: 'var(--text)',
              fontSize: 13,
              resize: 'vertical',
              boxSizing: 'border-box',
              outline: 'none',
              lineHeight: 1.55,
              opacity: phase === 'running' ? 0.6 : 1,
            }}
            onKeyDown={e => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) handleResearch();
            }}
          />
        </div>

        {/* Seed URLs (optional) */}
        <div style={{ padding: '12px 20px 0', flexShrink: 0 }}>
          <label style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
            Seed Sources <span style={{ fontWeight: 400, textTransform: 'none' }}>(optional, one URL per line)</span>
          </label>
          <textarea
            value={seedUrls}
            onChange={e => setSeedUrls(e.target.value)}
            placeholder="https://example.com/article&#10;https://arxiv.org/..."
            disabled={phase === 'running'}
            rows={3}
            style={{
              width: '100%',
              marginTop: 6,
              padding: '8px 12px',
              background: 'var(--surface2)',
              border: '1px solid var(--border-light)',
              borderRadius: 8,
              color: 'var(--text)',
              fontSize: 12,
              fontFamily: 'monospace',
              resize: 'none',
              boxSizing: 'border-box',
              outline: 'none',
              opacity: phase === 'running' ? 0.6 : 1,
            }}
          />
        </div>

        {/* Action buttons */}
        <div style={{ padding: '12px 20px 0', flexShrink: 0, display: 'flex', gap: 8 }}>
          {phase !== 'running' ? (
            <button
              onClick={handleResearch}
              disabled={!query.trim()}
              style={{
                flex: 1,
                padding: '9px 0',
                background: query.trim() ? 'var(--accent)' : 'var(--surface2)',
                color: query.trim() ? '#fff' : 'var(--text-dim)',
                border: 'none',
                borderRadius: 8,
                fontWeight: 600,
                fontSize: 13,
                cursor: query.trim() ? 'pointer' : 'not-allowed',
                transition: 'background 0.15s',
              }}
            >
              {phase === 'done' ? '🔬 Re-research' : '🔬 Start Research'}
            </button>
          ) : (
            <button
              onClick={handleStop}
              style={{
                flex: 1,
                padding: '9px 0',
                background: '#ef4444',
                color: '#fff',
                border: 'none',
                borderRadius: 8,
                fontWeight: 600,
                fontSize: 13,
                cursor: 'pointer',
              }}
            >
              ⏹ Stop
            </button>
          )}
          {phase !== 'idle' && (
            <button
              onClick={handleReset}
              style={{
                padding: '9px 14px',
                background: 'var(--surface2)',
                color: 'var(--text-dim)',
                border: '1px solid var(--border-light)',
                borderRadius: 8,
                fontSize: 12,
                cursor: 'pointer',
              }}
            >
              Reset
            </button>
          )}
        </div>

        {/* Pipeline stages */}
        <div style={{ padding: '20px 20px 0', flex: 1, overflow: 'auto' }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 12 }}>
            Research Pipeline
          </div>
          {STAGES.map((s, idx) => {
            const isDone    = stageIdx > idx && phase !== 'idle';
            const isActive  = stageIdx === idx && phase === 'running';
            const isPending = stageIdx < idx || phase === 'idle';
            return (
              <div
                key={s.id}
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 10,
                  marginBottom: 14,
                  opacity: isPending ? 0.4 : 1,
                  transition: 'opacity 0.3s',
                }}
              >
                {/* Stage icon / indicator */}
                <div style={{
                  width: 32,
                  height: 32,
                  borderRadius: '50%',
                  border: `2px solid ${isDone ? '#22c55e' : isActive ? 'var(--accent)' : 'var(--border-light)'}`,
                  background: isDone ? '#22c55e22' : isActive ? 'var(--accent)22' : 'transparent',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: 14,
                  flexShrink: 0,
                  position: 'relative',
                }}>
                  {isDone
                    ? <span style={{ color: '#22c55e', fontSize: 14 }}>✓</span>
                    : isActive
                    ? <span style={{ animation: 'spin 1s linear infinite', display: 'inline-block', fontSize: 14 }}>⟳</span>
                    : <span>{s.icon}</span>
                  }
                </div>
                {/* Label + description */}
                <div style={{ paddingTop: 4 }}>
                  <div style={{
                    fontSize: 13,
                    fontWeight: isActive ? 700 : 600,
                    color: isDone ? '#22c55e' : isActive ? 'var(--accent)' : 'var(--text)',
                  }}>
                    {s.label}
                  </div>
                  <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 1 }}>
                    {isActive ? <em>{s.desc}…</em> : s.desc}
                  </div>
                </div>
                {idx < STAGES.length - 1 && (
                  <div style={{
                    position: 'absolute',
                    left: 35,
                    width: 2,
                    height: 14,
                    background: isDone ? '#22c55e' : 'var(--border-light)',
                    marginTop: 32,
                    display: 'none', // connector line — hidden, using gap instead
                  }} />
                )}
              </div>
            );
          })}
        </div>

        {/* Cmd+Enter hint */}
        <div style={{ padding: '8px 20px 16px', fontSize: 10, color: 'var(--text-dim)', textAlign: 'center' }}>
          ⌘ Enter to start research
        </div>
      </div>

      {/* ── Right: Report + Q&A ───────────────────────────────────────────── */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>

        {/* ── Idle state ─────────────────────────────────────────────────── */}
        {phase === 'idle' && (
          <div style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            color: 'var(--text-dim)',
            gap: 16,
            padding: 40,
          }}>
            <div style={{ fontSize: 64 }}>🔬</div>
            <div style={{ textAlign: 'center', maxWidth: 400 }}>
              <h3 style={{ margin: '0 0 8px', fontSize: 18, color: 'var(--text)' }}>Deep Research</h3>
              <p style={{ margin: 0, fontSize: 14, lineHeight: 1.65 }}>
                Enter a research question on the left. The agent will decompose it, search the web,
                cross-reference sources, and synthesize a structured report — with source citations
                and a confidence rating.
              </p>
            </div>
            <div style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: 8,
              maxWidth: 460,
              width: '100%',
            }}>
              {[
                'What are the latest breakthroughs in quantum computing?',
                'Compare the top 5 LLM providers for enterprise use in 2026',
                'What is the current state of cold fusion research?',
                'How does Rust memory safety compare to C++ in production systems?',
              ].map(ex => (
                <button
                  key={ex}
                  onClick={() => setQuery(ex)}
                  style={{
                    padding: '10px 12px',
                    background: 'var(--surface2)',
                    border: '1px solid var(--border-light)',
                    borderRadius: 8,
                    color: 'var(--text)',
                    fontSize: 12,
                    textAlign: 'left',
                    cursor: 'pointer',
                    lineHeight: 1.4,
                  }}
                >
                  {ex}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* ── Running state ──────────────────────────────────────────────── */}
        {phase === 'running' && (
          <div style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 20,
            padding: 40,
          }}>
            <div style={{ textAlign: 'center' }}>
              <div style={{ fontSize: 48, marginBottom: 16 }}>
                {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].icon : '🔬'}
              </div>
              <h3 style={{ margin: '0 0 6px', fontSize: 17, color: 'var(--text)' }}>
                {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].label + '…' : 'Starting…'}
              </h3>
              <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)', maxWidth: 340, lineHeight: 1.6 }}>
                {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].desc : 'Initializing research pipeline'}
              </p>
            </div>

            {/* Animated progress bar */}
            <div style={{ width: '100%', maxWidth: 480, background: 'var(--surface2)', borderRadius: 8, height: 6, overflow: 'hidden' }}>
              <div style={{
                height: '100%',
                background: 'var(--accent)',
                borderRadius: 8,
                width: `${((stageIdx + 1) / STAGES.length) * 100}%`,
                transition: 'width 0.8s ease',
              }} />
            </div>

            {/* Stage dots */}
            <div style={{ display: 'flex', gap: 8 }}>
              {STAGES.map((s, i) => (
                <div
                  key={s.id}
                  title={s.label}
                  style={{
                    width: i <= stageIdx ? 10 : 8,
                    height: i <= stageIdx ? 10 : 8,
                    borderRadius: '50%',
                    background: i < stageIdx
                      ? '#22c55e'
                      : i === stageIdx
                      ? 'var(--accent)'
                      : 'var(--border-light)',
                    transition: 'all 0.3s',
                  }}
                />
              ))}
            </div>

            <p style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 4 }}>
              Researching: <em style={{ color: 'var(--text)' }}>
                {query.length > 80 ? query.slice(0, 80) + '…' : query}
              </em>
            </p>
          </div>
        )}

        {/* ── Error state ────────────────────────────────────────────────── */}
        {phase === 'error' && (
          <div style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 12,
            color: 'var(--text-dim)',
          }}>
            <div style={{ fontSize: 48 }}>⚠️</div>
            <h3 style={{ margin: 0, color: '#ef4444' }}>Research failed</h3>
            <p style={{ margin: 0, fontSize: 13 }}>{errMsg}</p>
            <button
              onClick={handleReset}
              style={{ marginTop: 8, padding: '8px 20px', background: 'var(--accent)', color: '#fff', border: 'none', borderRadius: 8, cursor: 'pointer', fontWeight: 600 }}
            >
              Try Again
            </button>
          </div>
        )}

        {/* ── Done: Report ───────────────────────────────────────────────── */}
        {phase === 'done' && report && (
          <div style={{ flex: 1, overflow: 'auto', display: 'flex', flexDirection: 'column' }}>

            {/* Report header */}
            <div style={{
              padding: '20px 28px 16px',
              borderBottom: '1px solid var(--border-light)',
              background: 'var(--surface)',
              flexShrink: 0,
            }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12 }}>
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                    <span style={{ fontSize: 16 }}>📋</span>
                    <h3 style={{ margin: 0, fontSize: 15, fontWeight: 700 }}>Research Report</h3>
                    {report.confidence && <ConfidenceBadge text={report.confidence} />}
                  </div>
                  <p style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)', maxWidth: 600 }}>
                    {query.length > 100 ? query.slice(0, 100) + '…' : query}
                  </p>
                </div>
                <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                  <button
                    onClick={() => { setQuery(query); handleResearch(); }}
                    title="Re-run this research"
                    style={{ padding: '6px 12px', background: 'var(--surface2)', border: '1px solid var(--border-light)', borderRadius: 6, fontSize: 12, cursor: 'pointer', color: 'var(--text)' }}
                  >
                    ↻ Refresh
                  </button>
                  <button
                    onClick={() => navigator.clipboard?.writeText(rawReply)}
                    title="Copy raw report"
                    style={{ padding: '6px 12px', background: 'var(--surface2)', border: '1px solid var(--border-light)', borderRadius: 6, fontSize: 12, cursor: 'pointer', color: 'var(--text)' }}
                  >
                    📋 Copy
                  </button>
                </div>
              </div>

              {/* Tab bar */}
              <div style={{ display: 'flex', gap: 2, marginTop: 14 }}>
                {[
                  { id: 'findings', label: '📌 Key Findings' },
                  { id: 'sources',  label: '🔗 Sources' + (report.sourceUrls?.length ? ` (${report.sourceUrls.length})` : '') },
                  { id: 'open',     label: '❓ Open Questions' },
                  { id: 'raw',      label: '📄 Full Report' },
                ].map(t => (
                  <button
                    key={t.id}
                    onClick={() => setActiveTab(t.id)}
                    style={{
                      padding: '5px 12px',
                      background: activeTab === t.id ? 'var(--accent)' : 'transparent',
                      color: activeTab === t.id ? '#fff' : 'var(--text-dim)',
                      border: activeTab === t.id ? 'none' : '1px solid var(--border-light)',
                      borderRadius: 6,
                      fontSize: 12,
                      cursor: 'pointer',
                      fontWeight: activeTab === t.id ? 600 : 400,
                    }}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
            </div>

            {/* Tab content */}
            <div style={{ flex: 1, overflow: 'auto', padding: '20px 28px' }}>

              {/* Lead answer */}
              {report.lead && activeTab === 'findings' && (
                <div style={{
                  padding: '14px 16px',
                  background: 'var(--accent)11',
                  border: '1px solid var(--accent)33',
                  borderRadius: 10,
                  marginBottom: 20,
                  fontSize: 14,
                  lineHeight: 1.65,
                  color: 'var(--text)',
                }}>
                  {report.lead}
                </div>
              )}

              {activeTab === 'findings' && (
                <MiniMarkdown text={report.findings || rawReply} />
              )}

              {activeTab === 'sources' && (
                <div>
                  {report.sourceUrls?.length > 0 ? (
                    <>
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 20 }}>
                        {report.sourceUrls.map(url => <SourceCard key={url} url={url} />)}
                      </div>
                      <MiniMarkdown text={report.sourcesRaw} />
                    </>
                  ) : (
                    <div style={{ color: 'var(--text-dim)', fontSize: 13 }}>
                      <MiniMarkdown text={report.sourcesRaw || 'No sources extracted.'} />
                    </div>
                  )}
                </div>
              )}

              {activeTab === 'open' && (
                <MiniMarkdown text={report.openQuestions || '_No open questions section found in the report._'} />
              )}

              {activeTab === 'raw' && (
                <pre style={{
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  fontSize: 13,
                  lineHeight: 1.7,
                  color: 'var(--text)',
                  margin: 0,
                  fontFamily: 'inherit',
                }}>
                  {rawReply}
                </pre>
              )}
            </div>

            {/* ── Q&A Follow-up ──────────────────────────────────────────── */}
            <div style={{
              borderTop: '1px solid var(--border-light)',
              background: 'var(--surface)',
              flexShrink: 0,
            }}>
              {/* History */}
              {history.length > 0 && (
                <div style={{ maxHeight: 320, overflow: 'auto', padding: '12px 28px 0' }}>
                  {history.map((item, i) => (
                    <div key={i} style={{ marginBottom: 14 }}>
                      <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)', marginBottom: 4 }}>
                        Q: {item.q}
                      </div>
                      {item.loading ? (
                        <div style={{ fontSize: 12, color: 'var(--text-dim)', fontStyle: 'italic' }}>Thinking…</div>
                      ) : (
                        <div style={{
                          padding: '10px 12px',
                          background: 'var(--surface2)',
                          borderRadius: 8,
                          fontSize: 13,
                          lineHeight: 1.65,
                          whiteSpace: 'pre-wrap',
                        }}>
                          {item.a}
                        </div>
                      )}
                    </div>
                  ))}
                  <div ref={bottomRef} />
                </div>
              )}

              {/* Follow-up input */}
              <div style={{ padding: '12px 28px 16px', display: 'flex', gap: 8, alignItems: 'flex-end' }}>
                <div style={{ flex: 1 }}>
                  <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 4 }}>
                    💬 Ask a follow-up question about this research
                  </div>
                  <textarea
                    value={followUp}
                    onChange={e => setFollowUp(e.target.value)}
                    placeholder={agentId
                      ? 'Dig deeper, request a comparison, ask for more detail on a specific finding…'
                      : 'Researcher agent not available — follow-up requires a researcher agent'}
                    disabled={!agentId || followLoading}
                    rows={2}
                    onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleFollowUp(); } }}
                    style={{
                      width: '100%',
                      padding: '9px 12px',
                      background: 'var(--surface2)',
                      border: '1px solid var(--border-light)',
                      borderRadius: 8,
                      color: 'var(--text)',
                      fontSize: 13,
                      resize: 'none',
                      boxSizing: 'border-box',
                      outline: 'none',
                      lineHeight: 1.5,
                    }}
                  />
                </div>
                <button
                  onClick={handleFollowUp}
                  disabled={!followUp.trim() || !agentId || followLoading}
                  style={{
                    padding: '9px 16px',
                    background: followUp.trim() && agentId ? 'var(--accent)' : 'var(--surface2)',
                    color: followUp.trim() && agentId ? '#fff' : 'var(--text-dim)',
                    border: 'none',
                    borderRadius: 8,
                    fontWeight: 600,
                    fontSize: 13,
                    cursor: followUp.trim() && agentId ? 'pointer' : 'not-allowed',
                    flexShrink: 0,
                    marginBottom: 1,
                  }}
                >
                  {followLoading ? '…' : 'Ask'}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* ── CSS for spin animation ─────────────────────────────────────── */}
      <style>{`
        @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}
