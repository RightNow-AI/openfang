'use client';

import { useEffect, useRef, useState } from 'react';

import type { StudioArtifact, StudioPipelineEvent } from '../app/studio/lib/studio-types';

type ActiveJob = {
  jobId?: string;
  stage?: string;
  label: string;
};

type Options = {
  initialArtifacts?: StudioArtifact[];
  initialStage?: string;
  initialStatus?: string;
};

function stageLabel(stage?: string) {
  if (!stage) return 'pipeline';
  return stage.replace(/_/g, ' ');
}

function upsertArtifact(list: StudioArtifact[], nextArtifact: StudioArtifact) {
  const next = [...list];
  const index = next.findIndex((artifact) => artifact.id === nextArtifact.id);
  if (index >= 0) {
    next[index] = nextArtifact;
  } else {
    next.unshift(nextArtifact);
  }
  return next;
}

export function useDraftEvents(draftId: string, options: Options = {}) {
  const [events, setEvents] = useState<StudioPipelineEvent[]>([]);
  const [artifacts, setArtifacts] = useState<StudioArtifact[]>(options.initialArtifacts ?? []);
  const [isConnected, setIsConnected] = useState(false);
  const [activeJob, setActiveJob] = useState<ActiveJob | null>(null);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [currentStage, setCurrentStage] = useState(options.initialStage ?? 'research');
  const [currentStatus, setCurrentStatus] = useState(options.initialStatus ?? 'Draft');
  const seenIds = useRef<Set<string>>(new Set());

  useEffect(() => {
    setArtifacts(options.initialArtifacts ?? []);
    setCurrentStage(options.initialStage ?? 'research');
    setCurrentStatus(options.initialStatus ?? 'Draft');
  }, [options.initialArtifacts, options.initialStage, options.initialStatus]);

  useEffect(() => {
    if (!draftId) return;

    seenIds.current = new Set();
    setEvents([]);
    setError(null);
    setProgress(0);

    const source = new EventSource(`/api/studio/drafts/${draftId}/events`);

    source.onopen = () => {
      setIsConnected(true);
    };

    source.onerror = () => {
      setIsConnected(false);
    };

    source.onmessage = (message) => {
      if (!message.data) return;

      try {
        const event = JSON.parse(message.data) as StudioPipelineEvent;
        const fingerprint = `${event.type}:${event.timestamp}:${event.jobId ?? 'no-job'}:${event.artifact?.id ?? event.stage ?? 'no-stage'}`;
        if (seenIds.current.has(fingerprint)) return;
        seenIds.current.add(fingerprint);

        setEvents((current) => [...current, event]);

        if (event.type === 'job.started') {
          setActiveJob({
            jobId: event.jobId,
            stage: event.stage,
            label: `${stageLabel(event.stage)} engine`,
          });
          setProgress(event.progress ?? 0);
          setError(null);
          if (event.stage) setCurrentStage(event.stage);
          return;
        }

        if (event.type === 'job.progress') {
          setActiveJob((current) => current ?? {
            jobId: event.jobId,
            stage: event.stage,
            label: `${stageLabel(event.stage)} engine`,
          });
          setProgress(event.progress ?? 0);
          if (event.stage) setCurrentStage(event.stage);
          return;
        }

        if (event.type === 'artifact.created' && event.artifact) {
          setArtifacts((current) => upsertArtifact(current, event.artifact!));
          return;
        }

        if (event.type === 'draft.stage_changed') {
          setActiveJob(null);
          setProgress(100);
          if (event.stage) setCurrentStage(event.stage);
          if (event.status) setCurrentStatus(event.status);
          return;
        }

        if (event.type === 'job.failed') {
          setActiveJob(null);
          setCurrentStatus(event.status ?? 'Failed');
          setError(event.error ?? 'The generation job failed.');
        }
      } catch {
        setError('Could not parse the live studio event stream.');
      }
    };

    return () => {
      source.close();
    };
  }, [draftId]);

  return {
    events,
    artifacts,
    isConnected,
    activeJob,
    progress,
    error,
    currentStage,
    currentStatus,
  };
}