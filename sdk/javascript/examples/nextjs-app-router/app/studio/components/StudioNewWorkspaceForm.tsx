'use client';

import { useRouter } from 'next/navigation';
import type { FormEvent } from 'react';
import { useState, useTransition } from 'react';
import styles from './studio-surfaces.module.css';

export default function StudioNewWorkspaceForm() {
  const router = useRouter();
  const [isPending, startTransition] = useTransition();
  const [error, setError] = useState('');
  const [form, setForm] = useState({
    name: '',
    niche: '',
    platform: 'youtube',
    language: 'en',
    publishGoalPerDay: 2,
  });

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError('');
    const response = await fetch('/api/studio/workspaces', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(form),
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
      setError(payload.error || 'Could not create the studio workspace.');
      return;
    }
    const workspaceId = payload.workspace?.id || payload.id;
    if (!workspaceId) {
      setError('Workspace was created but the response did not include an id.');
      return;
    }
    startTransition(() => {
      router.push(`/studio/${workspaceId}`);
      router.refresh();
    });
  }

  return (
    <main className={`${styles.page} ${styles.pageNarrow}`}>
      <div className={styles.intro}>
        <h1 className={styles.introTitle}>Create channel workspace</h1>
        <p className={styles.introCopy}>
          Define the channel once, then run every short through research, script, voice, visuals, edit, and publish from the same dashboard.
        </p>
      </div>

      <form onSubmit={handleSubmit} className={`${styles.card} ${styles.formCard}`}>
        <label className={styles.field}>
          <span className={styles.labelText}>Workspace name</span>
          <input
            required
            value={form.name}
            onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
            placeholder="Legends League Shorts"
            className={styles.input}
          />
        </label>

        <label className={styles.field}>
          <span className={styles.labelText}>Niche</span>
          <input
            required
            value={form.niche}
            onChange={(event) => setForm((current) => ({ ...current, niche: event.target.value }))}
            placeholder="Faceless history, business explainers, premium golf"
            className={styles.input}
          />
        </label>

        <div className={styles.fieldGrid}>
          <label className={styles.field}>
            <span className={styles.labelText}>Platform</span>
            <select value={form.platform} onChange={(event) => setForm((current) => ({ ...current, platform: event.target.value }))} className={styles.input}>
              <option value="youtube">YouTube</option>
              <option value="tiktok">TikTok</option>
            </select>
          </label>

          <label className={styles.field}>
            <span className={styles.labelText}>Language</span>
            <input value={form.language} onChange={(event) => setForm((current) => ({ ...current, language: event.target.value }))} placeholder="en" className={styles.input} />
          </label>

          <label className={styles.field}>
            <span className={styles.labelText}>Target posts per day</span>
            <input type="number" min={1} max={12} value={form.publishGoalPerDay} onChange={(event) => setForm((current) => ({ ...current, publishGoalPerDay: Number(event.target.value) || 1 }))} className={styles.input} />
          </label>
        </div>

        <div className={styles.callout}>
          <div className={styles.calloutTitle}>What this unlocks</div>
          <div className={styles.calloutCopy}>
            One dashboard for draft backlog, action queue, alerts, and the live creation pipeline for each short.
          </div>
        </div>

        {error ? <div className={styles.errorText}>{error}</div> : null}

        <div className={styles.footerRow}>
          <div className={styles.supportCopy}>The React pipeline is already wired to the Next BFF and can hand off to Rust endpoints when they are available.</div>
          <button type="submit" disabled={isPending} className={styles.submitButton}>
            {isPending ? 'Creating…' : 'Create workspace'}
          </button>
        </div>
      </form>
    </main>
  );
}
