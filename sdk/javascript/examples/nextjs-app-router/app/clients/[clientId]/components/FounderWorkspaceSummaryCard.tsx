"use client";

import Link from "next/link";
import type { FounderWorkspaceItem } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

type Props = {
  workspace: FounderWorkspaceItem;
  statusLabel: string;
  statusTone: "accent" | "warn" | "danger";
  currentPlaybookLabel: string;
  nextStepLabel: string;
  nextStepDescription: string;
  primaryActionHref: string;
  primaryActionLabel: string;
  secondaryActionHref?: string;
  secondaryActionLabel?: string;
};

export default function FounderWorkspaceSummaryCard({
  workspace,
  statusLabel,
  statusTone,
  currentPlaybookLabel,
  nextStepLabel,
  nextStepDescription,
  primaryActionHref,
  primaryActionLabel,
  secondaryActionHref,
  secondaryActionLabel,
}: Props) {
  return (
    <div className={`${styles.founderWorkspaceSummary} ${styles.founderSummaryCard}`} data-cy="founder-workspace-summary">
      <div className={styles.founderSummaryTopRow}>
        <div>
          <div className={styles.fieldLabel}>Founder status</div>
          <div className={styles.itemTitle}>{workspace.name}</div>
        </div>
        <span className={`${styles.badge} ${statusTone === "danger" ? styles.badgeDanger : statusTone === "warn" ? styles.badgeWarn : styles.badgeAccent}`}>
          {statusLabel}
        </span>
      </div>

      <div className={styles.badgeRow}>
        <span className={`${styles.badge} ${styles.badgeAccent}`}>{currentPlaybookLabel}</span>
        <span className={styles.badge}>{workspace.stage || "validation"}</span>
      </div>

      <div className={styles.founderSummaryIdeaBlock}>
        <div className={styles.fieldLabel}>What this founder work is about</div>
        <div className={styles.bodyText}>{workspace.idea || "Add the company idea so the research stays focused."}</div>
      </div>

      <div className={styles.founderNextStepBlock}>
        <div className={styles.fieldLabel}>Do this next</div>
        <div className={styles.founderPrimaryNextAction}>{nextStepLabel}</div>
        <div className={styles.itemMeta}>{nextStepDescription}</div>
      </div>

      <div className={styles.founderSummaryActions}>
        <Link href={primaryActionHref} className={`${styles.linkButton} ${styles.linkPrimary}`}>
          {primaryActionLabel}
        </Link>
        {secondaryActionHref && secondaryActionLabel ? (
          <Link href={secondaryActionHref} className={styles.linkButton}>
            {secondaryActionLabel}
          </Link>
        ) : null}
      </div>
    </div>
  );
}