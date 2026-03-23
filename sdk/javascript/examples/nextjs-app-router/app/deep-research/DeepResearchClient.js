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
import { useSearchParams } from 'next/navigation';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import FounderPlaybookChips from './FounderPlaybookChips';
import ResearchCitationsPanel from './ResearchCitationsPanel';
import ResearchDeliverablePanel from './ResearchDeliverablePanel';
import ResearchNextActionsCard from './ResearchNextActionsCard';
import ResearchStatusCard from './ResearchStatusCard';

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
const RUN_EVENT_TYPES = [
  'run.started',
  'run.routed',
  'run.token',
  'run.phase',
  'run.tool',
  'run.status',
  'run.completed',
  'run.failed',
];

// ─── Report parser ────────────────────────────────────────────────────────────

function extractSection(text, heading) {
  const escapedHeading = heading.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(`#{1,3}\\s*${escapedHeading}([\\s\\S]*?)(?=#{1,3}|\\n---|$)`, 'i');
  const match = text.match(regex);
  return match ? match[1].trim() : null;
}

function extractUniqueUrls(text) {
  if (!text) return [];
  const urlRe = /https?:\/\/[^\s)\]>"',]+/g;
  return [...new Set(text.match(urlRe) || [])];
}

function countStructuredItems(text) {
  if (!text) return 0;
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^([-*+]\s+|\d+\.\s+)/.test(line)).length;
}

function sectionLines(text) {
  if (!text) return [];
  return text
    .split('\n')
    .map((line) => line.replace(/^[-*+]\s+/, '').replace(/^\d+\.\s+/, '').trim())
    .filter(Boolean);
}

function parseReport(text) {
  const sections = { raw: text };

  const findings = extractSection(text, 'Key Findings');
  if (findings) sections.findings = findings;

  const sources = extractSection(text, 'Sources Used');
  if (sources) {
    const raw = sources.trim();
    sections.sourceUrls = extractUniqueUrls(raw);
    sections.sourcesRaw = raw;
  }

  const confM = text.match(/#{1,3}\s*Confidence Level[:\s]*([\s\S]*?)(?=#{1,3}|\n---|\*\*Open|$)/i);
  if (confM) sections.confidence = confM[1].trim().split('\n')[0].trim();

  const openQuestions = extractSection(text, 'Open Questions');
  if (openQuestions) sections.openQuestions = openQuestions;

  const citations = extractSection(text, 'Citations');
  if (citations) {
    sections.citations = citations;
    sections.citationUrls = extractUniqueUrls(citations);
    sections.citationCount = countStructuredItems(citations) || sections.citationUrls.length;
  }

  const nextActions = extractSection(text, 'Next Actions');
  if (nextActions) {
    sections.nextActions = nextActions;
    sections.nextActionCount = countStructuredItems(nextActions);
  }

  // Lead answer = first non-blank paragraph before any ## heading
  const leadM = text.match(/^(?!#)([\s\S]+?)(?=\n#{1,3}|\n---)/);
  if (leadM) sections.lead = leadM[1].trim();

  return sections;
}

function hasMeaningfulReportContent(text) {
  return typeof text === 'string' && text.trim().length > 0;
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
function MarkdownBlock({ text }) {
  if (!text) return null;
  return (
    <div style={{ fontSize: 14, color: 'var(--text)', lineHeight: 1.7 }}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: (props) => <h1 style={{ fontSize: 24, margin: '0 0 14px' }} {...props} />,
          h2: (props) => <h2 style={{ fontSize: 18, margin: '20px 0 10px' }} {...props} />,
          h3: (props) => <h3 style={{ fontSize: 15, margin: '18px 0 8px' }} {...props} />,
          p: (props) => <p style={{ margin: '0 0 10px' }} {...props} />,
          ul: (props) => <ul style={{ margin: '0 0 12px', paddingLeft: 22 }} {...props} />,
          ol: (props) => <ol style={{ margin: '0 0 12px', paddingLeft: 22 }} {...props} />,
          li: (props) => <li style={{ marginBottom: 4 }} {...props} />,
          a: (props) => <a target="_blank" rel="noopener noreferrer" style={{ color: 'var(--accent)' }} {...props} />,
          blockquote: (props) => <blockquote style={{ margin: '0 0 12px', paddingLeft: 14, borderLeft: '3px solid var(--border-light)', color: 'var(--text-dim)' }} {...props} />,
          code: ({ inline, className, children, ...props }) => inline
            ? <code className={className} style={{ background: 'var(--surface2)', padding: '1px 5px', borderRadius: 4, fontSize: 13 }} {...props}>{children}</code>
            : <code className={className} style={{ fontSize: 13 }} {...props}>{children}</code>,
          pre: (props) => <pre style={{ margin: '0 0 12px', padding: '12px 14px', borderRadius: 10, background: 'var(--surface2)', overflowX: 'auto' }} {...props} />,
          strong: (props) => <strong style={{ color: 'var(--text)' }} {...props} />,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function DeepResearchClient() {
  const searchParams = useSearchParams();
  const [query, setQuery]         = useState('');
  const [seedUrls, setSeedUrls]  = useState('');
  const [selectedPlaybook, setSelectedPlaybook] = useState(null);
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
  const [runId, setRunId]        = useState(null);
  const [runSnapshot, setRunSnapshot] = useState(null);
  const [autoScrollPinned, setAutoScrollPinned] = useState(true);
  const [founderWorkspace, setFounderWorkspace] = useState(null);
  const [founderRuns, setFounderRuns] = useState([]);

  const workspaceId = searchParams.get('workspaceId')?.trim() || null;
  const clientId = searchParams.get('clientId')?.trim() || null;
  const clientName = searchParams.get('clientName')?.trim() || null;
  const requestedPlaybookId = searchParams.get('playbookId')?.trim() || null;
  const requestedRunId = searchParams.get('runId')?.trim() || null;
  const draftQuery = searchParams.get('draftQuery')?.trim() || null;
  const autoStart = searchParams.get('autoStart') === '1';

  const stageTimerRef = useRef(null);
  const abortRef      = useRef(null);
  const sseRef        = useRef(null);
  const bottomRef     = useRef(null);
  const rawReplyRef   = useRef('');
  const reportPaneRef = useRef(null);
  const autoStartRef  = useRef(false);

  const setReportText = useCallback((nextText) => {
    rawReplyRef.current = nextText;
    setRawReply(nextText);
    setReport(nextText ? parseReport(nextText) : null);
  }, []);

  const appendReportText = useCallback((chunk) => {
    const nextText = rawReplyRef.current + chunk;
    setReportText(nextText);
  }, [setReportText]);

  const updateStageFromEvent = useCallback((event) => {
    if (event.type === 'run.routed') {
      setStageIdx((idx) => Math.max(idx, 1));
      return;
    }

    if (event.type === 'run.started' && event.runId !== runId) {
      setStageIdx((idx) => Math.max(idx, 2));
      return;
    }

    if (event.type === 'run.tool') {
      setStageIdx((idx) => Math.max(idx, 2));
      return;
    }

    if (event.type === 'run.phase') {
      const phaseName = String(event.phase ?? '').toLowerCase();
      if (phaseName === 'spawning_agent' || phaseName === 'agent_ready') {
        setStageIdx((idx) => Math.max(idx, 0));
        return;
      }
      if (phaseName === 'tool_use') {
        setStageIdx((idx) => Math.max(idx, 3));
        return;
      }
      if (phaseName === 'streaming' || phaseName === 'done') {
        setStageIdx((idx) => Math.max(idx, 4));
      }
    }

    if (event.type === 'run.token') {
      setStageIdx((idx) => Math.max(idx, 4));
    }
  }, [runId]);

  const handleReportPaneScroll = useCallback(() => {
    const pane = reportPaneRef.current;
    if (!pane) return;
    const thresholdPx = 40;
    const distanceFromBottom = pane.scrollHeight - pane.scrollTop - pane.clientHeight;
    setAutoScrollPinned(distanceFromBottom <= thresholdPx);
  }, []);

  const refreshFounderRuns = useCallback(async () => {
    if (!workspaceId) return;

    try {
      const response = await fetch(`/api/founder/workspaces/${encodeURIComponent(workspaceId)}/runs`);
      const data = await response.json().catch(() => ({}));
      if (!response.ok) return;
      setFounderRuns(Array.isArray(data?.runs) ? data.runs : []);
    } catch {}
  }, [workspaceId]);

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

  useEffect(() => {
    if (!requestedPlaybookId || selectedPlaybook?.id === requestedPlaybookId) return;

    let cancelled = false;

    fetch('/api/playbooks')
      .then((response) => response.ok ? response.json() : { playbooks: [] })
      .then((data) => {
        if (cancelled) return;
        const playbook = Array.isArray(data?.playbooks)
          ? data.playbooks.find((item) => item?.id === requestedPlaybookId)
          : null;
        if (!playbook) return;
        setSelectedPlaybook(playbook);
        if (!query.trim() && Array.isArray(playbook.starterQuestions) && playbook.starterQuestions.length > 0) {
          setQuery(playbook.starterQuestions[0]);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [query, requestedPlaybookId, selectedPlaybook]);

  useEffect(() => {
    if (!draftQuery || query.trim()) return;
    setQuery(draftQuery);
  }, [draftQuery, query]);

  useEffect(() => {
    if (!workspaceId) return;

    let cancelled = false;

    async function loadWorkspace() {
      const response = await fetch(`/api/founder/workspaces/${encodeURIComponent(workspaceId)}`);

      if (response.ok) {
        const data = await response.json().catch(() => ({}));
        if (cancelled) return;
        setFounderWorkspace(data.workspace ?? null);
        return;
      }

      if (response.status !== 404 || !clientId) {
        return;
      }

      const createResponse = await fetch('/api/founder/workspaces', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          workspaceId,
          clientId,
          name: `${clientName || 'Client'} Founder Workspace`,
          companyName: clientName || 'Client',
          idea: query.trim(),
          stage: 'validation',
          playbookDefaults: requestedPlaybookId ? { defaultPlaybookId: requestedPlaybookId } : { defaultPlaybookId: 'customer-discovery' },
        }),
      });

      if (!createResponse.ok) return;

      const created = await createResponse.json().catch(() => ({}));
      if (cancelled) return;
      setFounderWorkspace(created.workspace ?? null);
    }

    loadWorkspace().catch(() => {});
    refreshFounderRuns().catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [clientId, clientName, query, refreshFounderRuns, requestedPlaybookId, workspaceId]);

  useEffect(() => {
    if (!founderWorkspace?.playbookDefaults?.defaultPlaybookId || requestedPlaybookId || selectedPlaybook) return;

    let cancelled = false;

    fetch('/api/playbooks')
      .then((response) => response.ok ? response.json() : { playbooks: [] })
      .then((data) => {
        if (cancelled) return;
        const playbook = Array.isArray(data?.playbooks)
          ? data.playbooks.find((item) => item?.id === founderWorkspace.playbookDefaults.defaultPlaybookId)
          : null;
        if (playbook) setSelectedPlaybook(playbook);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [founderWorkspace, requestedPlaybookId, selectedPlaybook]);

  useEffect(() => {
    if (!requestedRunId) return;

    let cancelled = false;

    async function reopenRun() {
      if (workspaceId) {
        const founderRunResponse = await fetch(`/api/founder/workspaces/${encodeURIComponent(workspaceId)}/runs?runId=${encodeURIComponent(requestedRunId)}`);
        if (founderRunResponse.ok) {
          const founderRunData = await founderRunResponse.json().catch(() => ({}));
          const founderRun = founderRunData.run ?? null;
          if (!cancelled && founderRun?.prompt) {
            setQuery(founderRun.prompt);
          }
        }
      }

      const response = await fetch(`/api/runs/${encodeURIComponent(requestedRunId)}`);
      const data = await response.json().catch(() => ({}));
      if (!response.ok || cancelled) return;

      setRunSnapshot(data);
      setRunId(null);
      setPhase('done');
      setReportText(data.output ?? '');
    }

    reopenRun().catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [requestedRunId, setReportText, workspaceId]);

  useEffect(() => {
    if (phase !== 'running' || !autoScrollPinned) return;
    const pane = reportPaneRef.current;
    if (!pane) return;
    pane.scrollTo({ top: pane.scrollHeight, behavior: 'smooth' });
  }, [phase, rawReply, autoScrollPinned]);

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

  async function fetchRunSnapshot(currentRunId) {
    if (!currentRunId) return null;
    const response = await fetch(`/api/runs/${encodeURIComponent(currentRunId)}`);
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
    setRunSnapshot(data);
    return data;
  }

  const persistFounderRun = useCallback(async ({ currentRunId, output, status = 'completed' }) => {
    if (!workspaceId || !currentRunId || !hasMeaningfulReportContent(output)) return;

    const parsed = parseReport(output);
    const response = await fetch(`/api/founder/workspaces/${encodeURIComponent(workspaceId)}/runs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        runId: currentRunId,
        playbookId: selectedPlaybook?.id ?? runSnapshot?.playbookId ?? null,
        prompt: query.trim(),
        status,
        summary: parsed.lead || parsed.findings || output.slice(0, 280),
        citations: sectionLines(parsed.citations),
        nextActions: sectionLines(parsed.nextActions),
      }),
    });
    if (response.ok) {
      refreshFounderRuns().catch(() => {});
    }
  }, [query, refreshFounderRuns, runSnapshot?.playbookId, selectedPlaybook, workspaceId]);

  function saveTextFile(filename, content, mimeType = 'text/plain;charset=utf-8') {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  function handleDownload(format) {
    const slug = query.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'research-report';
    if (format === 'json') {
      saveTextFile(`${slug}.json`, JSON.stringify({ query, report, rawReply }, null, 2), 'application/json;charset=utf-8');
      return;
    }
    saveTextFile(`${slug}.md`, rawReply || report?.raw || '');
  }

  async function handleCheckLatestRun() {
    if (!runId) return;
    try {
      const data = await fetchRunSnapshot(runId);
      if (data.status === 'completed' && hasMeaningfulReportContent(data.output)) {
        stopStageAnimation();
        await persistFounderRun({ currentRunId: runId, output: data.output, status: data.status });
        setRunId(null);
        setReportText(data.output);
        setPhase('done');
        return;
      }

      if (data.status === 'completed' && !hasMeaningfulReportContent(data.output ?? '')) {
        stopStageAnimation();
        setErrMsg(data.error || 'Research finished without producing a report');
        setPhase('error');
        return;
      }

      if (data.status === 'failed' || data.status === 'cancelled') {
        stopStageAnimation();
        setErrMsg(data.error || 'Research failed');
        setPhase('error');
      }
    } catch (err) {
      setErrMsg(err.message || 'Could not load the latest run status');
      setPhase('error');
    }
  }

  useEffect(() => {
    if (phase !== 'running' || !runId) return undefined;

    const source = new EventSource(`/api/runs/${encodeURIComponent(runId)}/events`);
    sseRef.current = source;

    const detachListeners = RUN_EVENT_TYPES.map((eventType) => {
      const listener = (e) => {
        let event;
        try {
          event = JSON.parse(e.data);
        } catch {
          return;
        }

        updateStageFromEvent(event);

        if (event.type === 'run.phase' && event.detail === 'researcher' && event.phase === 'agent_ready') {
          setAgentName('researcher');
        }

        if (event.type === 'run.token') {
          appendReportText(event.content ?? '');
          return;
        }

        if (event.type === 'run.completed' && event.runId === runId) {
          detachListeners.forEach((detach) => detach());
          source.close();
          sseRef.current = null;
          stopStageAnimation();
          const output = typeof event.output === 'string'
            ? event.output
            : String(event.output ?? '');
          if (!hasMeaningfulReportContent(output)) {
            setRunSnapshot((prev) => prev ? { ...prev, status: 'failed', output, error: 'Research finished without producing a report' } : prev);
            setErrMsg('Research finished without producing a report');
            setPhase('error');
            return;
          }
          setRunSnapshot((prev) => prev ? { ...prev, status: 'completed', output } : prev);
          persistFounderRun({ currentRunId: runId, output, status: 'completed' }).catch(() => {});
          setRunId(null);
          setReportText(output);
          setPhase('done');
          return;
        }

        if (event.type === 'run.failed' && event.runId === runId) {
          detachListeners.forEach((detach) => detach());
          source.close();
          sseRef.current = null;
          stopStageAnimation();
          fetchRunSnapshot(runId).catch(() => {});
          setErrMsg(event.error || 'Research failed');
          setPhase('error');
        }
      };

      source.addEventListener(eventType, listener);
      return () => source.removeEventListener(eventType, listener);
    });

    source.onerror = async () => {
      detachListeners.forEach((detach) => detach());
      source.close();
      sseRef.current = null;

      try {
        const r = await fetch(`/api/runs/${encodeURIComponent(runId)}`);
        const data = await r.json().catch(() => ({}));
        if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`);

        if (data.status === 'completed') {
          stopStageAnimation();
          if (!hasMeaningfulReportContent(data.output ?? '')) {
            setRunSnapshot({ ...data, status: 'failed', error: data.error || 'Research finished without producing a report' });
            setErrMsg(data.error || 'Research finished without producing a report');
            setPhase('error');
            return;
          }
          setRunSnapshot(data);
          await persistFounderRun({ currentRunId: runId, output: data.output ?? '', status: data.status });
          setRunId(null);
          setReportText(data.output ?? '');
          setPhase('done');
          return;
        }

        if (data.status === 'failed' || data.status === 'cancelled') {
          stopStageAnimation();
          setRunSnapshot(data);
          setErrMsg(data.error || 'Research failed');
          setPhase('error');
          return;
        }
      } catch (err) {
        stopStageAnimation();
        setErrMsg(err.message || 'Lost connection while tracking research');
        setPhase('error');
        return;
      }

      stopStageAnimation();
      setErrMsg('Lost connection while tracking research');
      setPhase('error');
    };

    return () => {
      detachListeners.forEach((detach) => detach());
      source.close();
      if (sseRef.current === source) sseRef.current = null;
    };
  }, [appendReportText, persistFounderRun, phase, runId, setReportText, updateStageFromEvent]);

  const handlePlaybookChange = useCallback((playbook) => {
    setSelectedPlaybook(playbook);
    if (playbook && !query.trim() && Array.isArray(playbook.starterQuestions) && playbook.starterQuestions.length > 0) {
      setQuery(playbook.starterQuestions[0]);
    }
  }, [query]);

  // ── Run research ──────────────────────────────────────────────────────────
  const handleResearch = useCallback(async () => {
    const q = query.trim();
    if (!q || phase === 'running') return;

    setPhase('running');
    setReport(null);
    setReportText('');
    setErrMsg('');
    setHistory([]);
    setActiveTab('findings');
    setRunId(null);
    setRunSnapshot(null);
    setAutoScrollPinned(true);
    startStageAnimation();

    // Build the full message with optional seed URLs
    const urlLines = seedUrls.trim()
      .split('\n')
      .map(l => l.trim())
      .filter(l => l.startsWith('http'));

    const prompt = urlLines.length > 0
      ? `Research the following question thoroughly:\n\n${q}\n\nSeed sources to start from (fetch these first):\n${urlLines.map(u => '- ' + u).join('\n')}`
      : q;

    const requestContext = {
      ...(urlLines.length > 0 ? { seed_urls: urlLines } : {}),
      ...(clientId ? { client_id: clientId } : {}),
      ...(clientName ? { client_name: clientName } : {}),
      ...(workspaceId ? { workspace_id: workspaceId } : {}),
      ...(founderWorkspace?.companyName ? { company_name: founderWorkspace.companyName } : {}),
      ...(founderWorkspace?.idea ? { idea: founderWorkspace.idea } : {}),
      ...(founderWorkspace?.stage ? { stage: founderWorkspace.stage } : {}),
      ...(founderWorkspace?.playbookDefaults ? { playbook_defaults: founderWorkspace.playbookDefaults } : {}),
    };

    try {
      abortRef.current = new AbortController();

      const r = await fetch('/api/runs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: `[RESEARCH REQUEST] ${prompt}`,
          playbookId: selectedPlaybook?.id ?? null,
          workspaceId,
          clientId,
          context: Object.keys(requestContext).length > 0 ? requestContext : null,
        }),
        signal: abortRef.current.signal,
      });
      const data = await r.json().catch(() => ({}));
      if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`);
      setRunId(data.runId);
      setRunSnapshot({
        runId: data.runId,
        sessionId: data.sessionId,
        status: data.status,
        output: null,
        error: null,
        childRuns: [],
        playbookId: data.playbookId ?? selectedPlaybook?.id ?? null,
        workspaceId: data.workspaceId ?? workspaceId,
      });
      return;
    } catch (err) {
      stopStageAnimation();
      if (err.name === 'AbortError') {
        setPhase('idle');
      } else {
        setErrMsg(err.message || 'Research failed');
        setPhase('error');
      }
    }
  }, [clientId, clientName, founderWorkspace, phase, query, seedUrls, selectedPlaybook, setReportText, workspaceId]);

  useEffect(() => {
    if (!autoStart || autoStartRef.current || phase !== 'idle' || !query.trim()) return;
    if (requestedPlaybookId && !selectedPlaybook) return;

    autoStartRef.current = true;
    handleResearch();
  }, [autoStart, handleResearch, phase, query, requestedPlaybookId, selectedPlaybook]);

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
    sseRef.current?.close();
    clearTimeout(stageTimerRef.current);
    if (runId) {
      fetch(`/api/runs/${encodeURIComponent(runId)}/cancel`, { method: 'POST' }).catch(() => {});
    }
    setPhase('idle');
  };

  const handleReset = () => {
    handleStop();
    setQuery('');
    setSeedUrls('');
    setSelectedPlaybook(null);
    setReport(null);
    setReportText('');
    setHistory([]);
    setStageIdx(-1);
    setErrMsg('');
    setRunId(null);
    setRunSnapshot(null);
    setAutoScrollPinned(true);
  };

  const hasStreamingReport = phase === 'running' && hasMeaningfulReportContent(rawReply);
  const nextActionItems = sectionLines(report?.nextActions);
  const reportTabs = [
    { id: 'findings', label: report?.findings ? '📌 Key Findings' : selectedPlaybook ? '📌 Playbook Output' : '📌 Findings' },
    { id: 'sources', label: '🔗 Sources' + (report?.sourceUrls?.length ? ` (${report.sourceUrls.length})` : '') },
    ...(report?.citations
      ? [{ id: 'citations', label: '📚 Citations' + (report.citationCount ? ` (${report.citationCount})` : '') }]
      : []),
    ...(report?.nextActions
      ? [{ id: 'actions', label: '✅ Next Actions' + (report.nextActionCount ? ` (${report.nextActionCount})` : '') }]
      : []),
    { id: 'open', label: '❓ Open Questions' },
    { id: 'raw', label: '📄 Full Report' },
  ];

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
          <FounderPlaybookChips
            value={selectedPlaybook?.id ?? null}
            onChange={handlePlaybookChange}
            disabled={phase === 'running'}
          />

          {(founderWorkspace || clientName || workspaceId) && (
            <div style={{ marginTop: 12, padding: '10px 12px', borderRadius: 10, border: '1px solid var(--border-light)', background: 'var(--surface2)' }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
                Founder Workspace
              </div>
              <div style={{ marginTop: 6, fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>
                {founderWorkspace?.name || clientName || 'Linked workspace'}
              </div>
              <div style={{ marginTop: 4, fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.5 }}>
                {founderWorkspace?.companyName || clientName || 'This workspace'} · {founderWorkspace?.stage || 'validation'}
              </div>
              <div style={{ marginTop: 4, fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.5 }}>
                Runs from this session will be tagged to {workspaceId || 'the active founder workspace'} for later review.
              </div>
            </div>
          )}

          <label style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
            Research Question
          </label>
          {selectedPlaybook && (
            <div style={{ marginTop: 8, padding: '10px 12px', borderRadius: 10, border: '1px solid var(--border-light)', background: 'var(--surface2)' }}>
              <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text)', display: 'flex', alignItems: 'center', gap: 8 }}>
                <span>{selectedPlaybook.icon}</span>
                <span>{selectedPlaybook.title}</span>
              </div>
              <div style={{ marginTop: 4, fontSize: 12, color: 'var(--text-dim)' }}>
                {selectedPlaybook.description}
              </div>
            </div>
          )}
          <textarea
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder={selectedPlaybook
              ? `Use the ${selectedPlaybook.title} playbook. Describe the company, customer, stage, and constraints.`
              : 'What do you want to research deeply? Be specific — the more context the better.'}
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
            overflow: 'hidden',
          }}>
            <div style={{ padding: '24px 28px 16px', borderBottom: '1px solid var(--border-light)', background: 'var(--surface)', flexShrink: 0 }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16 }}>
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
                    <div style={{ fontSize: 34 }}>
                      {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].icon : '🔬'}
                    </div>
                    <div>
                      <h3 style={{ margin: '0 0 4px', fontSize: 17, color: 'var(--text)' }}>
                        {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].label + '…' : 'Starting…'}
                      </h3>
                      <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.6 }}>
                        {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].desc : 'Initializing research pipeline'}
                      </p>
                    </div>
                  </div>
                  <p style={{ fontSize: 12, color: 'var(--text-dim)', margin: 0 }}>
                    Researching: <em style={{ color: 'var(--text)' }}>{query.length > 110 ? query.slice(0, 110) + '…' : query}</em>
                  </p>
                </div>
                {runId && (
                  <div style={{
                    padding: '10px 12px',
                    background: 'var(--surface2)',
                    border: '1px solid var(--border-light)',
                    borderRadius: 8,
                    fontSize: 12,
                    color: 'var(--text-dim)',
                    lineHeight: 1.5,
                    textAlign: 'right',
                  }}>
                    <div>Live run</div>
                    <div style={{ color: 'var(--accent)', fontFamily: 'var(--font-mono, monospace)' }}>{runId}</div>
                  </div>
                )}
              </div>

              <div style={{ width: '100%', marginTop: 16, background: 'var(--surface2)', borderRadius: 8, height: 6, overflow: 'hidden' }}>
                <div style={{
                  height: '100%',
                  background: 'var(--accent)',
                  borderRadius: 8,
                  width: `${((stageIdx + 1) / STAGES.length) * 100}%`,
                  transition: 'width 0.8s ease',
                }} />
              </div>

              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginTop: 14 }}>
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
                <div style={{ fontSize: 12, color: autoScrollPinned ? 'var(--accent)' : 'var(--text-dim)' }}>
                  {autoScrollPinned ? 'Auto-following live report' : 'Auto-follow paused'}
                </div>
              </div>
            </div>

            <div
              ref={reportPaneRef}
              onScroll={handleReportPaneScroll}
              style={{ flex: 1, overflow: 'auto', padding: '20px 28px 28px' }}
            >
              <ResearchStatusCard
                phase="running"
                title="We’re working on your research"
                message="Stay on this page. We’re gathering sources, comparing them, and turning them into a readable result."
                detail={founderWorkspace ? `This run will be saved to ${founderWorkspace.name}.` : 'The result will appear here as soon as the report text starts streaming.'}
              />
              {hasStreamingReport ? (
                <div style={{ maxWidth: 860 }}>
                  <div style={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 8,
                    padding: '6px 10px',
                    marginBottom: 14,
                    background: 'var(--accent)11',
                    border: '1px solid var(--accent)33',
                    borderRadius: 999,
                    fontSize: 12,
                    color: 'var(--accent)',
                    fontWeight: 600,
                  }}>
                    <span style={{ width: 8, height: 8, borderRadius: '50%', background: 'currentColor' }} />
                    Streaming report draft
                  </div>
                  <MarkdownBlock text={rawReply} />
                </div>
              ) : (
                <div style={{
                  height: '100%',
                  minHeight: 280,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: 'var(--text-dim)',
                  textAlign: 'center',
                  padding: 24,
                }}>
                  <div>
                    <div style={{ fontSize: 36, marginBottom: 10 }}>
                      {stageIdx >= 0 ? STAGES[Math.min(stageIdx, STAGES.length - 1)].icon : '🔬'}
                    </div>
                    <div style={{ fontSize: 14, color: 'var(--text)' }}>Waiting for report text…</div>
                    <div style={{ fontSize: 12, marginTop: 6 }}>Tool events and routing are active; the report pane will fill as tokens arrive.</div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {/* ── Error state ────────────────────────────────────────────────── */}
        {phase === 'error' && (
          <div style={{
            flex: 1,
            overflow: 'auto',
            padding: 32,
          }}>
            <div style={{ maxWidth: 860, margin: '0 auto' }}>
              <ResearchStatusCard
                phase="error"
                title="The research didn’t finish"
                message={errMsg || 'Something went wrong before the result was ready.'}
                detail={runId ? `Technical detail: run ${runId}` : 'Try the same question again or simplify the wording.'}
                actions={[
                  ...(runId ? [{ label: 'Check latest result', onClick: handleCheckLatestRun }] : []),
                  { label: 'Try again', onClick: handleReset, primary: true },
                ]}
              />
            {runSnapshot?.children?.length > 0 && (
              <div style={{
                maxWidth: 720,
                width: '100%',
                padding: '12px 14px',
                background: 'var(--surface2)',
                border: '1px solid var(--border-light)',
                borderRadius: 10,
                textAlign: 'left',
              }}>
                <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text)', marginBottom: 8 }}>Backend run details</div>
                {runSnapshot.children.map((child) => (
                  <div key={child.runId} style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 6, lineHeight: 1.5 }}>
                    <strong style={{ color: 'var(--text)' }}>{child.agent}</strong>: {child.status}
                    {child.error ? ` — ${child.error}` : ''}
                  </div>
                ))}
              </div>
            )}
            </div>
          </div>
        )}

        {/* ── Done: Run Dispatched ────────────────────────────────────── */}
        {phase === 'done' && runId && (
          <div style={{ flex: 1, overflow: 'auto', padding: 32 }}>
            <div style={{ maxWidth: 760, margin: '0 auto' }}>
              <ResearchStatusCard
                phase="dispatched"
                title="Your research started"
                message="The result is still loading. Stay on this page and use the button below if you want to check again now."
                detail={runId ? `Technical detail: run ${runId}` : null}
                actions={[
                  { label: 'Check latest result', onClick: handleCheckLatestRun, primary: true },
                  { label: 'Start over', onClick: handleReset },
                ]}
              />
            </div>
          </div>
        )}

        {/* ── Done: Report ───────────────────────────────────────────────── */}
        {phase === 'done' && report && !runId && (
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
                <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                  Completed deliverable
                </div>
              </div>

              {/* Tab bar */}
              <div style={{ display: 'flex', gap: 2, marginTop: 14 }}>
                {reportTabs.map((t) => (
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
              <ResearchDeliverablePanel
                query={query}
                report={report}
                founderWorkspace={founderWorkspace}
                onRefresh={() => { setQuery(query); handleResearch(); }}
                onCopy={() => navigator.clipboard?.writeText(rawReply)}
                onDownloadMarkdown={() => handleDownload('markdown')}
                onDownloadJson={() => handleDownload('json')}
              />

              <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.35fr) minmax(280px, 0.65fr)', gap: 18, alignItems: 'start' }}>
                <div>
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
                    <MarkdownBlock text={report.findings || rawReply} />
                  )}

                  {activeTab === 'sources' && (
                    <div>
                      {report.sourceUrls?.length > 0 ? (
                        <>
                          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 20 }}>
                            {report.sourceUrls.map((url) => <SourceCard key={url} url={url} />)}
                          </div>
                          <MarkdownBlock text={report.sourcesRaw} />
                        </>
                      ) : (
                        <div style={{ color: 'var(--text-dim)', fontSize: 13 }}>
                          <MarkdownBlock text={report.sourcesRaw || 'No sources extracted.'} />
                        </div>
                      )}
                    </div>
                  )}

                  {activeTab === 'citations' && (
                    <div>
                      {report.citationUrls?.length > 0 && (
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 20 }}>
                          {report.citationUrls.map((url) => <SourceCard key={url} url={url} />)}
                        </div>
                      )}
                      <MarkdownBlock text={report.citations || '_No citations section found in the report._'} />
                    </div>
                  )}

                  {activeTab === 'actions' && (
                    <MarkdownBlock text={report.nextActions || '_No next actions section found in the report._'} />
                  )}

                  {activeTab === 'open' && (
                    <MarkdownBlock text={report.openQuestions || '_No open questions section found in the report._'} />
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

                <div style={{ display: 'grid', gap: 14 }}>
                  <ResearchNextActionsCard
                    actions={nextActionItems}
                    onCopy={nextActionItems.length > 0
                      ? () => navigator.clipboard?.writeText(nextActionItems.join('\n'))
                      : undefined}
                  />
                  <ResearchCitationsPanel urls={report.citationUrls || report.sourceUrls || []} />
                  {workspaceId && founderRuns.length > 0 ? (
                    <div style={{ padding: '16px', border: '1px solid var(--border-light)', borderRadius: 12, background: 'var(--surface2)' }}>
                      <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 8 }}>Recent founder runs</div>
                      <div style={{ display: 'grid', gap: 10 }}>
                        {founderRuns.slice(0, 4).map((run) => (
                          <a
                            key={run.runId}
                            href={`/deep-research?${new URLSearchParams({
                              clientId: clientId || '',
                              clientName: clientName || founderWorkspace?.companyName || 'Client',
                              workspaceId,
                              runId: run.runId,
                              ...(run.playbookId ? { playbookId: run.playbookId } : {}),
                            }).toString()}`}
                            style={{ padding: '10px 12px', borderRadius: 10, border: '1px solid rgba(148,163,184,0.14)', textDecoration: 'none', color: 'inherit' }}
                          >
                            <div style={{ fontSize: 13, fontWeight: 700 }}>{run.playbookId || 'founder research'}</div>
                            <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 4 }}>{run.summary || run.prompt}</div>
                          </a>
                        ))}
                      </div>
                    </div>
                  ) : null}
                </div>
              </div>

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
