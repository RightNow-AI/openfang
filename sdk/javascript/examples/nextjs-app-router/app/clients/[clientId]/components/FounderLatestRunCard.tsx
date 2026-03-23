"use client";

import Link from "next/link";
import type { FounderRunItem } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

type Props = {
  latestRun: FounderRunItem | null;
  playbookLabel: string;
  latestRunHref: string;
  fallbackHref: string;
  formatWhen: (value: string) => string;
  citationPreview: (citations: string[]) => string;
};

export default function FounderLatestRunCard({
  latestRun,
  playbookLabel,
  latestRunHref,
  fallbackHref,
  formatWhen,
  citationPreview,
}: Props) {
  return (
    <div className={styles.founderLatestRunCard} data-cy="founder-latest-run-card">
      <div className={styles.founderCardHeaderRow}>
        <div className={styles.fieldLabel}>Latest founder run</div>
        {latestRun ? <span className={styles.badge}>{latestRun.status.replace(/_/g, " ")}</span> : null}
      </div>
      {latestRun ? (
        <>
          <div className={styles.itemTitle}>{playbookLabel}</div>
          <div className={styles.itemMeta}>Saved {formatWhen(latestRun.updatedAt || latestRun.createdAt)}</div>
          <div className={styles.bodyText}>
            {latestRun.summary || latestRun.prompt || "This founder run is saved, but it still needs a summary."}
          </div>
          <div className={styles.founderCitationRow}>{citationPreview(latestRun.citations)}</div>
          <div className={styles.cardActions}>
            <Link href={latestRunHref} className={styles.linkButton} data-cy="founder-reopen-latest-run-cta">
              Open latest result
            </Link>
          </div>
        </>
      ) : (
        <>
          <div className={styles.itemTitle}>No founder run saved yet</div>
          <div className={styles.mutedText}>
            The workspace is ready. Start the first run so this panel can show the summary, citations, and latest result.
          </div>
          <div className={styles.cardActions}>
            <Link href={fallbackHref} className={styles.linkButton}>
              Start the first run
            </Link>
          </div>
        </>
      )}
    </div>
  );
}