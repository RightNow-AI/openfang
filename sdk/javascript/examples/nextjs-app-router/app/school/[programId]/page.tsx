"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { getRecord, getModeTasks, getModeResults } from "../../../lib/mode-api";
import type { ModeRecord, ModeTask } from "../../../lib/mode-types";

const STATUS_COLOR: Record<string, string> = {
  pending:   "var(--text-muted, #888)",
  running:   "#3b82f6",
  completed: "#22c55e",
  failed:    "#ef4444",
  blocked:   "#eab308",
};

export default function SchoolProgramPage() {
  const { programId } = useParams<{ programId: string }>();
  const [record, setRecord] = useState<ModeRecord | null>(null);
  const [tasks,  setTasks]  = useState<ModeTask[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!programId) return;
    Promise.all([
      getRecord("school", programId),
      getModeTasks("school", programId),
    ]).then(([r, t]) => {
      setRecord(r.record);
      setTasks(t.tasks);
      setLoading(false);
    });
  }, [programId]);

  if (loading) return <div style={{ padding: 32, color: "var(--text-muted, #888)", fontSize: 14 }}>Loading…</div>;
  if (!record)  return <div style={{ padding: 32, color: "#ef4444", fontSize: 14 }}>Program not found.</div>;

  const done  = tasks.filter((t) => t.status === "completed").length;
  const total = tasks.length;
  const pct   = total > 0 ? Math.round((done / total) * 100) : 0;
  const meta  = (record.meta ?? {}) as Record<string, unknown>;
  const hasHealthTracking = Boolean(meta.enable_student_health_tracking);

  return (
    <div style={{ padding: 32, maxWidth: 900, margin: "0 auto" }}>
      <div style={{ marginBottom: 4 }}>
        <Link href="/school/new" style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← New Program</Link>
      </div>
      <h1 style={{ fontSize: 24, fontWeight: 700, marginBottom: 4 }}>{record.title}</h1>
      {record.subtitle && <div style={{ fontSize: 14, color: "var(--text-muted, #888)", marginBottom: 4 }}>{record.subtitle}</div>}
      {record.goal && (
        <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 20 }}>
          <strong>Goal:</strong> {record.goal}
        </div>
      )}

      {/* Progress */}
      <div style={{ marginBottom: 24 }}>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: "var(--text-muted, #888)", marginBottom: 4 }}>
          <span>Progress</span>
          <span>{done} / {total} tasks</span>
        </div>
        <div style={{ height: 7, background: "var(--border)", borderRadius: 999, overflow: "hidden" }}>
          <div style={{ height: "100%", background: "var(--accent)", borderRadius: 999, width: `${pct}%`, transition: "width 0.4s" }} />
        </div>
        <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 4 }}>{pct}% complete</div>
      </div>

      {/* Nav actions */}
      <div style={{ display: "flex", gap: 10, marginBottom: 28, flexWrap: "wrap" }}>
        {hasHealthTracking && (
          <Link href={`/school/${programId}/cohort`}>
            <button style={{ padding: "8px 16px", borderRadius: 6, background: "var(--accent)", color: "#fff", border: "none", cursor: "pointer", fontSize: 13, fontWeight: 600 }}>
              🎓 Cohort Health
            </button>
          </Link>
        )}
        <Link href={`/school/${programId}/approvals`}>
          <button style={{ padding: "8px 16px", borderRadius: 6, background: "transparent", color: "var(--accent)", border: "1px solid var(--accent)", cursor: "pointer", fontSize: 13 }}>
            Approvals
          </button>
        </Link>
        <Link href={`/school/${programId}/results`}>
          <button style={{ padding: "8px 16px", borderRadius: 6, background: "transparent", color: "var(--accent)", border: "1px solid var(--accent)", cursor: "pointer", fontSize: 13 }}>
            Results
          </button>
        </Link>
      </div>

      {/* Task board */}
      <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>Task Plan</h2>
      {tasks.length === 0 ? (
        <div style={{ padding: 20, border: "1px dashed var(--border)", borderRadius: 8, textAlign: "center", color: "var(--text-muted, #888)", fontSize: 14 }}>
          No tasks yet. Generate a plan to get started.
        </div>
      ) : (
        <div>
          {tasks.map((t) => (
            <div key={t.id} style={{ display: "flex", alignItems: "flex-start", gap: 12, padding: "12px 14px", border: "1px solid var(--border)", borderRadius: 8, marginBottom: 8 }}>
              <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: `${STATUS_COLOR[t.status] ?? "#888"}22`, color: STATUS_COLOR[t.status] ?? "#888", fontWeight: 600, whiteSpace: "nowrap", marginTop: 1 }}>
                {t.status}
              </span>
              <div>
                <div style={{ fontSize: 14, fontWeight: 500 }}>{t.title}</div>
                {t.output_summary && <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 2 }}>{t.output_summary}</div>}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
