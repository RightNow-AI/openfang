import type { ApprovalItem } from "../../../lib/command-center-types";

type Props = {
  approvals: ApprovalItem[];
  onApprove: (taskId: string) => Promise<void>;
};

export default function ApprovalQueue({ approvals, onApprove }: Props) {
  if (approvals.length === 0)
    return <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>No approvals waiting.</p>;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {approvals.map((item) => (
        <div
          key={item.id}
          style={{
            padding: "16px 18px",
            border: `1px solid ${item.status === "pending" ? "var(--warning, #f59e0b)" : "var(--border, #333)"}`,
            borderRadius: 8,
          }}
        >
          <div style={{ fontWeight: 600, marginBottom: 6 }}>{item.preview_summary}</div>
          <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>
            Requested by: {item.requested_by}
          </div>
          {item.tool_actions.length > 0 && (
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 8 }}>
              Tool actions: {item.tool_actions.join(", ")}
            </div>
          )}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: item.status === "pending"
                  ? "var(--warning, #f59e0b)"
                  : item.status === "approved"
                  ? "var(--success, #22c55e)"
                  : "var(--text-muted, #888)",
              }}
            >
              {item.status}
            </span>
            {item.status === "pending" && (
              <>
                <button
                  onClick={() => onApprove(item.task_id)}
                  style={{
                    padding: "5px 14px",
                    background: "var(--success, #22c55e)",
                    color: "#fff",
                    border: "none",
                    borderRadius: 5,
                    fontWeight: 600,
                    fontSize: 13,
                    cursor: "pointer",
                  }}
                >
                  Approve
                </button>
                <button
                  style={{
                    padding: "5px 14px",
                    background: "transparent",
                    color: "#ef4444",
                    border: "1px solid #ef4444",
                    borderRadius: 5,
                    fontSize: 13,
                    cursor: "pointer",
                  }}
                >
                  Reject
                </button>
              </>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
