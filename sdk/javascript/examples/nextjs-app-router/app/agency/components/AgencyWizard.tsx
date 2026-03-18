"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { AGENCY_TASK_CATALOG, type AgencyClient } from "../../../lib/mode-types";
import { createRecord, generateModePlan } from "../../../lib/mode-api";

type StepKey = "client" | "context" | "approval" | "tasks" | "review";

const STEPS: { key: StepKey; label: string; description: string }[] = [
  { key: "client",   label: "Client",   description: "Who is this for" },
  { key: "context",  label: "Context",  description: "Business background" },
  { key: "approval", label: "Approval", description: "What needs approval" },
  { key: "tasks",    label: "Tasks",    description: "Select tasks to run" },
  { key: "review",   label: "Review",   description: "Confirm and launch" },
];

const STEP_KEYS: StepKey[] = STEPS.map((s) => s.key);

const INPUT: React.CSSProperties = {
  width: "100%", padding: "10px 12px", borderRadius: 6,
  border: "1px solid var(--border, #333)", background: "var(--input-bg, #1a1a1a)",
  color: "inherit", fontSize: 14, boxSizing: "border-box", marginBottom: 12,
};
const LABEL: React.CSSProperties = {
  display: "block", fontSize: 13, fontWeight: 500, marginBottom: 4,
  color: "var(--text-muted, #aaa)",
};
const CARD: React.CSSProperties = {
  border: "1px solid var(--border, #333)", borderRadius: 8,
  padding: "12px 16px", marginBottom: 8, cursor: "pointer",
};

type ClientMeta = {
  service_requested: string;
  deadline: string;
  budget_band: string;
  point_of_contact: string;
  approval_owner: string;
  business_summary: string;
  offer_summary: string;
  audience_summary: string;
  website_url: string;
  notes: string;
  require_draft_approval: boolean;
  require_send_approval: boolean;
  require_tool_use_approval: boolean;
  require_delivery_approval: boolean;
};

const EMPTY_META: ClientMeta = {
  service_requested: "", deadline: "", budget_band: "", point_of_contact: "",
  approval_owner: "", business_summary: "", offer_summary: "", audience_summary: "",
  website_url: "", notes: "", require_draft_approval: true, require_send_approval: true,
  require_tool_use_approval: false, require_delivery_approval: true,
};

type Props = { initialRecordId?: string };

export default function AgencyWizard({ initialRecordId }: Props) {
  const router = useRouter();
  const [stepIndex, setStepIndex] = useState(0);
  const step = STEP_KEYS[stepIndex];

  const [title, setTitle] = useState("");
  const [goal, setGoal]   = useState("");
  const [meta, setMeta]   = useState<ClientMeta>(EMPTY_META);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([
    "intake_client_brief", "scope_service", "summarize_business",
    "research_competitors", "draft_client_copy",
  ]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const setM = (k: keyof ClientMeta, v: string | boolean) =>
    setMeta((m) => ({ ...m, [k]: v }));

  const stepValid = useMemo(() => {
    if (step === "client") return title.trim().length > 0 && goal.trim().length > 0;
    if (step === "tasks") return selectedTaskIds.length > 0;
    return true;
  }, [step, title, goal, selectedTaskIds]);

  const toggleTask = (id: string) =>
    setSelectedTaskIds((prev) =>
      prev.includes(id) ? prev.filter((t) => t !== id) : [...prev, id]
    );

  async function handleLaunch() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const { record } = await createRecord("agency", {
        title,
        subtitle: meta.service_requested || "Service request",
        goal,
        status: "active",
        meta: meta as unknown as Record<string, unknown>,
      });
      await generateModePlan("agency", record.id, selectedTaskIds);
      router.push(`/agency/${record.id}`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Error creating record");
      setBusy(false);
    }
  }

  return (
    <div style={{ maxWidth: 680, margin: "0 auto", padding: "24px 16px" }}>
      {/* Step progress */}
      <div style={{ display: "flex", gap: 4, marginBottom: 32 }}>
        {STEPS.map((s, i) => (
          <div
            key={s.key}
            onClick={() => i < stepIndex && setStepIndex(i)}
            style={{
              flex: 1, padding: "6px 0", textAlign: "center", fontSize: 12,
              borderRadius: 6, cursor: i < stepIndex ? "pointer" : "default",
              background: i === stepIndex
                ? "var(--accent, #7c3aed)"
                : i < stepIndex
                ? "var(--border, #333)"
                : "transparent",
              border: "1px solid var(--border, #333)",
              color: i === stepIndex ? "#fff" : "var(--text-muted, #888)",
              fontWeight: i === stepIndex ? 600 : 400,
            }}
          >
            {s.label}
          </div>
        ))}
      </div>

      <h2 style={{ fontSize: 20, fontWeight: 700, marginBottom: 4 }}>
        {STEPS[stepIndex].label}
      </h2>
      <p style={{ color: "var(--text-muted, #888)", marginBottom: 24, fontSize: 14 }}>
        {STEPS[stepIndex].description}
      </p>

      {/* ── Step 1: Client ── */}
      {step === "client" && (
        <div>
          <label style={LABEL}>Client / project name *</label>
          <input style={INPUT} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Acme Corp — June Campaign" />
          <label style={LABEL}>Primary goal *</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What do we need to deliver for this client?" />
          <label style={LABEL}>Service requested</label>
          <input style={INPUT} value={meta.service_requested} onChange={(e) => setM("service_requested", e.target.value)} placeholder="Email sequence, landing page, research report…" />
          <label style={LABEL}>Point of contact</label>
          <input style={INPUT} value={meta.point_of_contact} onChange={(e) => setM("point_of_contact", e.target.value)} placeholder="Name and email" />
          <label style={LABEL}>Deadline</label>
          <input style={INPUT} type="date" value={meta.deadline} onChange={(e) => setM("deadline", e.target.value)} />
          <label style={LABEL}>Budget band</label>
          <input style={INPUT} value={meta.budget_band} onChange={(e) => setM("budget_band", e.target.value)} placeholder="$2,500 / month" />
        </div>
      )}

      {/* ── Step 2: Context ── */}
      {step === "context" && (
        <div>
          <label style={LABEL}>Website URL</label>
          <input style={INPUT} value={meta.website_url} onChange={(e) => setM("website_url", e.target.value)} placeholder="https://acme.com" />
          <label style={LABEL}>Business summary</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.business_summary} onChange={(e) => setM("business_summary", e.target.value)} placeholder="What does this business do? Who do they serve?" />
          <label style={LABEL}>Offer summary</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.offer_summary} onChange={(e) => setM("offer_summary", e.target.value)} placeholder="What's the product/service we're promoting?" />
          <label style={LABEL}>Target audience</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.audience_summary} onChange={(e) => setM("audience_summary", e.target.value)} placeholder="Who is the ideal customer?" />
          <label style={LABEL}>Notes</label>
          <textarea style={{ ...INPUT, minHeight: 60, resize: "vertical" }} value={meta.notes} onChange={(e) => setM("notes", e.target.value)} placeholder="Anything else the agents should know" />
        </div>
      )}

      {/* ── Step 3: Approval prefs ── */}
      {step === "approval" && (
        <div>
          <p style={{ fontSize: 14, color: "var(--text-muted, #888)", marginBottom: 16 }}>
            Choose which task types require human approval before proceeding.
          </p>
          {(
            [
              { key: "require_draft_approval",    label: "Draft approval — review written content before sending" },
              { key: "require_send_approval",     label: "Send approval — review before any send action" },
              { key: "require_tool_use_approval", label: "Tool use approval — review before automation tools run" },
              { key: "require_delivery_approval", label: "Client delivery approval — review before final delivery" },
            ] as { key: keyof ClientMeta; label: string }[]
          ).map(({ key, label }) => (
            <label key={key} style={{ ...CARD, display: "flex", alignItems: "center", gap: 12 }}>
              <input
                type="checkbox"
                checked={!!meta[key]}
                onChange={(e) => setM(key, e.target.checked)}
                style={{ width: 16, height: 16 }}
              />
              <span style={{ fontSize: 14 }}>{label}</span>
            </label>
          ))}
          <label style={LABEL}>Approval owner (who reviews?)</label>
          <input style={INPUT} value={meta.approval_owner} onChange={(e) => setM("approval_owner", e.target.value)} placeholder="Name or email of approver" />
        </div>
      )}

      {/* ── Step 4: Tasks ── */}
      {step === "tasks" && (
        <div>
          <p style={{ fontSize: 14, color: "var(--text-muted, #888)", marginBottom: 16 }}>
            Select the tasks to include in this client&apos;s plan. Tasks that need approval are marked.
          </p>
          {AGENCY_TASK_CATALOG.map((t) => {
            const on = selectedTaskIds.includes(t.id);
            return (
              <div
                key={t.id}
                onClick={() => toggleTask(t.id)}
                style={{
                  ...CARD,
                  background: on ? "rgba(124,58,237,0.1)" : "transparent",
                  borderColor: on ? "var(--accent, #7c3aed)" : "var(--border, #333)",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                  <div>
                    <div style={{ fontWeight: 600, fontSize: 14 }}>{t.title}</div>
                    <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 2 }}>{t.assigned_agent}</div>
                  </div>
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    {t.approval_required && (
                      <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: "rgba(234,179,8,0.2)", color: "#eab308" }}>
                        Approval
                      </span>
                    )}
                    <span style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>Screen {t.screen}</span>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ── Step 5: Review ── */}
      {step === "review" && (
        <div>
          <div style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: 16, marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>Client / Project</div>
            <div style={{ fontWeight: 700, fontSize: 18 }}>{title || "—"}</div>
            <div style={{ fontSize: 14, marginTop: 4 }}>{goal || "—"}</div>
          </div>
          <div style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: 16, marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 8 }}>Selected tasks ({selectedTaskIds.length})</div>
            {selectedTaskIds.map((id) => {
              const t = AGENCY_TASK_CATALOG.find((c) => c.id === id);
              return t ? (
                <div key={id} style={{ fontSize: 14, padding: "4px 0", borderBottom: "1px solid var(--border, #333)" }}>
                  {t.title}
                  {t.approval_required && <span style={{ fontSize: 11, marginLeft: 8, color: "#eab308" }}>needs approval</span>}
                </div>
              ) : null;
            })}
          </div>
          {error && <div style={{ color: "#f87171", marginBottom: 12, fontSize: 14 }}>{error}</div>}
        </div>
      )}

      {/* Nav buttons */}
      <div style={{ display: "flex", justifyContent: "space-between", marginTop: 32 }}>
        <button
          onClick={() => setStepIndex((i) => i - 1)}
          disabled={stepIndex === 0}
          style={{ padding: "8px 20px", borderRadius: 6, border: "1px solid var(--border, #333)", background: "transparent", color: "inherit", cursor: stepIndex === 0 ? "not-allowed" : "pointer", opacity: stepIndex === 0 ? 0.4 : 1 }}
        >
          Back
        </button>
        {stepIndex < STEP_KEYS.length - 1 ? (
          <button
            onClick={() => setStepIndex((i) => i + 1)}
            disabled={!stepValid}
            style={{ padding: "8px 20px", borderRadius: 6, border: "none", background: "var(--accent, #7c3aed)", color: "#fff", cursor: stepValid ? "pointer" : "not-allowed", opacity: stepValid ? 1 : 0.5 }}
          >
            Next
          </button>
        ) : (
          <button
            onClick={handleLaunch}
            disabled={busy || !stepValid}
            style={{ padding: "8px 24px", borderRadius: 6, border: "none", background: "var(--accent, #7c3aed)", color: "#fff", cursor: "pointer", fontWeight: 600 }}
          >
            {busy ? "Launching…" : "Launch Agency Plan"}
          </button>
        )}
      </div>
    </div>
  );
}
