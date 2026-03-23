"use client";

import Link from "next/link";
import styles from "../../client-dashboard.module.css";

type Props = {
  actions: string[];
  latestRunHref: string;
};

export default function FounderNextActionsPanel({ actions, latestRunHref }: Props) {
  const primaryAction = actions[0] ?? null;

  return (
    <div className={styles.founderNextActionsPanel} data-cy="founder-next-actions-panel">
      <div className={styles.fieldLabel}>Next actions</div>
      {primaryAction ? (
        <>
          <div className={styles.founderPrimaryNextActionCard}>
            <div className={styles.fieldLabel}>Most important next step</div>
            <div className={styles.founderPrimaryNextAction}>{primaryAction}</div>
          </div>
          <div className={styles.stackTight}>
            {actions.slice(1, 5).map((action, index) => (
              <div key={`${action}-${index}`} className={styles.founderActionItem}>
                <span className={styles.founderActionIndex}>{index + 2}</span>
                <span>{action}</span>
              </div>
            ))}
          </div>
          <div className={styles.cardActions}>
            <Link href={latestRunHref} className={styles.linkButton}>
              Review full result
            </Link>
          </div>
        </>
      ) : (
        <div className={styles.mutedText}>No next steps have been saved yet. Finish a founder run to turn the result into a concrete to-do list.</div>
      )}
    </div>
  );
}