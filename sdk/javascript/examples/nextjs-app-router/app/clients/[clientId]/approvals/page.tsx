"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "../components/ClientShell";
import styles from "../../client-dashboard.module.css";
import { getClientApprovals, getClientHome } from "../lib/client-api";
import {
  approveClientTask,
  draftClientUpdate,
  rejectClientTask,
  requestTaskChanges,
  runApprovedClientTask,
} from "../lib/client-actions";
import type { ApprovalItem, ClientApprovalsResponse, ClientHomeResponse } from "../lib/client-types";

function cloneApprovals(data: ClientApprovalsResponse): ClientApprovalsResponse {
  return {
    ...data,
    needs_review: data.needs_review.map((item) => ({ ...item, tools_involved: [...item.tools_involved] })),
    approved: data.approved.map((item) => ({ ...item, tools_involved: [...item.tools_involved] })),
    rejected: data.rejected.map((item) => ({ ...item, tools_involved: [...item.tools_involved] })),
    changes_requested: data.changes_requested.map((item) => ({ ...item, tools_involved: [...item.tools_involved] })),
    execution_queue: data.execution_queue.map((item) => ({ ...item })),
    approval_rules: data.approval_rules.map((rule) => ({ ...rule })),
  };
}

function updateApprovalBuckets(
  data: ClientApprovalsResponse,
  taskId: string,
  target: "approved" | "rejected" | "changes_requested",
) {
  const next = cloneApprovals(data);
  const item = next.needs_review.find((entry) => entry.linked_task_id === taskId);
  if (!item) return data;

  next.needs_review = next.needs_review.filter((entry) => entry.linked_task_id !== taskId);
  const updated = { ...item, status: target };

  if (target === "approved") {
    next.approved = [...next.approved, updated];
    if (!next.execution_queue.some((entry) => entry.id === taskId)) {
      next.execution_queue = [
        ...next.execution_queue,
        { id: taskId, title: item.title, status: "ready", source_approval_id: item.id },
      ];
    }
  }

  if (target === "rejected") {
    next.rejected = [...next.rejected, updated];
    next.execution_queue = next.execution_queue.filter((entry) => entry.id !== taskId);
  }

  if (target === "changes_requested") {
    next.changes_requested = [...next.changes_requested, updated];
    next.execution_queue = next.execution_queue.filter((entry) => entry.id !== taskId);
  }

  return next;
}

function updateExecutionStatus(
  data: ClientApprovalsResponse,
  taskId: string,
  status: ClientApprovalsResponse["execution_queue"][number]["status"],
) {
  const next = cloneApprovals(data);
  next.execution_queue = next.execution_queue.map((item) => (
    item.id === taskId ? { ...item, status } : item
  ));
  return next;
}

function ApprovalBucket({
  title,
  items,
  pendingTaskId,
  onApprove,
  onReject,
  onRequestChanges,
}: {
  title: string;
  items: ApprovalItem[];
  pendingTaskId: string | null;
  onApprove: (taskId: string) => void;
  onReject: (taskId: string) => void;
  onRequestChanges: (taskId: string) => void;
}) {
  return (
    <div className={styles.boardColumn}>
      <div className={styles.boardColumnTitle}>{title}</div>
      {items.length === 0 ? (
        <div className={styles.emptyText}>Nothing here.</div>
      ) : (
        <div className={styles.stack}>
          {items.map((item) => (
            <div key={item.id} className={styles.itemCard} data-cy={`approval-card-${item.linked_task_id}`}>
              <div className={styles.itemTitle}>{item.title}</div>
              <div className={styles.itemMeta}>{item.requested_by}</div>
              <div className={item.status === "needs_review" ? `${styles.fieldLabel} ${styles.itemMetaWarn}` : styles.fieldLabel}>
                {item.approval_type.replace(/_/g, " ")}
              </div>
              {item.status === "needs_review" ? (
                <div className={styles.cardActions}>
                  <div className={styles.actionRow}>
                    <button className={`${styles.button} ${styles.buttonPrimary}`} onClick={() => onApprove(item.linked_task_id)} disabled={pendingTaskId === item.linked_task_id} data-cy={`approval-task-${item.linked_task_id}-approve`}>
                      Approve
                    </button>
                    <button className={`${styles.button} ${styles.buttonDanger}`} onClick={() => onReject(item.linked_task_id)} disabled={pendingTaskId === item.linked_task_id} data-cy={`approval-task-${item.linked_task_id}-reject`}>
                      Reject
                    </button>
                  </div>
                  <button className={`${styles.button} ${styles.buttonWarn}`} onClick={() => onRequestChanges(item.linked_task_id)} disabled={pendingTaskId === item.linked_task_id} data-cy={`approval-task-${item.linked_task_id}-request-changes`}>
                    Request changes
                  </button>
                </div>
              ) : null}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default function ClientApprovalsPage() {
  const params = useParams<{ clientId: string }>();
  const clientId = Array.isArray(params?.clientId) ? params.clientId[0] || "" : params?.clientId || "";
  const [home, setHome] = useState<ClientHomeResponse | null>(null);
  const [approvals, setApprovals] = useState<ClientApprovalsResponse | null>(null);
  const [error, setError] = useState("");
  const [pendingTaskId, setPendingTaskId] = useState<string | null>(null);
  const [actionError, setActionError] = useState("");
  const [draft, setDraft] = useState<{ title: string; markdown: string } | null>(null);

  async function loadDashboard(id: string) {
    const [homeData, approvalsData] = await Promise.all([getClientHome(id), getClientApprovals(id)]);
    setHome(homeData);
    setApprovals(approvalsData);
  }

  useEffect(() => {
    if (!clientId) return;

    loadDashboard(clientId)
      .catch((event: Error) => setError(event.message));
  }, [clientId]);

  async function mutateTask(
    taskId: string,
    optimistic: (current: ClientApprovalsResponse) => ClientApprovalsResponse,
    operation: () => Promise<unknown>,
    commit?: (current: ClientApprovalsResponse) => ClientApprovalsResponse,
  ) {
    const snapshot = approvals;
    if (!snapshot) return;

    try {
      setActionError("");
      setPendingTaskId(taskId);
      setApprovals((current) => (current ? optimistic(current) : current));
      await operation();
      if (commit) {
        setApprovals((current) => (current ? commit(current) : current));
      }
      void loadDashboard(clientId).catch(() => undefined);
    } catch (event) {
      setApprovals(snapshot);
      setActionError(event instanceof Error ? event.message : "Approval action failed");
    } finally {
      setPendingTaskId(null);
    }
  }

  async function handleDraftUpdate() {
    try {
      setActionError("");
      setDraft(await draftClientUpdate(clientId));
    } catch (event) {
      setActionError(event instanceof Error ? event.message : "Draft update failed");
    }
  }

  if (error) return <main className={`${styles.statusPage} ${styles.errorText}`}>Error: {error}</main>;
  if (!home || !approvals) return <main className={styles.statusPage}>Loading…</main>;

  const shellApprovalsWaiting = approvals.needs_review.length + approvals.changes_requested.length;
  const shellTasksDueToday = approvals.execution_queue.filter((item) => item.status === "ready" || item.status === "running").length;

  return (
    <ClientShell
      clientId={clientId}
      clientName={home.client.name}
      currentPage="approvals"
      approvalsWaiting={shellApprovalsWaiting}
      tasksDueToday={shellTasksDueToday}
      lastActivityAt={home.client.last_activity_at}
      health={home.client.health}
    >
      <div className={styles.dashboardGrid} data-cy="client-approvals-page">
        <section className={`${styles.card} ${styles.span8}`}>
          <div className={styles.sectionTitle}>Approval queue board</div>
          <div className={styles.approvalGrid}>
            <ApprovalBucket
              title="Needs review"
              items={approvals.needs_review}
              pendingTaskId={pendingTaskId}
              onApprove={(taskId) => mutateTask(taskId, (current) => updateApprovalBuckets(current, taskId, "approved"), () => approveClientTask(taskId))}
              onReject={(taskId) => mutateTask(taskId, (current) => updateApprovalBuckets(current, taskId, "rejected"), () => rejectClientTask(taskId))}
              onRequestChanges={(taskId) => mutateTask(taskId, (current) => updateApprovalBuckets(current, taskId, "changes_requested"), () => requestTaskChanges(taskId))}
            />
            <ApprovalBucket title="Approved" items={approvals.approved} pendingTaskId={pendingTaskId} onApprove={() => {}} onReject={() => {}} onRequestChanges={() => {}} />
            <ApprovalBucket title="Rejected" items={approvals.rejected} pendingTaskId={pendingTaskId} onApprove={() => {}} onReject={() => {}} onRequestChanges={() => {}} />
            <ApprovalBucket title="Changes requested" items={approvals.changes_requested} pendingTaskId={pendingTaskId} onApprove={() => {}} onReject={() => {}} onRequestChanges={() => {}} />
          </div>
        </section>

        <section className={`${styles.card} ${styles.span4}`}>
          <div className={styles.sectionTitle}>Approval rules</div>
          <div className={styles.rulesList}>
            {approvals.approval_rules.map((rule) => (
              <div key={rule.key} className={styles.ruleRow}>
                <span>{rule.key.replace(/_/g, " ")}</span>
                <span className={rule.enabled ? styles.itemMetaWarn : styles.mutedText}>{rule.enabled ? "On" : "Off"}</span>
              </div>
            ))}
          </div>
          <div className={styles.draftBlock}>
            <div className={styles.fieldLabel}>Client update</div>
            <button className={`${styles.button} ${styles.buttonPrimary}`} onClick={handleDraftUpdate}>
              Draft client update
            </button>
            {actionError ? <div className={styles.errorText}>{actionError}</div> : null}
          </div>
        </section>

        <section className={`${styles.card} ${styles.span12}`}>
          <div className={styles.sectionTitle}>Execution queue</div>
          {approvals.execution_queue.length === 0 ? (
            <div className={styles.mutedText}>No approved work is queued yet.</div>
          ) : (
            <div className={styles.executionGrid}>
              {approvals.execution_queue.map((item) => (
                <div key={item.id} className={styles.itemCard} data-cy={`execution-card-${item.id}`}>
                  <div className={styles.itemTitle}>{item.title}</div>
                  <div className={styles.itemMeta}>{item.status}</div>
                  <div className={styles.fieldLabel}>Source approval: {item.source_approval_id || "direct"}</div>
                  {(item.status === "ready" || item.status === "running") ? (
                    <button className={`${styles.button} ${styles.buttonPrimary}`} onClick={() => mutateTask(item.id, (current) => updateExecutionStatus(current, item.id, "running"), () => runApprovedClientTask(item.id), (current) => updateExecutionStatus(current, item.id, "completed"))} disabled={pendingTaskId === item.id} data-cy={`execution-task-${item.id}-run`}>
                      Run approved work
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          )}
        </section>

        {draft ? (
          <section className={`${styles.card} ${styles.span12}`}>
            <div className={styles.sectionTitle}>Drafted client update</div>
            <pre className={styles.draftBlock}>{draft.markdown}</pre>
          </section>
        ) : null}
      </div>
    </ClientShell>
  );
}