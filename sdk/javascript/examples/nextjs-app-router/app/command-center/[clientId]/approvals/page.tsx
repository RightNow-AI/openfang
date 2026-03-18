"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getApprovals, approveTask } from "../../../lib/command-center-api";
import type { ApprovalItem } from "../../../lib/command-center-types";
import ApprovalQueue from "../components/ApprovalQueue";

type Props = {
  params: Promise<{ clientId: string }>;
};

export default function ApprovalsPage({ params }: Props) {
  const [clientId, setClientId] = useState("");
  const [approvals, setApprovals] = useState<ApprovalItem[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      getApprovals(cid)
        .then((data) => setApprovals(data.approvals))
        .catch((e: Error) => setError(e.message));
    });
  }, [params]);

  async function handleApprove(taskId: string) {
    await approveTask(taskId);
    const refreshed = await getApprovals(clientId);
    setApprovals(refreshed.approvals);
  }

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 16, marginBottom: 24 }}>
        <h1 style={{ fontSize: 22, fontWeight: 700 }}>Approvals</h1>
        {clientId && (
          <Link href={`/command-center/${clientId}`}
            style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>
            ← Back to overview
          </Link>
        )}
      </div>
      <ApprovalQueue approvals={approvals} onApprove={handleApprove} />
    </main>
  );
}
