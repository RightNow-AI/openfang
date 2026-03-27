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
import FounderPlaybookChips from './FounderPlaybookChips';
import DeepResearchWorkspace from './DeepResearchWorkspace';

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
const RUN_STATUS_POLL_MS = 8000;
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

function normalizeArtifactRecord(artifact) {
  if (!artifact || typeof artifact !== 'object') return null;

  const artifactId = String(artifact.artifactId ?? artifact.artifact_id ?? '').trim();
  const scopeId = String(artifact.scopeId ?? artifact.scope_id ?? '').trim();
  if (!artifactId || !scopeId) return null;

  return {
    artifactId,
    scopeKind: String(artifact.scopeKind ?? artifact.scope_kind ?? 'run').trim() || 'run',
    scopeId,
    kind: String(artifact.kind ?? 'binary').trim() || 'binary',
    title: String(artifact.title ?? 'artifact').trim() || 'artifact',
    contentType: String(artifact.contentType ?? artifact.content_type ?? 'application/octet-stream').trim() || 'application/octet-stream',
    byteSize: Number.isFinite(artifact.byteSize) ? artifact.byteSize : (Number.isFinite(artifact.byte_size) ? artifact.byte_size : null),
    createdAt: String(artifact.createdAt ?? artifact.created_at ?? '').trim(),
    downloadPath: String(artifact.downloadPath ?? artifact.download_path ?? '').trim(),
    metadataPath: String(artifact.metadataPath ?? artifact.metadata_path ?? '').trim(),
    workspaceId: artifact.workspaceId ? String(artifact.workspaceId).trim() : null,
    runId: artifact.runId ? String(artifact.runId).trim() : null,
  };
}

function mergeArtifactRecords(...artifactSets) {
  const merged = new Map();

  for (const artifactSet of artifactSets) {
    if (!Array.isArray(artifactSet)) continue;
    for (const artifact of artifactSet) {
      const normalized = normalizeArtifactRecord(artifact);
      if (!normalized) continue;
      merged.set(normalized.artifactId, normalized);
    }
  }

  return [...merged.values()].sort((left, right) => {
    const leftTime = Date.parse(left.createdAt || 0) || 0;
    const rightTime = Date.parse(right.createdAt || 0) || 0;
    return rightTime - leftTime;
  });
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
  const [trackingMode, setTrackingMode] = useState('stream');
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
    fetch('/api/openfang-proxy/api/agents', { credentials: 'same-origin' })
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

      const resolvedWorkspaceId = String(data.workspaceId ?? data.workspace_id ?? workspaceId ?? '').trim() || null;
      const durableArtifacts = resolvedWorkspaceId
        ? await fetchRunArtifacts(requestedRunId, resolvedWorkspaceId).catch(() => [])
        : [];

      setRunSnapshot({
        ...data,
        workspaceId: resolvedWorkspaceId,
        artifacts: mergeArtifactRecords(data.artifacts, durableArtifacts),
      });
      setRunId(null);
      setPhase('done');
      setReportText(data.output ?? '');
    }

    reopenRun().catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [fetchRunArtifacts, requestedRunId, setReportText, workspaceId]);

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

  const fetchRunArtifacts = useCallback(async (currentRunId, currentWorkspaceId) => {
    if (!currentRunId || !currentWorkspaceId) return [];

    const response = await fetch(`/api/workspaces/${encodeURIComponent(currentWorkspaceId)}/runs/${encodeURIComponent(currentRunId)}/artifacts`);
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
    return mergeArtifactRecords(data.artifacts);
  }, []);

  const fetchRunSnapshot = useCallback(async (currentRunId) => {
    if (!currentRunId) return null;
    const response = await fetch(`/api/runs/${encodeURIComponent(currentRunId)}`);
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
    const resolvedWorkspaceId = String(data.workspaceId ?? data.workspace_id ?? workspaceId ?? '').trim() || null;
    const durableArtifacts = resolvedWorkspaceId
      ? await fetchRunArtifacts(currentRunId, resolvedWorkspaceId).catch(() => [])
      : [];
    const nextSnapshot = {
      ...data,
      workspaceId: resolvedWorkspaceId,
      artifacts: mergeArtifactRecords(data.artifacts, durableArtifacts),
    };
    setRunSnapshot(nextSnapshot);
    return nextSnapshot;
  }, [fetchRunArtifacts, workspaceId]);

  const persistFounderRun = useCallback(async ({ currentRunId, output, status = 'completed' }) => {
    if (!workspaceId || !currentRunId || !hasMeaningfulReportContent(output)) return;

    const parsed = parseReport(output);
    const slug = query.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'research-report';
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
        artifacts: [
          {
            title: `${slug}.md`,
            content: output,
            contentType: 'text/markdown',
            kind: 'markdown',
          },
          {
            title: `${slug}.json`,
            content: JSON.stringify({ query, report: parsed, rawReply: output }, null, 2),
            contentType: 'application/json',
            kind: 'json',
          },
        ],
      }),
    });
    if (response.ok) {
      const data = await response.json().catch(() => ({}));
      if (data?.run) {
        const durableArtifacts = await fetchRunArtifacts(currentRunId, workspaceId).catch(() => []);
        setRunSnapshot((previous) => previous ? {
          ...previous,
          artifacts: mergeArtifactRecords(previous.artifacts, data.run.artifacts, durableArtifacts),
        } : previous);
      }
      refreshFounderRuns().catch(() => {});
    }
  }, [fetchRunArtifacts, query, refreshFounderRuns, runSnapshot?.playbookId, selectedPlaybook, workspaceId]);

  const resolveRunState = useCallback(async (data, currentRunId) => {
    if (!data || !currentRunId) return false;

    if (data.status === 'completed' && hasMeaningfulReportContent(data.output ?? '')) {
      stopStageAnimation();
      await persistFounderRun({ currentRunId, output: data.output ?? '', status: data.status });
      setRunId((previousRunId) => previousRunId === currentRunId ? null : previousRunId);
      setReportText(data.output ?? '');
      setTrackingMode('stream');
      setPhase('done');
      return true;
    }

    if (data.status === 'completed' && !hasMeaningfulReportContent(data.output ?? '')) {
      stopStageAnimation();
      setErrMsg(data.error || 'Research finished without producing a report');
      setTrackingMode('stream');
      setPhase('error');
      return true;
    }

    if (data.status === 'failed' || data.status === 'cancelled') {
      stopStageAnimation();
      setErrMsg(data.error || 'Research failed');
      setTrackingMode('stream');
      setPhase('error');
      return true;
    }

    return false;
  }, [persistFounderRun, setReportText]);

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

  function findDownloadArtifact(format) {
    const artifacts = Array.isArray(runSnapshot?.artifacts) ? runSnapshot.artifacts : [];
    return artifacts.find((artifact) => {
      const title = String(artifact?.title ?? '').toLowerCase();
      const kind = String(artifact?.kind ?? '').toLowerCase();
      if (format === 'json') return kind === 'json' || title.endsWith('.json');
      return kind === 'markdown' || title.endsWith('.md');
    }) ?? null;
  }

  async function handleDownload(format) {
    const slug = query.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'research-report';
    const artifact = findDownloadArtifact(format);
    if (artifact?.artifactId) {
      window.location.assign(`/api/artifacts/${encodeURIComponent(artifact.artifactId)}`);
      return;
    }

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
      await resolveRunState(data, runId);
    } catch (err) {
      setErrMsg(err.message || 'Could not load the latest run status');
      setPhase('error');
    }
  }

  useEffect(() => {
    if (phase !== 'running' || !runId) return undefined;

    const intervalId = setInterval(() => {
      fetchRunSnapshot(runId)
        .then((data) => resolveRunState(data, runId))
        .catch(() => {});
    }, RUN_STATUS_POLL_MS);

    return () => clearInterval(intervalId);
  }, [fetchRunSnapshot, phase, resolveRunState, runId]);

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
            setTrackingMode('stream');
            setPhase('error');
            return;
          }
          setRunSnapshot((prev) => prev ? { ...prev, status: 'completed', output } : prev);
          persistFounderRun({ currentRunId: runId, output, status: 'completed' }).catch(() => {});
          setRunId(null);
          setReportText(output);
          setTrackingMode('stream');
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
          setTrackingMode('stream');
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

        if (await resolveRunState(data, runId)) {
          return;
        }

        if (data.status === 'queued' || data.status === 'running' || data.status === 'pending') {
          setTrackingMode('polling');
          return;
        }
      } catch (err) {
        stopStageAnimation();
        setErrMsg(err.message || 'Lost connection while tracking research');
        setPhase('error');
        return;
      }

      setTrackingMode('polling');
    };

    return () => {
      detachListeners.forEach((detach) => detach());
      source.close();
      if (sseRef.current === source) sseRef.current = null;
    };
  }, [appendReportText, fetchRunSnapshot, persistFounderRun, phase, resolveRunState, runId, setReportText, updateStageFromEvent]);

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
    setTrackingMode('stream');
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
        artifacts: [],
        childRuns: [],
        playbookId: data.playbookId ?? selectedPlaybook?.id ?? null,
        workspaceId: data.workspaceId ?? workspaceId,
      });
      setTrackingMode('stream');
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
    setTrackingMode('stream');
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
    <div data-cy="deep-research-page" style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>

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
            data-cy="deep-research-query"
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
              data-cy="deep-research-start-btn"
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
              data-cy="deep-research-stop-btn"
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
        {phase !== 'idle' && (
          <DeepResearchWorkspace
            phase={phase}
            stageIdx={stageIdx}
            stages={STAGES}
            runId={runId}
            query={query}
            founderWorkspace={founderWorkspace}
            autoScrollPinned={autoScrollPinned}
            trackingMode={trackingMode}
            reportPaneRef={reportPaneRef}
            onReportPaneScroll={handleReportPaneScroll}
            hasStreamingReport={hasStreamingReport}
            rawReply={rawReply}
            errMsg={errMsg}
            runSnapshot={runSnapshot}
            onCheckLatestRun={handleCheckLatestRun}
            onReset={handleReset}
            report={report}
            reportTabs={reportTabs}
            activeTab={activeTab}
            onChangeTab={setActiveTab}
            onRefreshResearch={handleResearch}
            onCopyReport={() => navigator.clipboard?.writeText(rawReply)}
            onDownloadMarkdown={() => handleDownload('markdown')}
            onDownloadJson={() => handleDownload('json')}
            nextActionItems={nextActionItems}
            runArtifacts={Array.isArray(runSnapshot?.artifacts) ? runSnapshot.artifacts : []}
            founderRuns={founderRuns}
            clientId={clientId}
            clientName={clientName}
            workspaceId={workspaceId}
            history={history}
            followUp={followUp}
            onFollowUpChange={setFollowUp}
            followLoading={followLoading}
            agentId={agentId}
            onSubmitFollowUp={handleFollowUp}
            bottomRef={bottomRef}
          />
        )}
      </div>

      {/* ── CSS for spin animation ─────────────────────────────────────── */}
      <style>{`
        @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}
