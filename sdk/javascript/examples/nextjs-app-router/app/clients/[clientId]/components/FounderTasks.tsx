'use client';

import { useEffect, useState } from 'react';
import { getFounderTasks, updateFounderTaskStatus } from '../lib/client-api';
import type { FounderTaskItem, FounderTaskStatus } from '../lib/client-types';
import styles from '../../client-dashboard.module.css';

type Props = {
  workspaceId: string;
};

const STATUS_LABELS: Record<FounderTaskStatus, string> = {
  pending: 'Pending',
  in_progress: 'In Progress',
  completed: 'Completed',
  dismissed: 'Dismissed',
};

export default function FounderTasks({ workspaceId }: Props) {
  const [tasks, setTasks] = useState<FounderTaskItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [mutatingTaskId, setMutatingTaskId] = useState('');

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setLoading(true);
      const nextTasks = await getFounderTasks(workspaceId);
      if (cancelled) return;
      setTasks(nextTasks);
      setError('');
      setLoading(false);
    }

    load().catch((event) => {
      if (cancelled) return;
      setError(event instanceof Error ? event.message : 'Failed to load founder tasks.');
      setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [workspaceId]);

  async function handleStatusChange(taskId: string, status: FounderTaskStatus) {
    if (mutatingTaskId === taskId) return;
    const previous = tasks;
    const current = tasks.find((task) => task.taskId === taskId);
    if (!current || current.status === status) return;

    setMutatingTaskId(taskId);
    setTasks((items) => items.map((task) => (
      task.taskId === taskId
        ? { ...task, status, updatedAt: new Date().toISOString() }
        : task
    )));
    setError('');

    const updated = await updateFounderTaskStatus(workspaceId, taskId, status);
    setMutatingTaskId('');

    if (!updated) {
      setTasks(previous);
      setError('Failed to update task. The change was reverted.');
      return;
    }

    setTasks((items) => items.map((task) => (task.taskId === taskId ? updated : task)));
  }

  if (loading) {
    return <div className={styles.mutedText}>Loading founder tasks...</div>;
  }

  return (
    <section className={`${styles.card} ${styles.span7}`}>
      <div className={styles.sectionTitle}>Founder tasks</div>
      {error ? <div className={styles.errorBanner}>{error}</div> : null}
      {tasks.length === 0 ? (
        <div className={styles.mutedText}>No tasks have been ingested from founder runs yet.</div>
      ) : (
        <div className={styles.stack}>
          {tasks.map((task) => {
            const terminal = task.status === 'completed' || task.status === 'dismissed';
            return (
              <div key={task.taskId} className={styles.taskRow}>
                <div className={styles.taskRowBody}>
                  <div className={terminal ? styles.taskTextDone : styles.itemTitle}>{task.description}</div>
                  <div className={styles.itemMeta}>
                    {task.category.replace(/_/g, ' ')} · from run {task.runId.slice(0, 8)}
                  </div>
                </div>
                <select
                  aria-label={`Update status for founder task ${task.description}`}
                  className={styles.taskStatusSelect}
                  disabled={mutatingTaskId === task.taskId}
                  value={task.status}
                  onChange={(event) => handleStatusChange(task.taskId, event.target.value as FounderTaskStatus)}
                >
                  {Object.entries(STATUS_LABELS).map(([value, label]) => (
                    <option key={value} value={value}>{label}</option>
                  ))}
                </select>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}