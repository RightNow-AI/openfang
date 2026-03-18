"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { getModeApprovals, approveModeTask } from "../../../../lib/mode-api";
import type { ModeApproval } from "../../../../lib/mode-types";

const APPROVAL_LABELS: Record<string, string> = {
  draft_approval:                  "Draft Review",
  tool_use_approval:               "Tool Use",
  send_approval:                   "Send / Deliver",
  publish_approval:                "Publish",
  client_delivery_approval:        "Client Delivery",
  student_facing_content_approval: "Student-Facing Content",
  spend_approval:                  "Spend",
};

export default function SchoolApprovalsPage() {
  const { programId } = useParams<{ programId: string }>();
  const [approvals, setApprovals] = useState<ModeApproval[]>([]);
  const [loading, setLoading]     = useState(true);
  const [approving, setApproving] = useState<string | null>(null);

  const load = () => {
    if (!programId) return;
    getModeApprovals("school", programId).then((a) => {
      setApprovals(a.approvals);
      setLoading(false);
    });
  };

  useEffect(load, [programId]);

  const approve = async (approvalId: string) => {
    setApproving(approvalId);
    await approveModeTask("school", approvalId);
    setApproving(null);
    load();
  };

  const pending  = approvals.filter((a) => a.status === "pending");
  const resolved = approvals.filter((a) => a.status !== "pending");

  return (
    <div style={{ padding: 32, maxWidth: 800, margin: "0 auto" }}>
      <div style={{ marginBottom: 12 }}>
        <Link href={`/school/${programId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Program Overview</Link>
      </div>
      <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 20 }}>Approvals</h1>

      {loading ? (
        <div style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>Loading…</div>
      ) : (
        <>
          <h2 style={{ fontSize: 14, fontWeight: 600, marginBottom: 10, color: "var(--text-muted, #888)", textTransform: "uppercase", letterSpacing: "0.05em" }}>
            Pending ({pending.length})
          </h2>
          {pending.length === 0 ? (
            <div style={{ padding: 16, border: "1px dashed var(--border, #333)", borderRadius: 8, color: "var(--text-muted, #888)", fontSize: 14, marginBottom: 24 }}>
              No pending approvals.
            </div>
          ) : (
            <div style={{ marginBottom: 28 }}>
              {pending.map((a) => (
                <div key={a.id} style={{ padding: "14px 16px", border: "1px solid var(--border, #333)", borderRadius: 8, marginBottom: 8, display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 12 }}>
                  <div>
                    <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 2 }}>
                      {APPROVAL_LABELS[a.approval_type] ?? a.approval_type}
                    </div>
                    <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>Task ID: {a.task_id}</div>
                    {a.preview_summary && <div style={{ fontSize: 13, marginTop: 6 }}>{a.preview_summary}</div>}
                  </div>
                  <button
                    onClick={() => approve(a.id)}
                    disabled={approving === a.id}
                    style={{ padding: "6px 14px", borderRadius: 6, background: "var(--accent, #7c3aed)", color: "#fff", border: "none", cursor: approving === a.id ? "not-allowed" : "pointer", fontSize: 13, fontWeight: 600, whiteSpace: "nowrap" }}
                  >
                    {approving === a.id ? "Approving…" : "Approve"}
                  </button>
                </div>
              ))}
            </div>
          )}

          <h2 style={{ fontSize: 14, fontWeight: 600, marginBottom: 10, color: "var(--text-muted, #888)", textTransform: "uppercase", letterSpacing: "0.05em" }}>
            Resolved ({resolved.length})
          </h2>
          {resolved.length === 0 ? (
            <div style={{ padding: 16, border: "1px dashed var(--border, #333)", borderRadius: 8, color: "var(--text-muted, #888)", fontSize: 14 }}>
              No resolved approvals yet.
            </div>
          ) : (
            <div>
              {resolved.map((a) => (
                <div key={a.id} style={{ padding: "12px 16px", border: "1px solid var(--border, #333)", borderRadius: 8, marginBottom: 6, opacity: 0.65 }}>
                  <div style={{ fontSize: 13, fontWeight: 500 }}>{APPROVAL_LABELS[a.approval_type] ?? a.approval_type}</div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 2 }}>
                    {a.status} · {a.created_at ? new Date(a.created_at).toLocaleString() : "–"}
                  </div>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
