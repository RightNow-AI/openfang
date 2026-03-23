"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "../components/ClientShell";
import FounderTasks from "../components/FounderTasks";
import { getClientHome, getClientResults } from "../lib/client-api";
import type { ClientHomeResponse, ClientResultsResponse } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

export default function ClientResultsPage() {
  const params = useParams<{ clientId: string }>();
  const clientId = Array.isArray(params?.clientId) ? params.clientId[0] || "" : params?.clientId || "";
  const [home, setHome] = useState<ClientHomeResponse | null>(null);
  const [results, setResults] = useState<ClientResultsResponse | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!clientId) return;

    Promise.all([getClientHome(clientId), getClientResults(clientId)])
      .then(([homeData, resultsData]) => {
        setHome(homeData);
        setResults(resultsData);
      })
      .catch((event: Error) => setError(event.message));
  }, [clientId]);

  if (error) return <main className={`${styles.statusPage} ${styles.errorText}`}>Error: {error}</main>;
  if (!home || !results) return <main className={styles.statusPage}>Loading…</main>;

  const founderWorkspace = results.founder_workspace;

  return (
    <ClientShell
      clientId={clientId}
      clientName={home.client.name}
      currentPage="results"
      approvalsWaiting={home.client.approvals_waiting}
      tasksDueToday={home.client.tasks_due_today}
      lastActivityAt={home.client.last_activity_at}
      health={home.client.health}
    >
      <div className={styles.dashboardGrid} data-cy="client-results-page">
        <section className={`${styles.card} ${styles.span7}`}>
          {founderWorkspace && (
            <>
              <div className={styles.sectionTitle}>Founder run history</div>
              {results.founder_runs.length === 0 ? (
                <div className={styles.mutedText}>No founder runs saved for this workspace yet.</div>
              ) : (
                <div className={styles.stack}>
                  {results.founder_runs.map((run) => (
                    <a
                      key={run.runId}
                      href={`/deep-research?${new URLSearchParams({
                        clientId,
                        clientName: home.client.name,
                        workspaceId: founderWorkspace.workspaceId,
                        runId: run.runId,
                        ...(run.playbookId ? { playbookId: run.playbookId } : {}),
                      }).toString()}`}
                      className={styles.linkButton}
                    >
                      <span>{run.playbookId || 'freeform'} · {run.status}</span>
                      <span>{run.summary || run.prompt}</span>
                    </a>
                  ))}
                </div>
              )}
            </>
          )}
        </section>

        {founderWorkspace ? (
          <FounderTasks workspaceId={founderWorkspace.workspaceId} />
        ) : null}

        <section className={`${styles.card} ${styles.span5}`}>
          <div className={styles.sectionTitle}>Delivered outputs</div>
          {results.delivered_outputs.length === 0 ? (
            <div className={styles.mutedText}>No delivered outputs yet.</div>
          ) : (
            results.delivered_outputs.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.title}</div>
                <div className={styles.itemMeta}>{item.type} · {item.status}</div>
                <div className={styles.bodyText}>{item.summary}</div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span5}`}>
          <div className={styles.sectionTitle}>Performance summary</div>
          {results.performance_summary.metrics.map((metric) => (
            <div key={metric.label} className={styles.metricRow}>
              <span>{metric.label}</span>
              <strong>{metric.value}</strong>
            </div>
          ))}
          <div className={styles.metricBlock}>
            <div className={styles.fieldLabel}>Weekly review</div>
            <div className={styles.bodyText}>{results.weekly_review.summary}</div>
          </div>
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Lessons learned</div>
          {results.lessons_learned.length === 0 ? (
            <div className={styles.mutedText}>No lessons captured yet.</div>
          ) : (
            results.lessons_learned.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.type}</div>
                <div className={styles.itemMeta}>{item.text}</div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Next best actions</div>
          {results.next_best_actions.length === 0 ? (
            <div className={styles.mutedText}>No follow-up actions suggested yet.</div>
          ) : (
            results.next_best_actions.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.title}</div>
                <div className={styles.itemMeta}>{item.reason}</div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Case study candidates</div>
          {results.case_study_candidates.length === 0 ? (
            <div className={styles.mutedText}>No case study candidates yet.</div>
          ) : (
            results.case_study_candidates.map((item) => (
              <div key={item.id} className={styles.itemCard}>
                <div className={styles.itemTitle}>{item.title}</div>
                <div className={styles.itemMeta}>{item.proof_point}</div>
              </div>
            ))
          )}
        </section>
      </div>
    </ClientShell>
  );
}