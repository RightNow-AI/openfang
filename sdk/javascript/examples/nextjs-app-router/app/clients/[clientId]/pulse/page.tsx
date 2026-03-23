"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "../components/ClientShell";
import { getClientHome, getClientPulse } from "../lib/client-api";
import type { ClientHomeResponse, ClientPulseResponse } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

export default function ClientPulsePage() {
  const params = useParams<{ clientId: string }>();
  const clientId = Array.isArray(params?.clientId) ? params.clientId[0] || "" : params?.clientId || "";
  const [home, setHome] = useState<ClientHomeResponse | null>(null);
  const [pulse, setPulse] = useState<ClientPulseResponse | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!clientId) return;

    Promise.all([getClientHome(clientId), getClientPulse(clientId)])
      .then(([homeData, pulseData]) => {
        setHome(homeData);
        setPulse(pulseData);
      })
      .catch((event: Error) => setError(event.message));
  }, [clientId]);

  if (error) return <main className={`${styles.statusPage} ${styles.errorText}`}>Error: {error}</main>;
  if (!home || !pulse) return <main className={styles.statusPage}>Loading…</main>;

  return (
    <ClientShell
      clientId={clientId}
      clientName={home.client.name}
      currentPage="pulse"
      approvalsWaiting={home.client.approvals_waiting}
      tasksDueToday={home.client.tasks_due_today}
      lastActivityAt={home.client.last_activity_at}
      health={home.client.health}
    >
      <div className={styles.dashboardGrid}>
        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Business snapshot</div>
          <div className={styles.stackTight}>
            <div><strong>Offer:</strong> {pulse.business_snapshot.offer || "Not set"}</div>
            <div><strong>Audience:</strong> {pulse.business_snapshot.audience || "Not set"}</div>
            <div><strong>Positioning:</strong> {pulse.business_snapshot.positioning || "Not set"}</div>
            <div><strong>Current objective:</strong> {pulse.business_snapshot.current_objective || "Not set"}</div>
          </div>
        </section>

        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Brand voice and memory</div>
          <div className={styles.sectionLead}>{pulse.brand_voice.summary}</div>
          <div className={styles.stackTight}>
            {pulse.memory_facts.map((fact) => (
              <div key={fact.id} className={styles.itemCard}>
                <div className={styles.fieldLabel}>{fact.label}</div>
                <div>{fact.value}</div>
              </div>
            ))}
          </div>
        </section>

        <section className={`${styles.card} ${styles.span5}`}>
          <div className={styles.sectionTitle}>Competitor and market signal</div>
          {pulse.competitor_signals.length === 0 ? (
            <div className={styles.mutedText}>No competitor signals captured yet.</div>
          ) : (
            pulse.competitor_signals.map((signal) => (
              <div key={signal.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{signal.competitor_name}</div>
                <div className={styles.itemMeta}>{signal.change_summary}</div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span3}`}>
          <div className={styles.sectionTitle}>Missing info</div>
          {pulse.missing_info.length === 0 ? (
            <div className={styles.mutedText}>No missing inputs flagged.</div>
          ) : (
            pulse.missing_info.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.question}</div>
                <div className={styles.fieldLabel}>{item.owner_label}</div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Risks and opportunities</div>
          {pulse.risks_and_opportunities.length === 0 ? (
            <div className={styles.mutedText}>No current flags.</div>
          ) : (
            pulse.risks_and_opportunities.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.title}</div>
                <div className={styles.itemMeta}>{item.description}</div>
                {item.suggested_next_step ? <div className={styles.fieldLabel}>{item.suggested_next_step}</div> : null}
              </div>
            ))
          )}
        </section>
      </div>
    </ClientShell>
  );
}