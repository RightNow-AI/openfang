"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getModeApprovals, approveModeTask } from "../../../../lib/mode-api";
import type { ModeApproval } from "../../../../lib/mode-types";

type Props = { params: Promise<{ clientId: string }> };

const APPROVAL_LABELS: Record<string, string> = {
  draft_approval:                   "Draft Review",
  send_approval:                    "Send",
  tool_use_approval:                "Tool Use",
  publish_approval:                 "Publish",
  client_delivery_approval:         "Client Delivery",
  student_facing_content_approval:  "Student Content",
  spend_approval:                   "Spend",
};

export default function AgencyApprovalsPage({ params }: Props) {
  const [clientId, setClientId]     = useState("");
  const [approvals, setApprovals]   = useState<ModeApproval[]>([]);
  const [error, setError]           = useState("");

  const reload = (cid: string) =>
    getModeApprovals("agency", cid)
      .then((r) => setApprovals(r.approvals))
      .catch((e: Error) => setError(e.message));

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      reload(cid);
    });
  }, [params]);

  async function handleApprove(taskId: string) {
    await approveModeTask("agency", taskId);
    reload(clientId);
  }

  const pending   = approvals.filter((a) => a.status === "pending");
  const resolved  = approvals.filter((a) => a.status !== "pending");

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 860, margin: "0 auto" }}>
      <div style={{ marginBottom: 20 }}>
        <Link href={`/agency/${clientId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Overview</Link>
        <h1 style={{ fontSize: 22, fontWeight: 700, marginTop: 8 }}>Approvals</h1>
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>Review and approve pending tasks before they run.</p>
      </div>

      {pending.length === 0 && (
        <div style={{ padding: "24px", textAlign: "center", color: "var(--text-muted, #888)", fontSize: 14, border: "1px dashed var(--border)", borderRadius: 8, marginBottom: 24 }}>
          No pending approvals 🎉
        </div>
      )}

      {pending.map((a) => (
        <div key={a.id} style={{ border: "1px solid #eab308", borderRadius: 8, padding: "14px 16px", marginBottom: 10 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
            <div>
              <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: "rgba(234,179,8,0.2)", color: "#eab308", marginBottom: 6, display: "inline-block" }}>
                {APPROVAL_LABELS[a.approval_type] ?? a.approval_type}
              </span>
              <div style={{ fontWeight: 600, fontSize: 15, marginBottom: 4 }}>{a.preview_summary}</div>
              <div style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>Requested by: {a.requested_by}</div>
              {a.tool_actions.length > 0 && (
                <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 4 }}>
                  Tools: {a.tool_actions.join(", ")}
                </div>
              )}
            </div>
            <button
              onClick={() => handleApprove(a.task_id)}
              style={{ padding: "6px 16px", border: "none", background: "var(--accent)", color: "#fff", borderRadius: 6, cursor: "pointer", fontWeight: 600, whiteSpace: "nowrap" }}
            >
              Approve
            </button>
          </div>
        </div>
      ))}

      {resolved.length > 0 && (
        <div style={{ marginTop: 24 }}>
          <h2 style={{ fontSize: 14, fontWeight: 600, color: "var(--text-muted, #888)", marginBottom: 10 }}>Resolved</h2>
          {resolved.map((a) => (
            <div key={a.id} style={{ border: "1px solid var(--border)", borderRadius: 8, padding: "10px 16px", marginBottom: 8, opacity: 0.6 }}>
              <div style={{ fontSize: 14 }}>{a.preview_summary}</div>
              <div style={{ fontSize: 12, color: "#22c55e", marginTop: 2 }}>✓ {a.status}</div>
            </div>
          ))}
        </div>
      )}
    </main>
  );
}
