import type { PlannedTask } from "../../../lib/command-center-types";
import { approveTask, runTask } from "../../../lib/command-center-api";

const STATUS_COLOR: Record<string, string> = {
  draft:            "var(--text-muted, #888)",
  pending_approval: "var(--warning, #f59e0b)",
  approved:         "var(--success, #22c55e)",
  running:          "var(--accent, #7c6af7)",
  completed:        "var(--success, #22c55e)",
  failed:           "#ef4444",
};

type Props = {
  tasks: PlannedTask[];
  onApproveTask: (taskId: string) => Promise<void>;
  onRunTask: (taskId: string) => Promise<void>;
};

export default function TaskBoard({ tasks, onApproveTask, onRunTask }: Props) {
  if (tasks.length === 0)
    return <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>No tasks yet.</p>;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {tasks.map((task) => (
        <div
          key={task.id}
          style={{
            padding: "16px 18px",
            border: "1px solid var(--border)",
            borderRadius: 8,
            display: "flex",
            justifyContent: "space-between",
            alignItems: "flex-start",
            gap: 16,
          }}
        >
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 600, marginBottom: 4 }}>{task.title}</div>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>
              Agent: {task.assigned_agent}
              {task.required_tools.length > 0 && ` · Tools: ${task.required_tools.join(", ")}`}
            </div>
            <div style={{ fontSize: 12 }}>
              <span style={{ color: STATUS_COLOR[task.status] ?? "#888", fontWeight: 500 }}>
                {task.status.replace(/_/g, " ")}
              </span>
              {task.approval_required && (
                <span style={{ marginLeft: 8, color: "var(--text-muted, #888)" }}>
                  · approval: {task.approval_status}
                </span>
              )}
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
            {task.approval_status === "pending" && (
              <button
                onClick={() => approveTask(task.id).then(() => onApproveTask(task.id))}
                style={{
                  padding: "5px 12px",
                  background: "var(--success, #22c55e)",
                  color: "#fff",
                  border: "none",
                  borderRadius: 5,
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "pointer",
                }}
              >
                Approve
              </button>
            )}
            {(task.approval_status === "approved" || task.approval_status === "none") &&
              task.status !== "completed" &&
              task.status !== "running" && (
                <button
                  onClick={() => runTask(task.id).then(() => onRunTask(task.id))}
                  style={{
                    padding: "5px 12px",
                    background: "var(--accent, #7c6af7)",
                    color: "#fff",
                    border: "none",
                    borderRadius: 5,
                    fontSize: 12,
                    fontWeight: 600,
                    cursor: "pointer",
                  }}
                >
                  Run
                </button>
              )}
          </div>
        </div>
      ))}
    </div>
  );
}
