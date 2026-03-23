"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "./components/ClientShell";
import { getClientHome } from "./lib/client-api";
import type { ClientHomeResponse } from "./lib/client-types";
import styles from "../client-dashboard.module.css";

export default function ClientHomePage() {
  const params = useParams<{ clientId: string }>();
  const clientId = Array.isArray(params?.clientId) ? params.clientId[0] || "" : params?.clientId || "";
  const [data, setData] = useState<ClientHomeResponse | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!clientId) return;

    getClientHome(clientId).then(setData).catch((event: Error) => setError(event.message));
  }, [clientId]);

  if (error) return <main className={`${styles.statusPage} ${styles.errorText}`}>Error: {error}</main>;
  if (!data) return <main className={styles.statusPage}>Loading…</main>;

  return (
    <ClientShell
      clientId={clientId}
      clientName={data.client.name}
      currentPage="home"
      approvalsWaiting={data.client.approvals_waiting}
      tasksDueToday={data.client.tasks_due_today}
      lastActivityAt={data.client.last_activity_at}
      health={data.client.health}
    >
      <div className={styles.dashboardGrid} data-cy="client-home-page">
        <section className={`${styles.card} ${styles.span8}`}>
          <div className={styles.sectionTitle}>Client alignment</div>
          <div className={styles.infoGrid}>
            <div>
              <div className={styles.fieldLabel}>Industry</div>
              <div className={styles.itemTitle}>{data.client.industry}</div>
            </div>
            <div>
              <div className={styles.fieldLabel}>Approver</div>
              <div className={styles.itemTitle}>{data.client.approver_name}</div>
            </div>
            <div className={styles.infoGridFull}>
              <div className={styles.fieldLabel}>Main goal</div>
              <div className={styles.bodyText}>{data.client.main_goal}</div>
            </div>
          </div>
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Quick actions</div>
          <div className={styles.actionColumn}>
            <Link href={`/clients/${clientId}/plan`} className={`${styles.linkButton} ${styles.linkPrimary}`}>Start today&apos;s work</Link>
            <Link href={`/clients/${clientId}/approvals`} className={styles.linkButton}>Review approvals</Link>
            <Link href={`/clients/${clientId}/results`} className={styles.linkButton}>Draft client update</Link>
          </div>
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Today&apos;s priorities</div>
          {data.priorities.length === 0 ? (
            <div className={styles.mutedText}>No active priorities yet.</div>
          ) : (
            <div className={styles.stack}>
              {data.priorities.map((item) => (
                <div key={item.id} className={styles.itemCard}>
                  <div className={styles.itemTitle}>{item.title}</div>
                  <div className={styles.itemMeta}>{item.owner_label}</div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Approvals waiting</div>
          {data.approvals_waiting.length === 0 ? (
            <div className={styles.mutedText}>No pending approvals.</div>
          ) : (
            <div className={styles.stack}>
              {data.approvals_waiting.slice(0, 3).map((item) => (
                <div key={item.id} className={`${styles.itemCard} ${styles.itemCardWarn}`}>
                  <div className={styles.itemTitle}>{item.title}</div>
                  <div className={`${styles.itemMeta} ${styles.itemMetaWarn}`}>{item.approval_type.replace(/_/g, " ")}</div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Client health</div>
          <div className={styles.pageTitle}>{data.health_summary.delivery_confidence}%</div>
          <div className={styles.sectionLead}>
            Renewal likelihood {data.health_summary.renewal_likelihood ?? "n/a"}% · Approval lag {data.health_summary.approval_lag_hours ?? "n/a"}h
          </div>
        </section>

        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Blocked work</div>
          {data.blocked_tasks.length === 0 ? (
            <div className={styles.mutedText}>No blocked work right now.</div>
          ) : (
            <div className={styles.stack}>
              {data.blocked_tasks.map((task) => (
                <div key={task.id} className={`${styles.itemCard} ${styles.itemCardDanger}`}>
                  <div className={styles.itemTitle}>{task.title}</div>
                  <div className={styles.itemMeta}>{task.owner_label} · {task.status.replace(/_/g, " ")}</div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Recent activity</div>
          <div className={styles.dividerList}>
            {data.recent_activity.map((item) => (
              <div key={item.id}>
                <div className={styles.itemTitle}>{item.title}</div>
                <div className={styles.itemMeta}>{item.summary}</div>
              </div>
            ))}
          </div>
        </section>
      </div>
    </ClientShell>
  );
}