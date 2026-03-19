"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getClient, getTasks } from "../../../lib/command-center-api";
import type { ClientProfile, PlannedTask } from "../../../lib/command-center-types";
import TaskBoard from "../components/TaskBoard";

type Props = {
  params: Promise<{ clientId: string }>;
};

export default function CommandCenterOverviewPage({ params }: Props) {
  const [clientId, setClientId] = useState("");
  const [client, setClient] = useState<ClientProfile | null>(null);
  const [tasks, setTasks] = useState<PlannedTask[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      Promise.all([getClient(cid), getTasks(cid)])
        .then(([c, t]) => {
          setClient(c.client);
          setTasks(t.tasks);
        })
        .catch((e: Error) => setError(e.message));
    });
  }, [params]);

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;
  if (!client) return <main style={{ padding: 24 }}>Loading…</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <div style={{ marginBottom: 20 }}>
        <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 4 }}>{client.business_name}</h1>
        <p style={{ color: "var(--text-muted, #888)" }}>{client.main_goal}</p>
      </div>

      <div style={{ display: "flex", gap: 12, marginBottom: 24 }}>
        <Link href={`/command-center/${clientId}/wizard`}
          style={{ padding: "6px 16px", border: "1px solid var(--border)", borderRadius: 6, fontSize: 14 }}>
          Open wizard
        </Link>
        <Link href={`/command-center/${clientId}/approvals`}
          style={{ padding: "6px 16px", border: "1px solid var(--border)", borderRadius: 6, fontSize: 14 }}>
          Approvals
        </Link>
        <Link href={`/command-center/${clientId}/results`}
          style={{ padding: "6px 16px", border: "1px solid var(--border)", borderRadius: 6, fontSize: 14 }}>
          Results
        </Link>
      </div>

      <TaskBoard
        tasks={tasks}
        onApproveTask={async () => {
          const t = await getTasks(clientId);
          setTasks(t.tasks);
        }}
        onRunTask={async () => {
          const t = await getTasks(clientId);
          setTasks(t.tasks);
        }}
      />
    </main>
  );
}
