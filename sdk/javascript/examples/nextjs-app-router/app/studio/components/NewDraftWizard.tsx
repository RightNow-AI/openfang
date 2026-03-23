'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState } from 'react';

type Props = {
  workspaceId: string;
};

const PLAYBOOKS = [
  { id: 'educational_shorts', title: 'Educational', icon: 'Brain', desc: 'Facts, deep dives, and clear teaching beats.' },
  { id: 'reddit_stories', title: 'Storytime', icon: 'Story', desc: 'Narrative structure, tension, and payoff.' },
  { id: 'motivational', title: 'Motivational', icon: 'Drive', desc: 'Punchy rhythm, energy, and high-emotion delivery.' },
];

export default function NewDraftWizard({ workspaceId }: Props) {
  const router = useRouter();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [formData, setFormData] = useState({
    topic: '',
    playbook: 'educational_shorts',
    format: 'faceless_short',
    targetDurationSec: 45,
  });

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!formData.topic.trim()) return;

    setIsSubmitting(true);
    setError(null);

    try {
      const response = await fetch(`/api/studio/workspaces/${workspaceId}/drafts`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(formData),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error((payload as { error?: string }).error || 'Failed to initialize the draft.');
      }

      const draft = (payload as { draft?: { id?: string }; id?: string }).draft;
      const draftId = draft?.id || (payload as { id?: string }).id;
      if (!draftId) {
        throw new Error('The draft was created but the response did not include an id.');
      }

      router.push(`/studio/${workspaceId}/drafts/${draftId}/research`);
    } catch (caughtError) {
      setError(caughtError instanceof Error ? caughtError.message : 'Could not start the research engine.');
      setIsSubmitting(false);
    }
  }

  return (
    <main style={{ maxWidth: 960, margin: '0 auto', padding: '28px 32px 56px' }}>
      <div style={{ marginBottom: 22 }}>
        <Link href={`/studio/${workspaceId}`} style={{ textDecoration: 'none', color: '#fdba74', fontWeight: 700, fontSize: 14 }}>
          ← Back to dashboard
        </Link>
        <h1 style={{ margin: '16px 0 8px', fontSize: 34, fontWeight: 900 }}>Start a new short</h1>
        <p style={{ margin: 0, color: 'var(--text-dim)', fontSize: 15, maxWidth: 760, lineHeight: 1.55 }}>
          Drop the raw idea here. The pipeline turns it into a researched angle, a script, a voice track, visuals, a render, and a publish-ready package.
        </p>
      </div>

      <form onSubmit={handleSubmit} style={{ display: 'grid', gap: 22, borderRadius: 28, padding: 28, border: '1px solid rgba(249,115,22,0.16)', background: 'linear-gradient(180deg, rgba(17,24,39,0.92), rgba(10,14,22,0.94))', boxShadow: 'var(--shadow-sm)' }}>
        <section>
          <div style={{ fontSize: 18, fontWeight: 800, marginBottom: 8 }}>What is the video about?</div>
          <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', marginBottom: 12 }}>Be direct. The research stage will sharpen the hook and positioning.</div>
          <textarea
            required
            rows={4}
            value={formData.topic}
            onChange={(event) => setFormData((current) => ({ ...current, topic: event.target.value }))}
            placeholder="Why premium golf brands win on systems, not just style."
            style={{ width: '100%', boxSizing: 'border-box', padding: '16px 18px', borderRadius: 18, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit', fontSize: 16, resize: 'vertical', minHeight: 120 }}
          />
        </section>

        <section>
          <div style={{ fontSize: 18, fontWeight: 800, marginBottom: 12 }}>Which playbook should the AI use?</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(210px, 1fr))', gap: 14 }}>
            {PLAYBOOKS.map((playbook) => {
              const selected = formData.playbook === playbook.id;
              return (
                <button
                  key={playbook.id}
                  type="button"
                  onClick={() => setFormData((current) => ({ ...current, playbook: playbook.id }))}
                  style={{ textAlign: 'left', padding: 18, borderRadius: 18, border: selected ? '1px solid rgba(249,115,22,0.48)' : '1px solid rgba(255,255,255,0.08)', background: selected ? 'rgba(249,115,22,0.1)' : 'rgba(255,255,255,0.03)', color: 'inherit', cursor: 'pointer' }}
                >
                  <div style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: 0.8, color: selected ? '#fdba74' : 'rgba(255,255,255,0.52)', marginBottom: 8 }}>{playbook.icon}</div>
                  <div style={{ fontSize: 16, fontWeight: 800, marginBottom: 6 }}>{playbook.title}</div>
                  <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.62)', lineHeight: 1.5 }}>{playbook.desc}</div>
                </button>
              );
            })}
          </div>
        </section>

        <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))', gap: 14 }}>
          {[30, 45, 60].map((seconds) => {
            const selected = formData.targetDurationSec === seconds;
            return (
              <button
                key={seconds}
                type="button"
                onClick={() => setFormData((current) => ({ ...current, targetDurationSec: seconds }))}
                style={{ padding: '16px 18px', borderRadius: 18, border: selected ? '1px solid rgba(249,115,22,0.48)' : '1px solid rgba(255,255,255,0.08)', background: selected ? '#f97316' : 'rgba(255,255,255,0.03)', color: selected ? '#fff' : 'inherit', fontWeight: 800, cursor: 'pointer' }}
              >
                {seconds} seconds
              </button>
            );
          })}
        </section>

        {error ? <div style={{ padding: 14, borderRadius: 16, border: '1px solid rgba(249,115,22,0.22)', background: 'rgba(249,115,22,0.1)', color: '#fdba74', fontSize: 14 }}>{error}</div> : null}

        <button
          type="submit"
          disabled={isSubmitting || !formData.topic.trim()}
          style={{ padding: '16px 18px', borderRadius: 18, border: 'none', background: '#f97316', color: '#fff', fontSize: 17, fontWeight: 900, cursor: isSubmitting ? 'progress' : 'pointer', opacity: isSubmitting || !formData.topic.trim() ? 0.7 : 1 }}
        >
          {isSubmitting ? 'Initializing draft…' : 'Start research engine →'}
        </button>
      </form>
    </main>
  );
}