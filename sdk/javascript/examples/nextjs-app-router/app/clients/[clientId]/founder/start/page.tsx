"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "../../components/ClientShell";
import FounderStartWizard from "../../components/FounderStartWizard";
import { getClientHome } from "../../lib/client-api";
import type { ClientHomeResponse } from "../../lib/client-types";
import styles from "../../../client-dashboard.module.css";

export default function FounderStartPage() {
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
      currentPage="founder"
      approvalsWaiting={data.client.approvals_waiting}
      tasksDueToday={data.client.tasks_due_today}
      lastActivityAt={data.client.last_activity_at}
      health={data.client.health}
    >
      <div className={styles.dashboardGrid}>
        <FounderStartWizard
          clientId={clientId}
          clientName={data.client.name}
          initialIdea={data.client.main_goal}
          founderWorkspace={data.founder_workspace}
        />
      </div>
    </ClientShell>
  );
}