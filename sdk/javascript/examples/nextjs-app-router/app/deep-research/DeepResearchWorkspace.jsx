'use client';

import ResearchCitationsPanel from './ResearchCitationsPanel';
import ResearchDeliverablePanel from './ResearchDeliverablePanel';
import ResearchMarkdownBlock from './ResearchMarkdownBlock';
import ResearchNextActionsCard from './ResearchNextActionsCard';
import ResearchStatusCard from './ResearchStatusCard';

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
      background: `${color}22`,
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
  try {
    domain = new URL(url).hostname.replace(/^www\./, '');
  } catch {}

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
        <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
        <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
      </svg>
      {domain}
    </a>
  );
}

function formatByteSize(byteSize) {
  if (!Number.isFinite(byteSize) || byteSize <= 0) return null;
  if (byteSize < 1024) return `${byteSize} B`;
  if (byteSize < 1024 * 1024) return `${(byteSize / 1024).toFixed(1)} KB`;
  return `${(byteSize / (1024 * 1024)).toFixed(1)} MB`;
}

function ArtifactCard({ artifact }) {
  const createdLabel = artifact?.createdAt
    ? new Date(artifact.createdAt).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
    : null;
  const byteSizeLabel = formatByteSize(artifact?.byteSize);
  const href = artifact?.artifactId
    ? `/api/artifacts/${encodeURIComponent(artifact.artifactId)}`
    : artifact?.downloadPath || null;

  return (
    <div style={{ padding: '12px 14px', borderRadius: 10, border: '1px solid rgba(148,163,184,0.14)', background: 'rgba(15,23,42,0.18)' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
        <div style={{ minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)', wordBreak: 'break-word' }}>{artifact.title}</div>
          <div style={{ marginTop: 4, fontSize: 11, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{artifact.kind || 'artifact'}</div>
        </div>
        {href ? (
          <a
            href={href}
            style={{
              flexShrink: 0,
              padding: '6px 10px',
              borderRadius: 8,
              textDecoration: 'none',
              background: 'var(--accent)22',
              border: '1px solid var(--accent)44',
              color: 'var(--accent)',
              fontSize: 12,
              fontWeight: 700,
            }}
          >
            Download
          </a>
        ) : null}
      </div>
      <div style={{ marginTop: 8, display: 'flex', flexWrap: 'wrap', gap: 8, fontSize: 11, color: 'var(--text-dim)' }}>
        {artifact.contentType ? <span>{artifact.contentType}</span> : null}
        {byteSizeLabel ? <span>{byteSizeLabel}</span> : null}
        {createdLabel ? <span>{createdLabel}</span> : null}
      </div>
    </div>
  );
}

export default function DeepResearchWorkspace({
  phase,
  stageIdx,
  stages,
  runId,
  query,
  founderWorkspace,
  autoScrollPinned,
  trackingMode,
  reportPaneRef,
  onReportPaneScroll,
  hasStreamingReport,
  rawReply,
  errMsg,
  runSnapshot,
  onCheckLatestRun,
  onReset,
  report,
  reportTabs,
  activeTab,
  onChangeTab,
  onRefreshResearch,
  onCopyReport,
  onDownloadMarkdown,
  onDownloadJson,
  nextActionItems,
  runArtifacts = [],
  founderRuns,
  clientId,
  clientName,
  workspaceId,
  history,
  followUp,
  onFollowUpChange,
  followLoading,
  agentId,
  onSubmitFollowUp,
  bottomRef,
}) {
  if (phase === 'running') {
    return (
      <div data-cy="deep-research-running" style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '24px 28px 16px', borderBottom: '1px solid var(--border-light)', background: 'var(--surface)', flexShrink: 0 }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16 }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
                <div style={{ fontSize: 34 }}>
                  {stageIdx >= 0 ? stages[Math.min(stageIdx, stages.length - 1)].icon : '🔬'}
                </div>
                <div>
                  <h3 style={{ margin: '0 0 4px', fontSize: 17, color: 'var(--text)' }}>
                    {stageIdx >= 0 ? `${stages[Math.min(stageIdx, stages.length - 1)].label}…` : 'Starting…'}
                  </h3>
                  <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.6 }}>
                    {stageIdx >= 0 ? stages[Math.min(stageIdx, stages.length - 1)].desc : 'Initializing research pipeline'}
                  </p>
                </div>
              </div>
              <p style={{ fontSize: 12, color: 'var(--text-dim)', margin: 0 }}>
                Researching: <em style={{ color: 'var(--text)' }}>{query.length > 110 ? `${query.slice(0, 110)}…` : query}</em>
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
                <div>{trackingMode === 'polling' ? 'Polling run status' : 'Live run'}</div>
                <div style={{ color: 'var(--accent)', fontFamily: 'var(--font-mono, monospace)' }}>{runId}</div>
              </div>
            )}
          </div>

          <div style={{ width: '100%', marginTop: 16, background: 'var(--surface2)', borderRadius: 8, height: 6, overflow: 'hidden' }}>
            <div style={{
              height: '100%',
              background: 'var(--accent)',
              borderRadius: 8,
              width: `${((stageIdx + 1) / stages.length) * 100}%`,
              transition: 'width 0.8s ease',
            }} />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginTop: 14 }}>
            <div style={{ display: 'flex', gap: 8 }}>
              {stages.map((stage, index) => (
                <div
                  key={stage.id}
                  title={stage.label}
                  style={{
                    width: index <= stageIdx ? 10 : 8,
                    height: index <= stageIdx ? 10 : 8,
                    borderRadius: '50%',
                    background: index < stageIdx ? '#22c55e' : index === stageIdx ? 'var(--accent)' : 'var(--border-light)',
                    transition: 'all 0.3s',
                  }}
                />
              ))}
            </div>
            <div style={{ fontSize: 12, color: trackingMode === 'polling' ? '#f59e0b' : autoScrollPinned ? 'var(--accent)' : 'var(--text-dim)' }}>
              {trackingMode === 'polling'
                ? 'Live updates paused. Polling backend status.'
                : autoScrollPinned
                ? 'Auto-following live report'
                : 'Auto-follow paused'}
            </div>
          </div>
        </div>

        <div ref={reportPaneRef} onScroll={onReportPaneScroll} style={{ flex: 1, overflow: 'auto', padding: '20px 28px 28px' }}>
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
              <ResearchMarkdownBlock text={rawReply} />
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
                  {stageIdx >= 0 ? stages[Math.min(stageIdx, stages.length - 1)].icon : '🔬'}
                </div>
                <div style={{ fontSize: 14, color: 'var(--text)' }}>Waiting for report text…</div>
                <div style={{ fontSize: 12, marginTop: 6 }}>
                  {trackingMode === 'polling'
                    ? 'Live stream dropped, but the backend run is still active. Polling will pick up the terminal result.'
                    : 'Tool events and routing are active; the report pane will fill as tokens arrive.'}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  if (phase === 'error') {
    return (
      <div data-cy="deep-research-error" style={{ flex: 1, overflow: 'auto', padding: 32 }}>
        <div style={{ maxWidth: 860, margin: '0 auto' }}>
          <ResearchStatusCard
            phase="error"
            title="The research didn’t finish"
            message={errMsg || 'Something went wrong before the result was ready.'}
            detail={runId ? `Technical detail: run ${runId}` : 'Try the same question again or simplify the wording.'}
            actions={[
              ...(runId ? [{ label: 'Check latest result', onClick: onCheckLatestRun }] : []),
              { label: 'Try again', onClick: onReset, primary: true },
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
    );
  }

  if (phase === 'done' && runId) {
    return (
      <div style={{ flex: 1, overflow: 'auto', padding: 32 }}>
        <div style={{ maxWidth: 760, margin: '0 auto' }}>
          <ResearchStatusCard
            phase="dispatched"
            title="Your research started"
            message="The result is still loading. Stay on this page and use the button below if you want to check again now."
            detail={runId ? `Technical detail: run ${runId}` : null}
            actions={[
              { label: 'Check latest result', onClick: onCheckLatestRun, primary: true },
              { label: 'Start over', onClick: onReset },
            ]}
          />
        </div>
      </div>
    );
  }

  if (phase === 'done' && report && !runId) {
    return (
      <div data-cy="deep-research-report" style={{ flex: 1, overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '20px 28px 16px', borderBottom: '1px solid var(--border-light)', background: 'var(--surface)', flexShrink: 0 }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12 }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                <span style={{ fontSize: 16 }}>📋</span>
                <h3 style={{ margin: 0, fontSize: 15, fontWeight: 700 }}>Research Report</h3>
                {report.confidence && <ConfidenceBadge text={report.confidence} />}
              </div>
              <p style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)', maxWidth: 600 }}>
                {query.length > 100 ? `${query.slice(0, 100)}…` : query}
              </p>
            </div>
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>Completed deliverable</div>
          </div>

          <div style={{ display: 'flex', gap: 2, marginTop: 14 }}>
            {reportTabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => onChangeTab(tab.id)}
                style={{
                  padding: '5px 12px',
                  background: activeTab === tab.id ? 'var(--accent)' : 'transparent',
                  color: activeTab === tab.id ? '#fff' : 'var(--text-dim)',
                  border: activeTab === tab.id ? 'none' : '1px solid var(--border-light)',
                  borderRadius: 6,
                  fontSize: 12,
                  cursor: 'pointer',
                  fontWeight: activeTab === tab.id ? 600 : 400,
                }}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        <div style={{ flex: 1, overflow: 'auto', padding: '20px 28px' }}>
          <ResearchDeliverablePanel
            query={query}
            report={report}
            founderWorkspace={founderWorkspace}
            onRefresh={onRefreshResearch}
            onCopy={onCopyReport}
            onDownloadMarkdown={onDownloadMarkdown}
            onDownloadJson={onDownloadJson}
          />

          <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.35fr) minmax(280px, 0.65fr)', gap: 18, alignItems: 'start' }}>
            <div>
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

              {activeTab === 'findings' && <ResearchMarkdownBlock text={report.findings || rawReply} />}

              {activeTab === 'sources' && (
                <div>
                  {report.sourceUrls?.length > 0 ? (
                    <>
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 20 }}>
                        {report.sourceUrls.map((url) => <SourceCard key={url} url={url} />)}
                      </div>
                      <ResearchMarkdownBlock text={report.sourcesRaw} />
                    </>
                  ) : (
                    <div style={{ color: 'var(--text-dim)', fontSize: 13 }}>
                      <ResearchMarkdownBlock text={report.sourcesRaw || 'No sources extracted.'} />
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
                  <ResearchMarkdownBlock text={report.citations || '_No citations section found in the report._'} />
                </div>
              )}

              {activeTab === 'actions' && <ResearchMarkdownBlock text={report.nextActions || '_No next actions section found in the report._'} />}
              {activeTab === 'open' && <ResearchMarkdownBlock text={report.openQuestions || '_No open questions section found in the report._'} />}
              {activeTab === 'raw' && (
                <pre style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontSize: 13, lineHeight: 1.7, color: 'var(--text)', margin: 0, fontFamily: 'inherit' }}>
                  {rawReply}
                </pre>
              )}
            </div>

            <div style={{ display: 'grid', gap: 14 }}>
              <ResearchNextActionsCard
                actions={nextActionItems}
                onCopy={nextActionItems.length > 0 ? () => navigator.clipboard?.writeText(nextActionItems.join('\n')) : undefined}
              />
              {runArtifacts.length > 0 ? (
                <div style={{ padding: '16px', border: '1px solid var(--border-light)', borderRadius: 12, background: 'var(--surface2)' }}>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 8 }}>Run artifacts</div>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.55, marginBottom: 12 }}>
                    Durable files captured for this run.
                  </div>
                  <div style={{ display: 'grid', gap: 10 }}>
                    {runArtifacts.map((artifact) => (
                      <ArtifactCard key={artifact.artifactId} artifact={artifact} />
                    ))}
                  </div>
                </div>
              ) : null}
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

        <div style={{ borderTop: '1px solid var(--border-light)', background: 'var(--surface)', flexShrink: 0 }}>
          {history.length > 0 && (
            <div style={{ maxHeight: 320, overflow: 'auto', padding: '12px 28px 0' }}>
              {history.map((item, index) => (
                <div key={`${item.q}-${index}`} style={{ marginBottom: 14 }}>
                  <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)', marginBottom: 4 }}>Q: {item.q}</div>
                  {item.loading ? (
                    <div style={{ fontSize: 12, color: 'var(--text-dim)', fontStyle: 'italic' }}>Thinking…</div>
                  ) : (
                    <div style={{ padding: '10px 12px', background: 'var(--surface2)', borderRadius: 8, fontSize: 13, lineHeight: 1.65, whiteSpace: 'pre-wrap' }}>
                      {item.a}
                    </div>
                  )}
                </div>
              ))}
              <div ref={bottomRef} />
            </div>
          )}

          <div style={{ padding: '12px 28px 16px', display: 'flex', gap: 8, alignItems: 'flex-end' }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 4 }}>💬 Ask a follow-up question about this research</div>
              <textarea
                data-cy="deep-research-followup"
                value={followUp}
                onChange={(event) => onFollowUpChange(event.target.value)}
                placeholder={agentId ? 'Dig deeper, request a comparison, ask for more detail on a specific finding…' : 'Researcher agent not available — follow-up requires a researcher agent'}
                disabled={!agentId || followLoading}
                rows={2}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault();
                    onSubmitFollowUp();
                  }
                }}
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
              data-cy="deep-research-followup-btn"
              onClick={onSubmitFollowUp}
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
    );
  }

  return null;
}