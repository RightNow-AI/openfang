"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  createClient,
  generatePlan,
  getClient,
  updateClient,
} from "../../../lib/command-center-api";
import {
  TASK_CATALOG,
  type ClientProfile,
  type PlannedTask,
  type TaskType,
} from "../../../lib/command-center-types";

type StepKey = "client" | "context" | "approval" | "task" | "review";

const STEPS: { key: StepKey; label: string; description: string }[] = [
  { key: "client",   label: "Client",   description: "Who is this for" },
  { key: "context",  label: "Context",  description: "What should we know" },
  { key: "approval", label: "Approval", description: "What needs approval" },
  { key: "task",     label: "Tasks",    description: "Choose what to do" },
  { key: "review",   label: "Review",   description: "Review the plan" },
];

const STEP_KEYS: StepKey[] = STEPS.map((s) => s.key);

const EMPTY_PROFILE: ClientProfile = {
  id: "",
  client_name: "",
  business_name: "",
  industry: "",
  main_goal: "",
  website_url: "",
  offer: "",
  customer: "",
  notes: "",
  approval_mode: "required",
  approvers: [{ name: "", email: "" }],
  require_approval_for_email: true,
  require_approval_for_tool_use: true,
  require_approval_for_assignment: true,
  created_at: "",
  updated_at: "",
};

const INPUT_STYLE: React.CSSProperties = {
  width: "100%",
  padding: "10px 12px",
  borderRadius: 6,
  border: "1px solid var(--border, #333)",
  background: "var(--input-bg, #1a1a1a)",
  color: "inherit",
  fontSize: 14,
  boxSizing: "border-box",
  marginBottom: 12,
};

const LABEL_STYLE: React.CSSProperties = {
  display: "block",
  fontSize: 13,
  fontWeight: 500,
  marginBottom: 4,
  color: "var(--text-muted, #aaa)",
};

type Props = {
  initialClientId?: string;
};

export default function CommandCenterWizard({ initialClientId }: Props) {
  const router = useRouter();
  const [stepIndex, setStepIndex] = useState(0);
  const step = STEP_KEYS[stepIndex];

  const [profile, setProfile] = useState<ClientProfile>(EMPTY_PROFILE);
  const [selectedTaskIds, setSelectedTaskIds] = useState<TaskType[]>([
    "summarize_business",
    "draft_outreach_emails",
  ]);
  const [loading, setLoading] = useState(false);
  const [planTasks, setPlanTasks] = useState<PlannedTask[]>([]);
  const [planSummary, setPlanSummary] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (!initialClientId) return;
    getClient(initialClientId)
      .then((data) => setProfile(data.client))
      .catch((e: Error) => setError(e.message));
  }, [initialClientId]);

  const stepValid = useMemo(() => {
    if (step === "client")
      return Boolean(
        profile.client_name && profile.business_name && profile.industry && profile.main_goal,
      );
    if (step === "context")
      return Boolean(profile.website_url && profile.offer && profile.customer);
    if (step === "approval") return Boolean(profile.approval_mode);
    if (step === "task") return selectedTaskIds.length > 0;
    return true;
  }, [profile, selectedTaskIds, step]);

  async function saveProfile(): Promise<ClientProfile> {
    setLoading(true);
    setError("");
    try {
      if (!profile.id) {
        const res = await createClient(profile);
        setProfile(res.client);
        return res.client;
      }
      const res = await updateClient(profile.id, profile);
      setProfile(res.client);
      return res.client;
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Save failed";
      setError(msg);
      throw new Error(msg);
    } finally {
      setLoading(false);
    }
  }

  async function next() {
    if (!stepValid) return;
    try {
      const saved = await saveProfile();

      if (step === "task") {
        const plan = await generatePlan({
          client_id: saved.id,
          selected_task_types: selectedTaskIds,
        });
        setPlanTasks(plan.tasks);
        setPlanSummary(
          `${plan.tasks.length} task${plan.tasks.length !== 1 ? "s" : ""} ready. ` +
            "Any that need approval are flagged.",
        );
      }

      setStepIndex((i) => Math.min(i + 1, STEP_KEYS.length - 1));
    } catch {
      // error already set in saveProfile
    }
  }

  function back() {
    setError("");
    setStepIndex((i) => Math.max(i - 1, 0));
  }

  function toggleTask(taskId: TaskType) {
    setSelectedTaskIds((prev) =>
      prev.includes(taskId) ? prev.filter((x) => x !== taskId) : [...prev, taskId],
    );
  }

  function finish() {
    if (!profile.id) return;
    router.push(`/command-center/${profile.id}`);
  }

  // ── Progress bar ────────────────────────────────────────────────────────
  const progressBar = (
    <div style={{ display: "flex", gap: 0, marginBottom: 32 }}>
      {STEPS.map((s, i) => {
        const done    = i < stepIndex;
        const current = i === stepIndex;
        return (
          <div key={s.key} style={{ flex: 1, display: "flex", alignItems: "center" }}>
            <div style={{ flex: 1 }}>
              <div
                style={{
                  width: 28,
                  height: 28,
                  borderRadius: "50%",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 12,
                  fontWeight: 700,
                  background: done ? "var(--accent, #7c6af7)" : current ? "var(--accent, #7c6af7)" : "var(--surface2, #222)",
                  color: done || current ? "#fff" : "var(--text-muted, #888)",
                  border: current ? "2px solid var(--accent, #7c6af7)" : done ? "none" : "1px solid var(--border, #333)",
                  cursor: done ? "pointer" : "default",
                }}
                onClick={() => done && setStepIndex(i)}
              >
                {done ? "✓" : i + 1}
              </div>
              {!current && <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 4 }}>{s.label}</div>}
              {current && <div style={{ fontSize: 11, fontWeight: 600, marginTop: 4 }}>{s.label}</div>}
            </div>
            {i < STEPS.length - 1 && (
              <div style={{
                flex: 1,
                height: 2,
                background: done ? "var(--accent, #7c6af7)" : "var(--border, #333)",
                margin: "0 4px",
                marginBottom: 20,
              }} />
            )}
          </div>
        );
      })}
    </div>
  );

  // ── Steps ────────────────────────────────────────────────────────────────
  const stepContent = (
    <>
      {step === "client" && (
        <section>
          <h2 style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>Who is this for?</h2>
          <p style={{ color: "var(--text-muted, #888)", marginBottom: 20, fontSize: 14 }}>
            Start with the basics. Who is the client and what do they need done right now?
          </p>
          <label style={LABEL_STYLE}>Client name *</label>
          <input style={INPUT_STYLE} placeholder="e.g. Sarah Johnson"
            value={profile.client_name}
            onChange={(e) => setProfile({ ...profile, client_name: e.target.value })} />

          <label style={LABEL_STYLE}>Business name *</label>
          <input style={INPUT_STYLE} placeholder="e.g. Northstar Studio"
            value={profile.business_name}
            onChange={(e) => setProfile({ ...profile, business_name: e.target.value })} />

          <label style={LABEL_STYLE}>Industry *</label>
          <input style={INPUT_STYLE} placeholder="e.g. Brand strategy, SaaS, E-commerce"
            value={profile.industry}
            onChange={(e) => setProfile({ ...profile, industry: e.target.value })} />

          <label style={LABEL_STYLE}>Main goal right now *</label>
          <textarea style={{ ...INPUT_STYLE, minHeight: 80, resize: "vertical" }}
            placeholder="e.g. Book 20 qualified calls this month"
            value={profile.main_goal}
            onChange={(e) => setProfile({ ...profile, main_goal: e.target.value })} />
        </section>
      )}

      {step === "context" && (
        <section>
          <h2 style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>What should we know first?</h2>
          <p style={{ color: "var(--text-muted, #888)", marginBottom: 20, fontSize: 14 }}>
            Give agents the context they need before they act.
          </p>
          <label style={LABEL_STYLE}>Website URL *</label>
          <input style={INPUT_STYLE} placeholder="https://example.com"
            type="url"
            value={profile.website_url}
            onChange={(e) => setProfile({ ...profile, website_url: e.target.value })} />

          <label style={LABEL_STYLE}>What does the business sell? *</label>
          <textarea style={{ ...INPUT_STYLE, minHeight: 72, resize: "vertical" }}
            placeholder="e.g. A 12-week messaging sprint for founder-led agencies"
            value={profile.offer}
            onChange={(e) => setProfile({ ...profile, offer: e.target.value })} />

          <label style={LABEL_STYLE}>Who is the customer? *</label>
          <textarea style={{ ...INPUT_STYLE, minHeight: 72, resize: "vertical" }}
            placeholder="e.g. Founders running 1–5 person brand agencies"
            value={profile.customer}
            onChange={(e) => setProfile({ ...profile, customer: e.target.value })} />

          <label style={LABEL_STYLE}>Extra notes (optional)</label>
          <textarea style={{ ...INPUT_STYLE, minHeight: 80, resize: "vertical" }}
            placeholder="Competitors, tone of voice, files to read, etc."
            value={profile.notes}
            onChange={(e) => setProfile({ ...profile, notes: e.target.value })} />
        </section>
      )}

      {step === "approval" && (
        <section>
          <h2 style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>What work needs approval?</h2>
          <p style={{ color: "var(--text-muted, #888)", marginBottom: 20, fontSize: 14 }}>
            Nothing sensitive executes without a human sign-off when required.
          </p>

          <label style={LABEL_STYLE}>Approval mode</label>
          <select
            style={{ ...INPUT_STYLE }}
            value={profile.approval_mode}
            onChange={(e) =>
              setProfile({ ...profile, approval_mode: e.target.value as ClientProfile["approval_mode"] })
            }
          >
            <option value="none">No approval — run everything automatically</option>
            <option value="required">Approval required — nothing runs without a human yes</option>
            <option value="conditional">Conditional — only high-risk tasks need approval</option>
          </select>

          <div style={{ display: "flex", flexDirection: "column", gap: 10, marginBottom: 16 }}>
            {(
              [
                ["require_approval_for_email",      "Require approval before sending emails"],
                ["require_approval_for_tool_use",   "Require approval before using external tools"],
                ["require_approval_for_assignment", "Require approval before assigning work"],
              ] as const
            ).map(([field, label]) => (
              <label key={field} style={{ display: "flex", alignItems: "center", gap: 10, fontSize: 14 }}>
                <input
                  type="checkbox"
                  checked={profile[field]}
                  onChange={(e) => setProfile({ ...profile, [field]: e.target.checked })}
                />
                {label}
              </label>
            ))}
          </div>

          <label style={LABEL_STYLE}>Approver name</label>
          <input style={INPUT_STYLE} placeholder="e.g. Sarah Johnson"
            value={profile.approvers[0]?.name ?? ""}
            onChange={(e) =>
              setProfile({ ...profile, approvers: [{ ...profile.approvers[0], name: e.target.value }] })
            } />

          <label style={LABEL_STYLE}>Approver email</label>
          <input style={INPUT_STYLE} type="email" placeholder="approver@example.com"
            value={profile.approvers[0]?.email ?? ""}
            onChange={(e) =>
              setProfile({ ...profile, approvers: [{ ...profile.approvers[0], email: e.target.value }] })
            } />
        </section>
      )}

      {step === "task" && (
        <section>
          <h2 style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>Choose what to do first</h2>
          <p style={{ color: "var(--text-muted, #888)", marginBottom: 20, fontSize: 14 }}>
            Select the work you want agents to tackle. You can run more later.
          </p>
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            {TASK_CATALOG.map((task) => {
              const selected = selectedTaskIds.includes(task.id);
              return (
                <label
                  key={task.id}
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    gap: 12,
                    padding: "14px 16px",
                    border: `1px solid ${selected ? "var(--accent, #7c6af7)" : "var(--border, #333)"}`,
                    borderRadius: 8,
                    cursor: "pointer",
                    background: selected ? "var(--accent-bg, rgba(124,106,247,0.08))" : "transparent",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={selected}
                    onChange={() => toggleTask(task.id)}
                    style={{ marginTop: 2 }}
                  />
                  <div>
                    <div style={{ fontWeight: 600, marginBottom: 2 }}>{task.title}</div>
                    <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>
                      {task.description}
                    </div>
                    <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>
                      {task.assignedAgent} · {task.estimatedTime}
                      {task.requiresApproval && (
                        <span style={{ marginLeft: 8, color: "var(--warning, #f59e0b)", fontWeight: 500 }}>
                          · Needs approval
                        </span>
                      )}
                    </div>
                  </div>
                </label>
              );
            })}
          </div>
        </section>
      )}

      {step === "review" && (
        <section>
          <h2 style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>Review the plan</h2>
          <p style={{ color: "var(--text-muted, #888)", marginBottom: 20, fontSize: 14 }}>
            {planSummary}
          </p>
          <div style={{ display: "flex", flexDirection: "column", gap: 10, marginBottom: 24 }}>
            {planTasks.map((task) => (
              <div
                key={task.id}
                style={{
                  padding: "14px 16px",
                  border: "1px solid var(--border, #333)",
                  borderRadius: 8,
                }}
              >
                <div style={{ fontWeight: 600, marginBottom: 4 }}>{task.title}</div>
                <div style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>
                  Agent: {task.assigned_agent} · Tools: {task.required_tools.join(", ")}
                </div>
                {task.approval_required && (
                  <div style={{ fontSize: 13, color: "var(--warning, #f59e0b)", marginTop: 4, fontWeight: 500 }}>
                    Approval required before run
                  </div>
                )}
              </div>
            ))}
          </div>
          <button
            onClick={finish}
            style={{
              padding: "10px 24px",
              background: "var(--accent, #7c6af7)",
              color: "#fff",
              border: "none",
              borderRadius: 6,
              fontWeight: 600,
              cursor: "pointer",
              fontSize: 15,
            }}
          >
            Approve plan and open command center →
          </button>
        </section>
      )}
    </>
  );

  // ── Nav buttons ──────────────────────────────────────────────────────────
  const navButtons = step !== "review" && (
    <div style={{ display: "flex", gap: 10, marginTop: 24 }}>
      {stepIndex > 0 && (
        <button
          onClick={back}
          style={{
            padding: "8px 18px",
            border: "1px solid var(--border, #333)",
            borderRadius: 6,
            background: "transparent",
            color: "inherit",
            cursor: "pointer",
            fontSize: 14,
          }}
        >
          ← Back
        </button>
      )}
      <button
        disabled={!stepValid || loading}
        onClick={next}
        style={{
          padding: "8px 20px",
          background: stepValid ? "var(--accent, #7c6af7)" : "var(--surface2, #333)",
          color: stepValid ? "#fff" : "var(--text-muted, #888)",
          border: "none",
          borderRadius: 6,
          fontWeight: 600,
          cursor: stepValid ? "pointer" : "not-allowed",
          fontSize: 14,
        }}
      >
        {loading ? "Saving…" : step === "task" ? "Generate plan →" : "Continue →"}
      </button>
      <button
        onClick={() => router.push("/command-center/new")}
        style={{
          padding: "8px 14px",
          border: "none",
          background: "transparent",
          color: "var(--text-muted, #888)",
          cursor: "pointer",
          fontSize: 13,
        }}
      >
        Save for later
      </button>
    </div>
  );

  return (
    <div style={{ maxWidth: 640 }}>
      {progressBar}
      {error && (
        <div style={{
          padding: "10px 14px",
          background: "rgba(239,68,68,0.1)",
          border: "1px solid rgba(239,68,68,0.3)",
          borderRadius: 6,
          color: "#ef4444",
          fontSize: 13,
          marginBottom: 16,
        }}>
          {error}
        </div>
      )}
      {stepContent}
      {navButtons}
    </div>
  );
}
