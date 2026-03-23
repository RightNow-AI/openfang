"use client";

import Link from "next/link";
import type { FounderRunItem } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

type Props = {
  runs: FounderRunItem[];
  workspaceId: string;
  clientId: string;
  clientName: string;
  playbookLabels: Record<string, string>;
  formatWhen: (value: string) => string;
  labelForPlaybook: (playbookId: string | null | undefined, labels: Record<string, string>) => string;
};

function buildRunHref(
  run: FounderRunItem,
  workspaceId: string,
  clientId: string,
  clientName: string,
) {
  return `/deep-research?${new URLSearchParams({
    clientId,
    clientName,
    workspaceId,
    runId: run.runId,
    ...(run.playbookId ? { playbookId: run.playbookId } : {}),
  }).toString()}`;
}

export default function FounderRecentRunsList({
  runs,
  workspaceId,
  clientId,
  clientName,
  playbookLabels,
  formatWhen,
  labelForPlaybook,
}: Props) {
  return (
    <div className={styles.founderRecentRunsPanel} data-cy="founder-recent-runs-panel">
      <div className={styles.fieldLabel}>Recent founder runs</div>
      {runs.length === 0 ? (
        <div className={styles.mutedText}>Recent founder runs will appear here after the first saved result.</div>
      ) : (
        <div className={styles.stackTight}>
          {runs.slice(0, 4).map((run) => (
            <Link
              key={run.runId}
              href={buildRunHref(run, workspaceId, clientId, clientName)}
              className={styles.founderRunLink}
              data-cy="founder-recent-run-link"
            >
              <div className={styles.founderRunTopRow}>
                <div className={styles.itemTitle}>{labelForPlaybook(run.playbookId, playbookLabels)}</div>
                <span className={styles.badge}>{run.status}</span>
              </div>
              <div className={styles.itemMeta}>{formatWhen(run.updatedAt || run.createdAt)}</div>
              <div className={styles.founderRunPreview}>{run.summary || run.prompt}</div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}