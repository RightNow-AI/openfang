"use client";

import Link from "next/link";
import styles from "../../client-dashboard.module.css";

type Props = {
  startHref: string;
};

export default function FounderWorkspaceEmptyState({ startHref }: Props) {
  return (
    <section className={`${styles.card} ${styles.founderPanel}`} data-cy="founder-workspace-panel">
      <div className={styles.founderPanelHeader}>
        <div>
          <div className={styles.sectionTitle}>Founder workspace</div>
          <div className={styles.sectionLead}>
            Start here if the client needs founder research, a playbook, and clear next steps.
          </div>
        </div>
        <Link href={startHref} className={`${styles.linkButton} ${styles.linkPrimary}`} data-cy="founder-empty-state-cta">
          Start founder setup
        </Link>
      </div>
      <div className={`${styles.founderEmptyState} ${styles.founderEmptyStateStrong}`} data-cy="founder-empty-state">
        <div className={styles.itemTitle}>No founder workspace yet</div>
        <div className={styles.bodyText}>
          The guided setup takes a minute. Pick a playbook, describe the company or idea in plain language, and start the first research run.
        </div>
        <div className={styles.badgeRow}>
          <span className={styles.badge}>Choose a playbook</span>
          <span className={styles.badge}>Add company details</span>
          <span className={styles.badge}>Start research</span>
        </div>
      </div>
    </section>
  );
}