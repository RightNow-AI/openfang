"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getRecord, getModeTasks } from "../../../lib/mode-api";
import type { ModeRecord, ModeTask } from "../../../lib/mode-types";

type Props = { params: Promise<{ clientId: string }> };

const STATUS_COLOR: Record<string, string> = {
  completed: "#22c55e", running: "#3b82f6", approved: "#a78bfa",
  pending_approval: "#eab308", draft: "var(--text-muted, #888)",
};

export default function AgencyOverviewPage({ params }: Props) {
  const [clientId, setClientId] = useState("");
  const [record, setRecord]     = useState<ModeRecord | null>(null);
  const [tasks, setTasks]       = useState<ModeTask[]>([]);
  const [error, setError]       = useState("");

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      Promise.all([getRecord("agency", cid), getModeTasks("agency", cid)])
        .then(([r, t]) => {
          setRecord(r.record);
          setTasks(t.tasks);
        })
        .catch((e: Error) => setError(e.message));
    });
  }, [params]);

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;
  if (!record) return <main style={{ padding: 24 }}>Loading…</main>;

  const done  = tasks.filter((t) => t.status === "completed").length;
  const total = tasks.length;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <div style={{ marginBottom: 20 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 4 }}>
          <Link href="/agency/new" style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Agency</Link>
          <span style={{ color: "var(--text-muted, #555)" }}>/</span>
          <span style={{ fontSize: 13 }}>{record.title}</span>
        </div>
        <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 4 }}>{record.title}</h1>
        <p style={{ color: "var(--text-muted, #888)" }}>{record.goal}</p>
      </div>

      {/* Progress bar */}
      {total > 0 && (
        <div style={{ marginBottom: 24 }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13, marginBottom: 6 }}>
            <span style={{ color: "var(--text-muted, #888)" }}>Progress</span>
            <span>{done}/{total} tasks done</span>
          </div>
          <div style={{ height: 6, background: "var(--border, #333)", borderRadius: 999 }}>
            <div style={{ height: "100%", background: "var(--accent, #7c3aed)", borderRadius: 999, width: `${(done / total) * 100}%`, transition: "width 0.3s" }} />
          </div>
        </div>
      )}

      {/* Action nav */}
      <div style={{ display: "flex", gap: 10, marginBottom: 28, flexWrap: "wrap" }}>
        <Link href={`/agency/${clientId}/approvals`}
          style={{ padding: "6px 16px", border: "1px solid var(--border, #333)", borderRadius: 6, fontSize: 14 }}>
          Approvals
        </Link>
        <Link href={`/agency/${clientId}/results`}
          style={{ padding: "6px 16px", border: "1px solid var(--border, #333)", borderRadius: 6, fontSize: 14 }}>
          Results
        </Link>
      </div>

      {/* Task board */}
      {tasks.length === 0 ? (
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>No tasks yet. Run the wizard to generate a plan.</p>
      ) : (
        <div>
          <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>Tasks</h2>
          {tasks.map((t) => (
            <div key={t.id} style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: "12px 16px", marginBottom: 8 }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                <div>
                  <div style={{ fontWeight: 600, fontSize: 14 }}>{t.title}</div>
                  <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 2 }}>{t.assigned_agent}</div>
                </div>
                <span style={{ fontSize: 12, padding: "2px 10px", borderRadius: 999, background: "rgba(0,0,0,0.3)", color: STATUS_COLOR[t.status] ?? "#888" }}>
                  {t.status.replace(/_/g, " ")}
                </span>
              </div>
              {t.approval_required && t.approval_type && (
                <div style={{ fontSize: 12, color: "#eab308", marginTop: 6 }}>
                  Requires {t.approval_type.replace(/_/g, " ")}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </main>
  );
}
