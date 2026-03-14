'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';

// ── Step definitions ────────────────────────────────────────────────────────
const STEPS = [
  { id: 'welcome',       title: 'Welcome',        icon: '👋' },
  { id: 'what-it-does',  title: 'What this does', icon: '🤖' },
  { id: 'checklist',     title: 'Getting ready',  icon: '📋' },
  { id: 'check-status',  title: 'Checking setup', icon: '🔌' },
  { id: 'api-key',       title: 'Connect AI',     icon: '🔑' },
  { id: 'first-message', title: 'First message',  icon: '💬' },
  { id: 'done',          title: "You're all set", icon: '🎉' },
];

// ── Small reusable UI pieces ────────────────────────────────────────────────

function InfoCard({ children, style }) {
  return (
    <div style={{
      background: 'var(--surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius)',
      padding: '18px 22px',
      ...style,
    }}>
      {children}
    </div>
  );
}

function Callout({ type = 'info', icon, children }) {
  const map = {
    info:    { bg: 'var(--accent-subtle)',  border: 'var(--accent-light)' },
    success: { bg: 'var(--success-subtle)', border: 'var(--success)' },
    warning: { bg: 'var(--warning-subtle)', border: 'var(--warning)' },
    error:   { bg: 'var(--error-subtle)',   border: 'var(--error)' },
  };
  const c = map[type] ?? map.info;
  return (
    <div style={{
      background: c.bg,
      border: `1px solid ${c.border}`,
      borderRadius: 'var(--radius-sm)',
      padding: '12px 16px',
      marginTop: 12,
      display: 'flex',
      gap: 10,
      alignItems: 'flex-start',
    }}>
      {icon && <span style={{ fontSize: 16, flexShrink: 0, marginTop: 1 }}>{icon}</span>}
      <div style={{ fontSize: 13, lineHeight: 1.7, color: 'var(--text)' }}>{children}</div>
    </div>
  );
}

function StatusRow({ label, status, detail }) {
  const icon =
    status === 'ok'       ? '✅' :
    status === 'checking' ? '⏳' :
    status === 'error'    ? '❌' : '⚠️';
  return (
    <div style={{
      display: 'flex', alignItems: 'flex-start', gap: 10,
      padding: '10px 0', borderBottom: '1px solid var(--border-subtle)',
    }}>
      <span style={{ fontSize: 16, flexShrink: 0, marginTop: 2 }}>{icon}</span>
      <div>
        <div style={{ fontWeight: 600, fontSize: 13 }}>{label}</div>
        {detail && <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2, lineHeight: 1.5 }}>{detail}</div>}
      </div>
    </div>
  );
}

function Expandable({ title, children }) {
  const [open, setOpen] = useState(false);
  return (
    <div style={{
      border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
      overflow: 'hidden', marginTop: 12,
    }}>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          width: '100%', textAlign: 'left', padding: '10px 14px',
          background: 'var(--surface2)', border: 'none', cursor: 'pointer',
          display: 'flex', justifyContent: 'space-between', alignItems: 'center',
          fontSize: 13, fontWeight: 600, color: 'var(--text-dim)',
        }}
      >
        <span>{title}</span>
        <span style={{ transform: open ? 'rotate(90deg)' : '', transition: 'transform 0.2s', fontSize: 16 }}>›</span>
      </button>
      {open && (
        <div style={{
          padding: '14px 16px', background: 'var(--surface)',
          fontSize: 13, lineHeight: 1.75, color: 'var(--text)',
        }}>
          {children}
        </div>
      )}
    </div>
  );
}

function CopyButton({ text }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try { await navigator.clipboard.writeText(text); } catch (_) {}
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }
  return (
    <button
      onClick={copy}
      style={{
        padding: '3px 10px', fontSize: 11, borderRadius: 5, cursor: 'pointer',
        background: copied ? 'var(--success-subtle)' : 'var(--surface3)',
        border: `1px solid ${copied ? 'var(--success)' : 'var(--border)'}`,
        color: copied ? 'var(--success)' : 'var(--text-dim)',
        verticalAlign: 'middle', marginLeft: 8,
      }}
    >
      {copied ? '✓ Copied' : 'Copy'}
    </button>
  );
}

function CodeBlock({ children }) {
  return (
    <div style={{
      background: 'var(--surface2)',
      border: '1px solid var(--border)',
      borderRadius: 6,
      padding: '10px 14px',
      fontFamily: 'var(--font-mono)',
      fontSize: 12,
      color: 'var(--text)',
      margin: '8px 0',
      whiteSpace: 'pre-wrap',
      wordBreak: 'break-all',
    }}>
      {children}
    </div>
  );
}

function NavBtns({ onBack, onNext, nextLabel = 'Continue →', nextDisabled = false, extra }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 28 }}>
      <div>
        {onBack && (
          <button className="btn btn-ghost" onClick={onBack}>← Back</button>
        )}
      </div>
      <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
        {extra}
        {onNext && (
          <button className="btn btn-primary" onClick={onNext} disabled={nextDisabled}>
            {nextLabel}
          </button>
        )}
      </div>
    </div>
  );
}

// ── Main wizard component ────────────────────────────────────────────────────
export default function OnboardingPage() {
  const router = useRouter();
  const [stepIdx, setStepIdx] = useState(0);

  // Status check state
  const [daemonStatus, setDaemonStatus] = useState('idle');
  const [llmStatus,    setLlmStatus]    = useState('idle');
  const [agentCount,   setAgentCount]   = useState(0);
  const [statusError,  setStatusError]  = useState(null);
  const [statusChecked, setStatusChecked] = useState(false);

  // First message state
  const [firstMessage, setFirstMessage] = useState('');
  const [firstReply,   setFirstReply]   = useState(null);
  const [sending,      setSending]      = useState(false);
  const [sendError,    setSendError]    = useState(null);
  const [runId,        setRunId]        = useState(null);

  const textareaRef = useRef(null);

  const step = STEPS[stepIdx];
  const visibleSteps = STEPS.slice(1); // exclude welcome from progress bar
  const progressIdx  = Math.max(0, stepIdx - 1);
  const progressPct  = Math.round((progressIdx / (visibleSteps.length - 1)) * 100);

  function goTo(id) { setStepIdx(STEPS.findIndex(s => s.id === id)); }
  function next()   { setStepIdx(i => Math.min(i + 1, STEPS.length - 1)); }
  function prev()   { setStepIdx(i => Math.max(i - 1, 0)); }

  // ── Status check ────────────────────────────────────────────────────────
  const checkStatus = useCallback(async () => {
    setDaemonStatus('checking');
    setLlmStatus('checking');
    setStatusError(null);
    try {
      const r = await fetch('/api/onboarding/status', {
        signal: AbortSignal.timeout(18000),
      });
      const data = await r.json();
      setDaemonStatus(data.daemon);
      setLlmStatus(data.llm);
      setAgentCount(data.agentCount ?? 0);
      if (data.error) setStatusError(data.error);
    } catch (err) {
      setDaemonStatus('error');
      setLlmStatus('error');
      setStatusError(err.message);
    }
    setStatusChecked(true);
  }, []);

  // Auto-run check when we land on the status step
  useEffect(() => {
    if (step?.id === 'check-status' && !statusChecked) {
      checkStatus();
    }
  }, [step, statusChecked, checkStatus]);

  // ── Send first message ───────────────────────────────────────────────────
  const sendFirstMessage = useCallback(async () => {
    if (!firstMessage.trim() || sending) return;
    setSending(true);
    setSendError(null);
    setFirstReply(null);
    try {
      const r = await fetch('/api/runs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: firstMessage.trim(), sessionId: 'onboarding' }),
      });
      if (!r.ok) throw new Error(`Server returned ${r.status}`);
      const data = await r.json();
      if (!data.runId) throw new Error(data.error || 'No run ID returned.');
      setRunId(data.runId);

      // Poll for result
      let attempts = 0;
      while (attempts < 45) {
        await new Promise(res => setTimeout(res, 1000));
        attempts++;
        const pr = await fetch(`/api/runs/${data.runId}`);
        const run = await pr.json();
        if (run.status === 'completed') {
          const output = run.output
            ?? run.children?.find(c => c.output)?.output
            ?? 'Done!';
          setFirstReply(output);
          break;
        }
        if (run.status === 'failed') {
          throw new Error(run.error || 'The AI returned an error. It may not be connected yet.');
        }
      }
      if (attempts >= 45) {
        throw new Error('The AI is taking too long to reply. The API key may not be set up yet — ask the person who installed this app for help.');
      }
    } catch (err) {
      setSendError(err.message);
    }
    setSending(false);
  }, [firstMessage, sending]);

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div style={{ maxWidth: 700, margin: '0 auto' }} data-cy="onboarding-wizard">

      {/* Page header */}
      <div className="page-header">
        <h1>{step?.id === 'welcome' ? 'Setup Guide' : `${step?.icon} ${step?.title}`}</h1>
        {stepIdx > 0 && stepIdx < STEPS.length - 1 && (
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => router.push('/chat')}
            title="Skip setup and go straight to chat"
          >
            Skip for now →
          </button>
        )}
      </div>

      {/* Progress bar (hidden on welcome and done) */}
      {stepIdx > 0 && stepIdx < STEPS.length - 1 && (
        <div style={{ marginBottom: 24 }} aria-label="Setup progress">
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
            <span style={{ fontSize: 12, color: 'var(--text-dim)' }}>
              Step {stepIdx} of {visibleSteps.length - 1}
            </span>
            <span style={{ fontSize: 12, color: 'var(--text-dim)' }}>{progressPct}% complete</span>
          </div>
          <div style={{ height: 5, background: 'var(--surface3)', borderRadius: 3 }}>
            <div style={{
              height: '100%', width: `${progressPct}%`,
              background: 'var(--accent)', borderRadius: 3,
              transition: 'width 0.4s ease',
            }} />
          </div>
          {/* Mini step dots */}
          <div style={{ display: 'flex', gap: 5, marginTop: 6 }}>
            {visibleSteps.map((s, i) => (
              <div key={s.id} style={{
                flex: 1, height: 3, borderRadius: 2,
                background: i < progressIdx ? 'var(--accent)' :
                            i === progressIdx ? 'var(--accent-light)' : 'var(--surface3)',
                transition: 'background 0.3s',
              }} title={s.title} />
            ))}
          </div>
        </div>
      )}

      {/* ── Card content ── */}
      <div className="card" style={{ padding: '32px 36px' }}>

        {/* ════════════════════════════════════════════════════════════════
            STEP 0 — Welcome
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'welcome' && (
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: 72, marginBottom: 20, lineHeight: 1 }}>👋</div>
            <h2 style={{ fontSize: 30, fontWeight: 800, marginBottom: 10, letterSpacing: '-0.5px' }}>
              Welcome to OpenFang
            </h2>
            <p style={{ fontSize: 16, color: 'var(--text-dim)', maxWidth: 500, margin: '0 auto 12px', lineHeight: 1.75 }}>
              Your personal AI assistant — ready to help you write, research, plan, and more.
            </p>
            <p style={{ fontSize: 14, color: 'var(--text-muted)', marginBottom: 36, lineHeight: 1.65 }}>
              This guide will walk you through setup one step at a time.<br />
              It takes about <strong style={{ color: 'var(--text)' }}>3 to 5 minutes</strong> and you do not need any technical knowledge.
            </p>
            <div style={{ display: 'flex', gap: 14, justifyContent: 'center', flexWrap: 'wrap', marginBottom: 24 }}>
              <button
                className="btn btn-primary"
                style={{ fontSize: 16, padding: '13px 36px', borderRadius: 10 }}
                onClick={next}
                data-cy="onboarding-start"
              >
                Get started →
              </button>
              <button
                className="btn btn-ghost"
                style={{ fontSize: 13 }}
                onClick={() => goTo('check-status')}
              >
                Already set up — just check my connection
              </button>
            </div>
            <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>
              You can return to this guide any time from the sidebar under <strong>System → Setup</strong>.
            </p>
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 1 — What this app does
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'what-it-does' && (
          <div>
            <p style={{ color: 'var(--text-dim)', marginBottom: 22, fontSize: 15, lineHeight: 1.75 }}>
              OpenFang gives you a team of AI assistants that can help you with your daily life.
              Here is what they can do.
            </p>

            <div style={{ display: 'grid', gap: 10, marginBottom: 24 }}>
              {[
                { icon: '💬', title: 'Chat and answer questions',
                  desc: 'Ask anything — cooking tips, history, health advice, how to write an email. Get a clear, plain-English answer, instantly.' },
                { icon: '✍️', title: 'Write for you',
                  desc: "Tell your assistant what you need to say. It will write letters, emails, messages, or reports — then you can edit as you like." },
                { icon: '🔍', title: 'Research and explain',
                  desc: "Give it a topic and it will find and summarise the key information for you. No more hours of Googling." },
                { icon: '📅', title: 'Plan and organise',
                  desc: 'Create lists, plan your week, break big tasks into small steps, set priorities.' },
                { icon: '🚀', title: 'Run automated tasks',
                  desc: 'Set up agents that check things, summarise emails, or follow up on tasks — automatically, while you get on with your day.' },
              ].map(item => (
                <div key={item.icon} style={{
                  display: 'flex', gap: 14, padding: '14px 16px',
                  background: 'var(--surface2)', borderRadius: 'var(--radius-sm)',
                }}>
                  <span style={{ fontSize: 26, flexShrink: 0, marginTop: 1 }}>{item.icon}</span>
                  <div>
                    <div style={{ fontWeight: 700, marginBottom: 3, fontSize: 14 }}>{item.title}</div>
                    <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.65 }}>{item.desc}</div>
                  </div>
                </div>
              ))}
            </div>

            <Callout type="info" icon="🔒">
              <strong>Your privacy matters.</strong> Everything you type stays on your own computer.
              Your conversations are not shared with anyone else.
            </Callout>

            <NavBtns onBack={prev} onNext={next} nextLabel="That sounds great →" />
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 2 — Checklist
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'checklist' && (
          <div>
            <p style={{ color: 'var(--text-dim)', marginBottom: 22, fontSize: 15, lineHeight: 1.75 }}>
              Here is everything you will need. Most of it you probably already have.
            </p>

            {[
              {
                icon: '🌐', done: true,
                title: 'A web browser',
                desc: 'You are already using one! Chrome, Safari, Firefox, and Edge all work perfectly.',
              },
              {
                icon: '📧', done: true,
                title: 'An email address',
                desc: 'You will need this to create a free account with the AI service (called OpenRouter).',
              },
              {
                icon: '⏱️', done: true,
                title: 'About 5 minutes',
                desc: "That's all the time it takes. If something interrupts you, just come back — nothing gets erased.",
              },
              {
                icon: '📝', done: false,
                title: 'A safe place to copy and paste',
                desc: 'We will generate a special code (called an API key). Keep it safe like a password — do not share it with anyone.',
              },
            ].map(item => (
              <div key={item.icon} style={{
                display: 'flex', gap: 14, padding: '14px 16px', marginBottom: 8,
                background: 'var(--surface2)', borderRadius: 'var(--radius-sm)',
                alignItems: 'flex-start',
              }}>
                <span style={{ fontSize: 24, flexShrink: 0 }}>{item.icon}</span>
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 700, marginBottom: 2, fontSize: 14 }}>{item.title}</div>
                  <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.65 }}>{item.desc}</div>
                </div>
                {item.done && (
                  <span style={{ color: 'var(--success)', fontWeight: 800, fontSize: 18, marginTop: 2 }}>✓</span>
                )}
              </div>
            ))}

            <Callout type="warning" icon="🔑">
              <strong>What is an API key?</strong><br />
              Think of it like a special password that lets this app talk to an AI service on the internet.
              You create it once on a website, copy it, and paste it into this app — then you are done.
              It's completely free to get started.
            </Callout>

            <NavBtns onBack={prev} onNext={next} nextLabel="I'm ready →" />
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 3 — Check status
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'check-status' && (
          <div>
            <p style={{ color: 'var(--text-dim)', marginBottom: 20, fontSize: 15, lineHeight: 1.75 }}>
              We are running a quick check to see if everything is connected.
              This takes just a few seconds.
            </p>

            <InfoCard style={{ marginBottom: 16 }}>
              <StatusRow
                label="App backend"
                status={daemonStatus === 'idle' ? 'checking' : daemonStatus}
                detail={
                  daemonStatus === 'idle'  ? 'Waiting to check…' :
                  daemonStatus === 'checking' ? 'Looking for the app backend…' :
                  daemonStatus === 'ok'    ? `Connected! ${agentCount > 0 ? `${agentCount} AI agent${agentCount === 1 ? '' : 's'} ready.` : 'Backend is running.'}` :
                  'Could not find the app backend. See help below.'
                }
              />
              <StatusRow
                label="AI connection"
                status={llmStatus === 'idle' ? 'checking' : llmStatus === 'unconfigured' ? 'warning' : llmStatus}
                detail={
                  llmStatus === 'idle'         ? 'Waiting to check…' :
                  llmStatus === 'checking'     ? 'Testing if AI is responding (may take up to 10 seconds)…' :
                  llmStatus === 'ok'           ? 'AI is responding correctly. All systems go!' :
                  llmStatus === 'unconfigured' ? 'AI service not connected yet. We will help you set this up.' :
                  'AI connection is not working. See help below.'
                }
              />
            </InfoCard>

            {/* ── All good → celebrate ── */}
            {daemonStatus === 'ok' && llmStatus === 'ok' && (
              <Callout type="success" icon="🎉">
                <strong>Everything is working perfectly!</strong><br />
                Your AI assistant is ready. Let's send your very first message.
              </Callout>
            )}

            {/* ── Daemon error help ── */}
            {daemonStatus === 'error' && (
              <>
                <Callout type="error" icon="❌">
                  <strong>The app backend is not running.</strong><br />
                  This app needs a background service running on your computer.
                  If someone installed this for you, ask them to start it.
                  If you installed it yourself, see the technical help below.
                </Callout>
                <Expandable title="🛠️ For the person helping — How to start the backend">
                  <p style={{ marginTop: 0 }}>Run this command from the project folder:</p>
                  <CodeBlock>.\target\release\openfang.exe start</CodeBlock>
                  <p>
                    The backend starts on port 50051.
                    Make sure you have an API key set <em>before</em> starting (see step 5 of this wizard).
                  </p>
                  <p>
                    Check that the config file has a model configured in{' '}
                    <code style={{ background: 'var(--surface3)', padding: '1px 5px', borderRadius: 3 }}>~/.openfang/config.toml</code>.
                  </p>
                  {statusError && (
                    <p style={{ color: 'var(--error)', fontSize: 12 }}>Technical detail: {statusError}</p>
                  )}
                </Expandable>
              </>
            )}

            {/* ── LLM not configured ── */}
            {daemonStatus === 'ok' && (llmStatus === 'unconfigured' || llmStatus === 'error') && (
              <Callout type="warning" icon="⚠️">
                <strong>The AI service is not connected yet.</strong><br />
                The app is running, but it needs an AI key to generate responses.
                The next step will walk you through getting a free one — it only takes a minute.
              </Callout>
            )}

            <NavBtns
              onBack={stepIdx > 0 ? prev : undefined}
              onNext={() => {
                if (daemonStatus === 'ok' && llmStatus === 'ok') {
                  goTo('first-message');
                } else {
                  next();
                }
              }}
              nextLabel={
                daemonStatus === 'checking' || llmStatus === 'checking'
                  ? '⏳ Checking…'
                  : (daemonStatus === 'ok' && llmStatus === 'ok')
                    ? 'Send my first message →'
                    : 'Continue to setup →'
              }
              nextDisabled={daemonStatus === 'checking' || llmStatus === 'checking'}
              extra={
                statusChecked && (
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => { setStatusChecked(false); checkStatus(); }}
                    disabled={daemonStatus === 'checking' || llmStatus === 'checking'}
                  >
                    🔄 Check again
                  </button>
                )
              }
            />
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 4 — API key
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'api-key' && (
          <div>
            <p style={{ color: 'var(--text-dim)', marginBottom: 20, fontSize: 15, lineHeight: 1.75 }}>
              To make the AI work, this app needs to connect to an AI service.
              We recommend <strong style={{ color: 'var(--text)' }}>OpenRouter</strong>
              — it is free to start and works with dozens of different AI models.
            </p>

            <Callout type="info" icon="💡">
              <strong>What is OpenRouter?</strong><br />
              It is a website that gives you access to powerful AI assistants. You create a free account,
              and they give you a secret code — called an <strong>API key</strong> — that lets this app
              use their AI. You only pay for what you use, and getting started is completely free.
            </Callout>

            {/* Step-by-step instructions */}
            <div style={{ marginTop: 26 }}>
              <h3 style={{ fontWeight: 700, marginBottom: 16, fontSize: 15 }}>
                Follow these steps to get your free API key
              </h3>

              {[
                {
                  n: 1,
                  title: 'Go to the OpenRouter website',
                  body: (
                    <span>
                      Open a new browser tab and visit{' '}
                      <a
                        href="https://openrouter.ai"
                        target="_blank"
                        rel="noreferrer"
                        style={{ color: 'var(--accent)', textDecoration: 'underline', fontWeight: 600 }}
                      >
                        openrouter.ai ↗
                      </a>
                    </span>
                  ),
                },
                {
                  n: 2,
                  title: 'Create a free account',
                  body: 'Click the "Sign in" button at the top right of the page. You can sign up with your email address or your Google or GitHub account. It is free.',
                },
                {
                  n: 3,
                  title: 'Go to your API keys page',
                  body: (
                    <span>
                      After signing in, click your name or profile picture at the top right.
                      In the menu, click <strong>"API Keys"</strong>. Or go directly to{' '}
                      <a
                        href="https://openrouter.ai/keys"
                        target="_blank"
                        rel="noreferrer"
                        style={{ color: 'var(--accent)', textDecoration: 'underline', fontWeight: 600 }}
                      >
                        openrouter.ai/keys ↗
                      </a>
                    </span>
                  ),
                },
                {
                  n: 4,
                  title: 'Create a new API key',
                  body: (
                    <span>
                      Click the <strong>"+ Create Key"</strong> button.
                      Give it any name you like — for example: <em>My OpenFang Key</em>.
                      Then click <strong>Create</strong>.
                    </span>
                  ),
                },
                {
                  n: 5,
                  title: 'Copy your key carefully',
                  body: (
                    <span>
                      A long code will appear. It starts with <code style={{ background: 'var(--surface3)', padding: '2px 6px', borderRadius: 3, fontSize: 12 }}>sk-or-</code>.
                      {' '}<strong>Copy it now</strong> — you will only see it once.
                      Paste it into a notes app or text file while you follow the next step.
                    </span>
                  ),
                },
                {
                  n: 6,
                  title: 'Give it to the person who set up this app',
                  body: 'Show your copied key to the person who installed this app on your computer. They need to add it to the app configuration so the AI can start working. See the technical instructions below if that is you.',
                },
              ].map(item => (
                <div key={item.n} style={{ display: 'flex', gap: 14, marginBottom: 16 }}>
                  <div style={{
                    width: 30, height: 30, borderRadius: '50%', background: 'var(--accent)',
                    color: 'white', fontWeight: 800, fontSize: 14,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    flexShrink: 0, marginTop: 1,
                  }}>
                    {item.n}
                  </div>
                  <div>
                    <div style={{ fontWeight: 700, marginBottom: 3, fontSize: 14 }}>{item.title}</div>
                    <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.7 }}>{item.body}</div>
                  </div>
                </div>
              ))}
            </div>

            <Callout type="warning" icon="⚠️">
              <strong>Keep your key safe — it is like a password.</strong><br />
              Never type it into a chat window, email, or any public place.
              If you think someone else has seen it, go to openrouter.ai/keys,
              delete the old key, and create a new one.
            </Callout>

            {/* Technical instructions for the person helping */}
            <Expandable title="🛠️ For the person helping — Technical setup instructions">
              <p style={{ marginTop: 0 }}>
                <strong>Option A — OpenRouter (recommended)</strong>
              </p>
              <p>Edit <code>~/.openfang/config.toml</code> and ensure this section exists:</p>
              <CodeBlock>{`[default_model]
provider = "openrouter"
model = "openrouter/auto"
api_key_env = "OPENROUTER_API_KEY"`}</CodeBlock>
              <p>Then start the daemon with the key as an environment variable:</p>
              <CodeBlock>$env:OPENROUTER_API_KEY="sk-or-v1-..."
.\target\release\openfang.exe start</CodeBlock>

              <p style={{ marginTop: 16 }}>
                <strong>Option B — Groq (faster, free tier)</strong>
              </p>
              <p>
                Get a free key at{' '}
                <a href="https://console.groq.com/keys" target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>
                  console.groq.com/keys ↗
                </a>. Then:
              </p>
              <CodeBlock>{`[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"`}</CodeBlock>
              <CodeBlock>$env:GROQ_API_KEY="gsk_..."
.\target\release\openfang.exe start</CodeBlock>

              <p style={{ marginTop: 16 }}>
                After starting, click <strong>"Test connection"</strong> in the wizard to confirm it is working.
              </p>
            </Expandable>

            <NavBtns
              onBack={prev}
              onNext={() => {
                // Go back to status step to re-check
                setStatusChecked(false);
                goTo('check-status');
              }}
              nextLabel="Test my connection →"
              extra={
                <a
                  href="https://openrouter.ai/keys"
                  target="_blank"
                  rel="noreferrer"
                  className="btn btn-ghost btn-sm"
                >
                  Open OpenRouter ↗
                </a>
              }
            />
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 5 — First message
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'first-message' && (
          <div>
            <p style={{ color: 'var(--text-dim)', marginBottom: 20, fontSize: 15, lineHeight: 1.75 }}>
              Your AI assistant is ready! Type anything below and press{' '}
              <strong style={{ color: 'var(--text)' }}>Send</strong>.
              Not sure what to say? Pick one of the suggestions.
            </p>

            {!firstReply ? (
              <>
                {/* Suggestion chips */}
                <div style={{ marginBottom: 14 }}>
                  <div style={{
                    fontSize: 11, fontWeight: 700, color: 'var(--text-muted)',
                    marginBottom: 7, textTransform: 'uppercase', letterSpacing: '0.6px',
                  }}>
                    Try one of these
                  </div>
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                    {[
                      'Tell me something interesting!',
                      'What can you help me with?',
                      'Write me a short poem about spring.',
                      'Give me a tip for a great morning routine.',
                    ].map(s => (
                      <button
                        key={s}
                        className="btn btn-ghost btn-sm"
                        style={{ fontSize: 12 }}
                        onClick={() => setFirstMessage(s)}
                        data-cy="onboarding-suggestion"
                      >
                        {s}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Message textarea */}
                <textarea
                  ref={textareaRef}
                  value={firstMessage}
                  onChange={e => setFirstMessage(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      sendFirstMessage();
                    }
                  }}
                  placeholder="Type your message here… (Press Enter to send)"
                  rows={3}
                  style={{
                    width: '100%', padding: '12px 14px',
                    borderRadius: 'var(--radius-sm)',
                    border: '1px solid var(--border)',
                    background: 'var(--surface)', fontSize: 14,
                    resize: 'none', outline: 'none',
                    color: 'var(--text)', fontFamily: 'var(--font-sans)',
                    transition: 'border-color 0.15s',
                  }}
                  onFocus={e => { e.target.style.borderColor = 'var(--accent)'; }}
                  onBlur={e => { e.target.style.borderColor = 'var(--border)'; }}
                  data-cy="onboarding-message-input"
                />

                {sendError && (
                  <Callout type="error" icon="❌">
                    <strong>Something went wrong.</strong><br />
                    {sendError}<br />
                    <span style={{ fontSize: 12, marginTop: 6, display: 'block' }}>
                      This usually means the AI key is not set up yet.
                      Go back to <strong>Step 4 (Connect AI)</strong> for instructions,
                      or ask the person who set up this app for help.
                    </span>
                  </Callout>
                )}

                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 16 }}>
                  <button className="btn btn-ghost" onClick={prev}>← Back</button>
                  <button
                    className="btn btn-primary"
                    disabled={!firstMessage.trim() || sending}
                    onClick={sendFirstMessage}
                    style={{ minWidth: 120 }}
                    data-cy="onboarding-send"
                  >
                    {sending ? (
                      <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <span className="spinner" style={{ width: 14, height: 14, borderWidth: 2 }} />
                        Thinking…
                      </span>
                    ) : 'Send →'}
                  </button>
                </div>
              </>
            ) : (
              /* ── Reply received ── */
              <div>
                {/* User bubble */}
                <div style={{
                  background: 'var(--surface2)', borderRadius: 'var(--radius-sm)',
                  padding: '12px 16px', marginBottom: 10,
                }}>
                  <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-muted)', marginBottom: 4, letterSpacing: '0.5px' }}>
                    YOU
                  </div>
                  <div style={{ fontSize: 14 }}>{firstMessage}</div>
                </div>

                {/* AI reply bubble */}
                <div style={{
                  background: 'var(--accent-subtle)',
                  border: '1px solid var(--accent-light)',
                  borderRadius: 'var(--radius-sm)',
                  padding: '14px 18px', marginBottom: 20,
                }}>
                  <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--accent)', marginBottom: 6, letterSpacing: '0.5px' }}>
                    OPENFANG AI
                  </div>
                  <div style={{ fontSize: 14, lineHeight: 1.75, whiteSpace: 'pre-wrap' }}>
                    {firstReply}
                  </div>
                </div>

                <Callout type="success" icon="✨">
                  <strong>It worked!</strong> Your AI assistant just replied.
                  You are all set to start using OpenFang.
                </Callout>

                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 20 }}>
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => { setFirstReply(null); setSendError(null); }}
                  >
                    Try another message
                  </button>
                  <button className="btn btn-primary" onClick={next}>
                    Finish setup →
                  </button>
                </div>
              </div>
            )}
          </div>
        )}

        {/* ════════════════════════════════════════════════════════════════
            STEP 6 — Done
        ════════════════════════════════════════════════════════════════ */}
        {step?.id === 'done' && (
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: 72, lineHeight: 1, marginBottom: 20 }}>🎉</div>
            <h2 style={{ fontSize: 26, fontWeight: 800, marginBottom: 10, letterSpacing: '-0.3px' }}>
              You are all set!
            </h2>
            <p style={{ fontSize: 15, color: 'var(--text-dim)', maxWidth: 480, margin: '0 auto 28px', lineHeight: 1.75 }}>
              Your AI assistant is ready to use. Here is a quick look at what you can explore.
            </p>

            {/* Section links */}
            <div style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))',
              gap: 10, marginBottom: 28, textAlign: 'left',
            }}>
              {[
                { href: '/chat',      icon: '💬', label: 'Chat',       desc: 'Talk to your AI assistant' },
                { href: '/inbox',     icon: '📥', label: 'Inbox',      desc: 'Tasks and suggestions from AI' },
                { href: '/today',     icon: '📅', label: 'Today',      desc: 'Your daily overview' },
                { href: '/workflows', icon: '⚙️', label: 'Workflows',  desc: 'Set up automated tasks' },
                { href: '/sessions',  icon: '🤖', label: 'Agents',     desc: 'Your AI team members' },
                { href: '/settings',  icon: '🔧', label: 'Settings',   desc: 'App configuration' },
              ].map(item => (
                <Link
                  key={item.href}
                  href={item.href}
                  style={{
                    display: 'block', padding: '14px 16px',
                    background: 'var(--surface2)',
                    borderRadius: 'var(--radius-sm)',
                    border: '1px solid var(--border)',
                    textDecoration: 'none',
                    transition: 'border-color 0.15s, background 0.15s',
                  }}
                  onMouseEnter={e => {
                    e.currentTarget.style.borderColor = 'var(--accent)';
                    e.currentTarget.style.background  = 'var(--accent-subtle)';
                  }}
                  onMouseLeave={e => {
                    e.currentTarget.style.borderColor = 'var(--border)';
                    e.currentTarget.style.background  = 'var(--surface2)';
                  }}
                >
                  <div style={{ fontSize: 22, marginBottom: 5 }}>{item.icon}</div>
                  <div style={{ fontWeight: 700, fontSize: 13, marginBottom: 1 }}>{item.label}</div>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.5 }}>{item.desc}</div>
                </Link>
              ))}
            </div>

            {/* Safety reminder */}
            <div style={{
              background: 'var(--surface2)', border: '1px solid var(--border)',
              borderRadius: 'var(--radius-sm)', padding: '16px 20px',
              textAlign: 'left', marginBottom: 28,
            }}>
              <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 14 }}>🛡️ A few safety reminders</div>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 13, color: 'var(--text-dim)', lineHeight: 2 }}>
                <li>Never share your API key with anyone — treat it like a password</li>
                <li>Do not paste your API key into a chat, email, or any website</li>
                <li>If you think your key was seen by someone, go to{' '}
                  <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>
                    openrouter.ai/keys
                  </a>{' '}and delete it, then make a new one
                </li>
                <li>Your conversations stay on your device — they are not shared with anyone</li>
              </ul>
            </div>

            <div style={{ display: 'flex', gap: 14, justifyContent: 'center', flexWrap: 'wrap' }}>
              <Link href="/chat" className="btn btn-primary" style={{ fontSize: 15, padding: '13px 32px' }}>
                Start chatting →
              </Link>
              <Link href="/today" className="btn btn-ghost">
                See today's overview
              </Link>
            </div>
          </div>
        )}
      </div>

      {/* ── Help section (visible on all steps except welcome & done) ── */}
      {stepIdx > 0 && step?.id !== 'done' && (
        <div style={{ marginTop: 28 }}>
          <Expandable title="🆘 Common problems and how to fix them">
            <div style={{ display: 'grid', gap: 14 }}>
              {[
                {
                  q: 'The page says it cannot connect to the app backend',
                  a: 'The background service is not running. If someone installed this for you, ask them to start it. If you installed it yourself, run the backend with: .\\target\\release\\openfang.exe start',
                },
                {
                  q: 'I set up an API key but the AI is still not working',
                  a: 'The key needs to be added to the config file AND the app backend needs to be restarted. Ask the person who set this up to restart it with the new key.',
                },
                {
                  q: 'The page is just loading and nothing happens',
                  a: 'Try refreshing your browser. If that does not help, make sure you are opening the right address — usually http://localhost:3002. Check that both the backend and frontend are running.',
                },
                {
                  q: 'I see "Invalid API key" or "Unauthorized"',
                  a: 'The API key may have been typed incorrectly or deleted. Go to openrouter.ai/keys, create a fresh key, and ask the person who set up this app to update the config with the new key.',
                },
                {
                  q: 'I accidentally shared my API key with someone',
                  a: 'Go to openrouter.ai/keys right away. Find the key and click Delete. Then create a new one. Ask the person who set up this app to update the config file with your new key.',
                },
                {
                  q: 'The first message I sent got a strange error',
                  a: "This usually means the AI is not quite connected yet. Go back to the 'Connect AI' step and follow the instructions to restart the backend with your API key.",
                },
              ].map(item => (
                <div key={item.q} style={{ paddingBottom: 12, borderBottom: '1px solid var(--border-subtle)' }}>
                  <div style={{ fontWeight: 700, marginBottom: 4, fontSize: 13 }}>❓ {item.q}</div>
                  <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.65 }}>→ {item.a}</div>
                </div>
              ))}
            </div>
          </Expandable>

          {/* Quick-connect mode: show full wizard option */}
          <div style={{ marginTop: 12, textAlign: 'center' }}>
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => setStepIdx(0)}
              style={{ fontSize: 12, color: 'var(--text-muted)' }}
            >
              Restart from the beginning
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
