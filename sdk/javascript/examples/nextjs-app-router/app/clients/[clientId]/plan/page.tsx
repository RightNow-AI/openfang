"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import ClientShell from "../components/ClientShell";
import styles from "../../client-dashboard.module.css";
import { getClientHome, getClientPlan } from "../lib/client-api";
import {
  approveClientTask,
  assignClientTask,
  draftClientUpdate,
  moveClientTask,
  runApprovedClientTask,
} from "../lib/client-actions";
import type { ClientHomeResponse, ClientPlanResponse, TaskItem } from "../lib/client-types";

const BOARD_TARGETS = ["backlog", "this_week", "today", "waiting", "done"] as const;
const DEFAULT_ASSIGNEES = ["planner_agent", "ops_agent", "writer_agent", "review_agent"];
type PlanColumnKey = typeof BOARD_TARGETS[number];

function clonePlan(plan: ClientPlanResponse): ClientPlanResponse {
  return {
    ...plan,
    board: {
      backlog: plan.board.backlog.map((task) => ({ ...task })),
      this_week: plan.board.this_week.map((task) => ({ ...task })),
      today: plan.board.today.map((task) => ({ ...task })),
      waiting: plan.board.waiting.map((task) => ({ ...task })),
      done: plan.board.done.map((task) => ({ ...task })),
    },
    dependencies: plan.dependencies.map((dependency) => ({ ...dependency, blocked_by_ids: [...dependency.blocked_by_ids], unlocks_ids: [...dependency.unlocks_ids] })),
    capacity: plan.capacity.map((item) => ({ ...item })),
    approval_needed: plan.approval_needed.map((task) => ({ ...task })),
  };
}

function syncApprovalNeeded(plan: ClientPlanResponse) {
  plan.approval_needed = [
    ...plan.board.backlog,
    ...plan.board.this_week,
    ...plan.board.waiting,
  ].filter((task) => task.approval_required);
  return plan;
}

function updateTaskAssignment(plan: ClientPlanResponse, taskId: string, assignee: string) {
  const next = clonePlan(plan);
  for (const column of BOARD_TARGETS) {
    next.board[column] = next.board[column].map((task) => (
      task.id === taskId ? { ...task, owner_label: assignee } : task
    ));
  }
  next.approval_needed = next.approval_needed.map((task) => (
    task.id === taskId ? { ...task, owner_label: assignee } : task
  ));
  return next;
}

function moveTaskBetweenColumns(plan: ClientPlanResponse, taskId: string, target: PlanColumnKey, statusOverride?: TaskItem["status"]) {
  const next = clonePlan(plan);
  let movedTask: TaskItem | null = null;

  for (const column of BOARD_TARGETS) {
    const existing = next.board[column].find((task) => task.id === taskId);
    if (existing) {
      movedTask = { ...existing, status: statusOverride ?? target };
      next.board[column] = next.board[column].filter((task) => task.id !== taskId);
      break;
    }
  }

  if (!movedTask) return plan;

  next.board[target] = [
    ...next.board[target].filter((task) => task.id !== taskId),
    movedTask,
  ];

  return syncApprovalNeeded(next);
}

function markTaskRunning(plan: ClientPlanResponse, taskId: string) {
  const next = clonePlan(plan);
  next.board.today = next.board.today.map((task) => (
    task.id === taskId ? { ...task, status: "running" } : task
  ));
  return next;
}

function TaskColumn({
  columnKey,
  title,
  tasks,
  pendingTaskId,
  moveTargets,
  assignees,
  onApprove,
  onAssign,
  onMove,
  onRun,
}: {
  columnKey: PlanColumnKey;
  title: string;
  tasks: TaskItem[];
  pendingTaskId: string | null;
  moveTargets: readonly string[];
  assignees: string[];
  onApprove: (taskId: string) => void;
  onAssign: (taskId: string, assignee: string) => void;
  onMove: (taskId: string, target: string) => void;
  onRun: (taskId: string) => void;
}) {
  return (
    <div className={styles.boardColumn} data-cy={`plan-column-${columnKey}`}>
      <div className={styles.boardColumnTitle}>{title}</div>
      {tasks.length === 0 ? (
        <div className={styles.emptyText}>No tasks in this column.</div>
      ) : (
        <div className={styles.stack}>
          {tasks.map((task) => (
            <div key={task.id} className={styles.itemCard} data-cy={`task-card-${task.id}`}>
              <div className={styles.itemTitle}>{task.title}</div>
              <div className={styles.itemMeta}>{task.owner_label}</div>
              <div className={styles.badgeRow}>
                <span className={styles.badge}>{task.priority}</span>
                {task.approval_required ? <span className={`${styles.badge} ${styles.badgeWarn}`}>approval</span> : null}
                {task.status === "running" ? <span className={`${styles.badge} ${styles.badgeAccent}`}>running</span> : null}
                {task.status === "failed" ? <span className={`${styles.badge} ${styles.badgeDanger}`}>failed</span> : null}
              </div>
              <div className={styles.cardActions}>
                <div className={styles.actionRow}>
                  <select
                    className={styles.select}
                    defaultValue={task.owner_label}
                    onChange={(event) => onAssign(task.id, event.target.value)}
                    disabled={pendingTaskId === task.id}
                    data-cy={`task-${task.id}-assign`}
                    aria-label={`Assign ${task.title}`}
                  >
                    {assignees.map((assignee) => (
                      <option key={assignee} value={assignee}>{assignee}</option>
                    ))}
                  </select>
                </div>
                <div className={styles.actionRow}>
                  <select
                    className={styles.select}
                    defaultValue={task.status === "running" ? "today" : task.status === "done" ? "done" : columnKey}
                    onChange={(event) => onMove(task.id, event.target.value)}
                    disabled={pendingTaskId === task.id}
                    data-cy={`task-${task.id}-move`}
                    aria-label={`Move ${task.title}`}
                  >
                    {moveTargets.map((target) => (
                      <option key={target} value={target}>{target.replace(/_/g, " ")}</option>
                    ))}
                  </select>
                </div>
                <div className={styles.actionRow}>
                  {task.approval_required && (task.status === "waiting" || task.status === "backlog" || task.status === "this_week") ? (
                    <button className={`${styles.button} ${styles.buttonWarn}`} onClick={() => onApprove(task.id)} disabled={pendingTaskId === task.id} data-cy={`task-${task.id}-approve`}>
                      Approve
                    </button>
                  ) : null}
                  {(task.status === "today" || task.status === "running") ? (
                    <button className={`${styles.button} ${styles.buttonPrimary}`} onClick={() => onRun(task.id)} disabled={pendingTaskId === task.id} data-cy={`task-${task.id}-run`}>
                      Run
                    </button>
                  ) : null}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default function ClientPlanPage() {
  const params = useParams<{ clientId: string }>();
  const clientId = Array.isArray(params?.clientId) ? params.clientId[0] || "" : params?.clientId || "";
  const [home, setHome] = useState<ClientHomeResponse | null>(null);
  const [plan, setPlan] = useState<ClientPlanResponse | null>(null);
  const [error, setError] = useState("");
  const [pendingTaskId, setPendingTaskId] = useState<string | null>(null);
  const [actionError, setActionError] = useState("");
  const [draft, setDraft] = useState<{ title: string; markdown: string } | null>(null);

  async function loadDashboard(id: string) {
    const [homeData, planData] = await Promise.all([getClientHome(id), getClientPlan(id)]);
    setHome(homeData);
    setPlan(planData);
  }

  useEffect(() => {
    if (!clientId) return;

    loadDashboard(clientId)
      .catch((event: Error) => setError(event.message));
  }, [clientId]);

  async function mutateTask(
    taskId: string,
    optimistic: (current: ClientPlanResponse) => ClientPlanResponse,
    operation: () => Promise<unknown>,
    commit?: (current: ClientPlanResponse) => ClientPlanResponse,
  ) {
    const snapshot = plan;
    if (!snapshot) return;

    try {
      setActionError("");
      setPendingTaskId(taskId);
      setPlan((current) => (current ? optimistic(current) : current));
      await operation();
      if (commit) {
        setPlan((current) => (current ? commit(current) : current));
      }
      void loadDashboard(clientId).catch(() => undefined);
    } catch (event) {
      setPlan(snapshot);
      setActionError(event instanceof Error ? event.message : "Task update failed");
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
  if (!home || !plan) return <main className={styles.statusPage}>Loading…</main>;

  const assignees = Array.from(
    new Set([
      ...DEFAULT_ASSIGNEES,
      ...Object.values(plan.board).flat().map((task) => task.owner_label),
    ].filter(Boolean)),
  );

  const shellApprovalsWaiting = plan.board.waiting.filter((task) => task.approval_required).length;
  const shellTasksDueToday = plan.board.today.length;

  return (
    <ClientShell
      clientId={clientId}
      clientName={home.client.name}
      currentPage="plan"
      approvalsWaiting={shellApprovalsWaiting}
      tasksDueToday={shellTasksDueToday}
      lastActivityAt={home.client.last_activity_at}
      health={home.client.health}
    >
      <div className={styles.dashboardGrid} data-cy="client-plan-page">
        <section className={`${styles.card} ${styles.span9}`}>
          <div className={styles.sectionTitle}>Weekly plan board</div>
          <div className={styles.columnsGrid}>
            <TaskColumn
              columnKey="backlog"
              title="Backlog"
              tasks={plan.board.backlog}
              pendingTaskId={pendingTaskId}
              moveTargets={BOARD_TARGETS}
              assignees={assignees}
              onApprove={(taskId) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, "today", "today"), () => approveClientTask(taskId))}
              onAssign={(taskId, assignee) => mutateTask(taskId, (current) => updateTaskAssignment(current, taskId, assignee), () => assignClientTask(taskId, assignee))}
              onMove={(taskId, target) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, target as PlanColumnKey), () => moveClientTask(taskId, target as typeof BOARD_TARGETS[number]))}
              onRun={(taskId) => mutateTask(taskId, (current) => markTaskRunning(current, taskId), () => runApprovedClientTask(taskId), (current) => moveTaskBetweenColumns(current, taskId, "done", "done"))}
            />
            <TaskColumn
              columnKey="this_week"
              title="This week"
              tasks={plan.board.this_week}
              pendingTaskId={pendingTaskId}
              moveTargets={BOARD_TARGETS}
              assignees={assignees}
              onApprove={(taskId) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, "today", "today"), () => approveClientTask(taskId))}
              onAssign={(taskId, assignee) => mutateTask(taskId, (current) => updateTaskAssignment(current, taskId, assignee), () => assignClientTask(taskId, assignee))}
              onMove={(taskId, target) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, target as PlanColumnKey), () => moveClientTask(taskId, target as typeof BOARD_TARGETS[number]))}
              onRun={(taskId) => mutateTask(taskId, (current) => markTaskRunning(current, taskId), () => runApprovedClientTask(taskId), (current) => moveTaskBetweenColumns(current, taskId, "done", "done"))}
            />
            <TaskColumn
              columnKey="today"
              title="Today"
              tasks={plan.board.today}
              pendingTaskId={pendingTaskId}
              moveTargets={BOARD_TARGETS}
              assignees={assignees}
              onApprove={(taskId) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, "today", "today"), () => approveClientTask(taskId))}
              onAssign={(taskId, assignee) => mutateTask(taskId, (current) => updateTaskAssignment(current, taskId, assignee), () => assignClientTask(taskId, assignee))}
              onMove={(taskId, target) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, target as PlanColumnKey), () => moveClientTask(taskId, target as typeof BOARD_TARGETS[number]))}
              onRun={(taskId) => mutateTask(taskId, (current) => markTaskRunning(current, taskId), () => runApprovedClientTask(taskId), (current) => moveTaskBetweenColumns(current, taskId, "done", "done"))}
            />
            <TaskColumn
              columnKey="waiting"
              title="Waiting"
              tasks={plan.board.waiting}
              pendingTaskId={pendingTaskId}
              moveTargets={BOARD_TARGETS}
              assignees={assignees}
              onApprove={(taskId) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, "today", "today"), () => approveClientTask(taskId))}
              onAssign={(taskId, assignee) => mutateTask(taskId, (current) => updateTaskAssignment(current, taskId, assignee), () => assignClientTask(taskId, assignee))}
              onMove={(taskId, target) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, target as PlanColumnKey), () => moveClientTask(taskId, target as typeof BOARD_TARGETS[number]))}
              onRun={(taskId) => mutateTask(taskId, (current) => markTaskRunning(current, taskId), () => runApprovedClientTask(taskId), (current) => moveTaskBetweenColumns(current, taskId, "done", "done"))}
            />
            <TaskColumn
              columnKey="done"
              title="Done"
              tasks={plan.board.done}
              pendingTaskId={pendingTaskId}
              moveTargets={BOARD_TARGETS}
              assignees={assignees}
              onApprove={(taskId) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, "today", "today"), () => approveClientTask(taskId))}
              onAssign={(taskId, assignee) => mutateTask(taskId, (current) => updateTaskAssignment(current, taskId, assignee), () => assignClientTask(taskId, assignee))}
              onMove={(taskId, target) => mutateTask(taskId, (current) => moveTaskBetweenColumns(current, taskId, target as PlanColumnKey), () => moveClientTask(taskId, target as typeof BOARD_TARGETS[number]))}
              onRun={(taskId) => mutateTask(taskId, (current) => markTaskRunning(current, taskId), () => runApprovedClientTask(taskId), (current) => moveTaskBetweenColumns(current, taskId, "done", "done"))}
            />
          </div>
        </section>

        <section className={`${styles.card} ${styles.span3}`}>
          <div className={styles.sectionTitle}>Approval needed</div>
          {plan.approval_needed.length === 0 ? (
            <div className={styles.mutedText}>No approval-gated tasks right now.</div>
          ) : (
            <div className={styles.stack}>
              {plan.approval_needed.map((task) => (
                <div key={task.id} className={`${styles.itemCard} ${styles.itemCardWarn}`} data-cy={`task-card-${task.id}`}>
                  <div className={styles.itemTitle}>{task.title}</div>
                  <div className={styles.itemMeta}>{task.owner_label}</div>
                  <button className={`${styles.button} ${styles.buttonWarn}`} onClick={() => mutateTask(task.id, (current) => moveTaskBetweenColumns(current, task.id, "today", "today"), () => approveClientTask(task.id))} disabled={pendingTaskId === task.id} data-cy={`task-${task.id}-approve-now`}>
                    Approve now
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Capacity</div>
          {plan.capacity.length === 0 ? (
            <div className={styles.mutedText}>No capacity data yet.</div>
          ) : (
            plan.capacity.map((item) => (
              <div key={item.owner_label} className={styles.capacityBlock}>
                <div className={styles.capacityRow}>
                  <span>{item.owner_label}</span>
                  <span className={item.overloaded ? styles.errorText : styles.mutedText}>{item.load_percent}%</span>
                </div>
                <div className={styles.capacityTrack}>
                  <progress className={`${styles.capacityMeter} ${item.overloaded ? styles.capacityMeterOverloaded : ""}`} max={100} value={item.load_percent} />
                </div>
              </div>
            ))
          )}
        </section>

        <section className={`${styles.card} ${styles.span6}`}>
          <div className={styles.sectionTitle}>Dependencies</div>
          {plan.dependencies.length === 0 ? (
            <div className={styles.mutedText}>No dependency map yet.</div>
          ) : (
            <div className={styles.dividerList}>
              {plan.dependencies.map((dependency) => (
                <div key={dependency.task_id}>
                  <div className={styles.itemTitle}>{dependency.task_id}</div>
                  <div className={styles.itemMeta}>
                  Blocked by {dependency.blocked_by_ids.length || 0} · Unlocks {dependency.unlocks_ids.length || 0}
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className={`${styles.card} ${styles.span12}`}>
          <div className={styles.sectionTitle}>Client update draft</div>
          <div className={styles.inlineWrap}>
            <button className={`${styles.button} ${styles.buttonPrimary}`} onClick={handleDraftUpdate}>
              Draft client update
            </button>
            {actionError ? <span className={styles.errorText}>{actionError}</span> : null}
          </div>
          {draft ? <pre className={styles.draftBlock}>{draft.markdown}</pre> : null}
        </section>
      </div>
    </ClientShell>
  );
}