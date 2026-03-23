"use client";

import type { FounderRunItem, FounderWorkspaceSnapshot } from "../lib/client-types";
import FounderLatestRunCard from "./FounderLatestRunCard";
import FounderNextActionsPanel from "./FounderNextActionsPanel";
import FounderRecentRunsList from "./FounderRecentRunsList";
import FounderWorkspaceEmptyState from "./FounderWorkspaceEmptyState";
import FounderWorkspaceSummaryCard from "./FounderWorkspaceSummaryCard";
import styles from "../../client-dashboard.module.css";

type Props = {
  clientId: string;
  clientName: string;
  founderHref: string;
  founderStartHref: string;
  loading: boolean;
  error: string;
  snapshot: FounderWorkspaceSnapshot | null;
};

function formatWhen(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Saved recently";
  return date.toLocaleString();
}

function labelForPlaybook(playbookId: string | null | undefined, labels: Record<string, string>) {
  if (!playbookId) return "Founder research";
  return labels[playbookId] || playbookId.replace(/[-_]+/g, " ");
}

function citationPreview(citations: string[]) {
  if (citations.length === 0) return "No citations saved yet";

  const preview = citations
    .slice(0, 2)
    .map((citation) => {
      try {
        return new URL(citation).hostname.replace(/^www\./, "");
      } catch {
        return citation;
      }
    })
    .join(" · ");

  return `${citations.length} citation${citations.length === 1 ? "" : "s"} · ${preview}`;
}

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

export default function FounderWorkspacePanel({
  clientId,
  clientName,
  founderHref,
  founderStartHref,
  loading,
  error,
  snapshot,
}: Props) {
  if (loading) {
    return (
      <section className={`${styles.card} ${styles.founderPanel}`} data-cy="founder-workspace-panel">
        <div className={styles.sectionTitle}>Founder workspace</div>
        <div className={styles.mutedText}>Loading founder workspace…</div>
      </section>
    );
  }

  if (error) {
    return (
      <section className={`${styles.card} ${styles.founderPanel}`} data-cy="founder-workspace-panel">
        <div className={styles.sectionTitle}>Founder workspace</div>
        <div className={styles.errorBanner}>{error}</div>
        <div className={styles.cardActions}>
          <a href={founderStartHref} className={`${styles.linkButton} ${styles.linkPrimary}`}>Start founder setup</a>
          <a href={founderHref} className={styles.linkButton}>Open founder research</a>
        </div>
      </section>
    );
  }

  const workspace = snapshot?.workspace ?? null;
  const runs = snapshot?.runs ?? [];
  const playbookLabels = snapshot?.playbookLabels ?? {};
  const latestRun = runs[0] ?? null;
  const defaultPlaybookId = typeof workspace?.playbookDefaults?.defaultPlaybookId === "string"
    ? workspace.playbookDefaults.defaultPlaybookId
    : null;
  const currentPlaybookId = latestRun?.playbookId ?? defaultPlaybookId;
  const currentPlaybookLabel = labelForPlaybook(currentPlaybookId, playbookLabels);

  if (!workspace) {
    return (
      <FounderWorkspaceEmptyState startHref={founderStartHref} />
    );
  }

  const latestRunHref = latestRun
    ? buildRunHref(latestRun, workspace.workspaceId, clientId, clientName)
    : founderStartHref;

  const founderStatusLabel = latestRun
    ? latestRun.status === "completed"
      ? "Result ready"
      : latestRun.status === "failed"
        ? "Needs retry"
        : "Research running"
    : "Ready to start";
  const founderStatusTone = latestRun
    ? latestRun.status === "failed"
      ? "danger"
      : latestRun.status === "completed"
        ? "accent"
        : "warn"
    : "accent";
  const primaryActionLabel = latestRun ? "Resume founder work" : "Start founder research";
  const highlightedNextAction = latestRun?.nextActions?.[0] || "Pick a playbook and start the first founder run.";
  const nextStepDescription = latestRun
    ? latestRun.status === "completed"
      ? "Open the latest result, review the top insight, and save the next steps into the client plan."
      : latestRun.status === "failed"
        ? "Open the founder workspace, fix the question or playbook choice, and start again."
        : "Open the founder workspace to watch progress and review the result when it lands."
    : "Use the guided start flow to choose a playbook, add company context, and launch the first analysis.";

  return (
    <section className={`${styles.card} ${styles.founderPanel}`} data-cy="founder-workspace-panel">
      <div className={styles.founderPanelHeader}>
        <div>
          <div className={styles.sectionTitle}>Founder workspace</div>
          <div className={styles.sectionLead}>
            Founder research, next steps, and recent runs for {workspace.companyName || clientName}.
          </div>
        </div>
        <div className={styles.inlineWrap}>
          <span className={`${styles.badge} ${styles.badgeAccent}`} data-cy="founder-playbook-badge">{currentPlaybookLabel}</span>
          <span className={styles.badge}>{workspace.stage || "validation"}</span>
        </div>
      </div>

      <div className={styles.founderPanelGrid}>
        <div className={styles.founderPrimaryColumn}>
          <FounderWorkspaceSummaryCard
            workspace={workspace}
            statusLabel={founderStatusLabel}
            statusTone={founderStatusTone}
            currentPlaybookLabel={currentPlaybookLabel}
            nextStepLabel={highlightedNextAction}
            nextStepDescription={nextStepDescription}
            primaryActionHref={latestRun ? latestRunHref : founderStartHref}
            primaryActionLabel={primaryActionLabel}
            secondaryActionHref={founderStartHref}
            secondaryActionLabel={latestRun ? "Use guided setup" : "See guided steps"}
          />

          <FounderLatestRunCard
            latestRun={latestRun}
            playbookLabel={currentPlaybookLabel}
            latestRunHref={latestRunHref}
            fallbackHref={founderStartHref}
            formatWhen={formatWhen}
            citationPreview={citationPreview}
          />
        </div>

        <div className={styles.founderSecondaryColumn}>
          <FounderNextActionsPanel
            actions={latestRun?.nextActions ?? []}
            latestRunHref={latestRun ? latestRunHref : founderStartHref}
          />

          <FounderRecentRunsList
            runs={runs}
            workspaceId={workspace.workspaceId}
            clientId={clientId}
            clientName={clientName}
            playbookLabels={playbookLabels}
            formatWhen={formatWhen}
            labelForPlaybook={labelForPlaybook}
          />
        </div>
      </div>
    </section>
  );
}