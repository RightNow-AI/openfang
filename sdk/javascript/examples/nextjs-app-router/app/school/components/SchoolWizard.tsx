"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { SCHOOL_TASK_CATALOG } from "../../../lib/mode-types";
import { createRecord, generateModePlan } from "../../../lib/mode-api";

type StepKey = "program" | "audience" | "content" | "cohort" | "review";

const STEPS = [
  { key: "program"  as StepKey, label: "Program",  description: "What is the program?" },
  { key: "audience" as StepKey, label: "Audience",  description: "Who is it for?" },
  { key: "content"  as StepKey, label: "Content",   description: "Curriculum and delivery" },
  { key: "cohort"   as StepKey, label: "Cohort",    description: "Approval and ops settings" },
  { key: "review"   as StepKey, label: "Review",    description: "Confirm and launch" },
];

const INPUT: React.CSSProperties = {
  width: "100%", padding: "10px 12px", borderRadius: 6,
  border: "1px solid var(--border)", background: "var(--input-bg)",
  color: "inherit", fontSize: 14, boxSizing: "border-box", marginBottom: 12,
};
const LABEL: React.CSSProperties = {
  display: "block", fontSize: 13, fontWeight: 500, marginBottom: 4,
  color: "var(--text-muted, #aaa)",
};
const CARD: React.CSSProperties = {
  border: "1px solid var(--border)", borderRadius: 8, padding: "12px 16px", marginBottom: 8, cursor: "pointer",
};

type ProgramMeta = {
  who_its_for: string;
  outcome: string;
  duration: string;
  delivery_style: string;
  price: string;
  cohort_size: string;
  notes: string;
  require_content_approval: boolean;
  require_send_approval: boolean;
  enable_student_health_tracking: boolean;
};

const EMPTY_META: ProgramMeta = {
  who_its_for: "", outcome: "", duration: "", delivery_style: "",
  price: "", cohort_size: "", notes: "",
  require_content_approval: true, require_send_approval: true,
  enable_student_health_tracking: true,
};

export default function SchoolWizard() {
  const router = useRouter();
  const [stepIndex, setStepIndex] = useState(0);
  const step = STEPS[stepIndex].key;

  const [title, setTitle] = useState("");
  const [goal, setGoal]   = useState("");
  const [meta, setMeta]   = useState<ProgramMeta>(EMPTY_META);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([
    "define_program_brief", "map_student_needs", "build_curriculum_outline",
    "write_lessons", "run_cohort_onboarding", "track_student_health",
  ]);
  const [busy, setBusy]   = useState(false);
  const [error, setError] = useState("");

  const setM = (k: keyof ProgramMeta, v: string | boolean) =>
    setMeta((m) => ({ ...m, [k]: v }));

  const stepValid = useMemo(() => {
    if (step === "program") return title.trim().length > 0 && goal.trim().length > 0;
    if (step === "cohort") return selectedTaskIds.length > 0;
    return true;
  }, [step, title, goal, selectedTaskIds]);

  const toggleTask = (id: string) =>
    setSelectedTaskIds((prev) => prev.includes(id) ? prev.filter((t) => t !== id) : [...prev, id]);

  async function handleLaunch() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const { record } = await createRecord("school", {
        title, subtitle: meta.delivery_style || "School program",
        goal, status: "active",
        meta: meta as unknown as Record<string, unknown>,
      });
      await generateModePlan("school", record.id, selectedTaskIds);
      router.push(`/school/${record.id}`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Error");
      setBusy(false);
    }
  }

  return (
    <div style={{ maxWidth: 680, margin: "0 auto", padding: "24px 16px" }}>
      {/* Step progress */}
      <div style={{ display: "flex", gap: 4, marginBottom: 32 }}>
        {STEPS.map((s, i) => (
          <div key={s.key} onClick={() => i < stepIndex && setStepIndex(i)}
            style={{
              flex: 1, padding: "6px 0", textAlign: "center", fontSize: 12, borderRadius: 6,
              cursor: i < stepIndex ? "pointer" : "default",
              background: i === stepIndex ? "var(--accent)" : i < stepIndex ? "var(--border)" : "transparent",
              border: "1px solid var(--border)",
              color: i === stepIndex ? "#fff" : "var(--text-muted, #888)",
              fontWeight: i === stepIndex ? 600 : 400,
            }}
          >{s.label}</div>
        ))}
      </div>

      <h2 style={{ fontSize: 20, fontWeight: 700, marginBottom: 4 }}>{STEPS[stepIndex].label}</h2>
      <p style={{ color: "var(--text-muted, #888)", marginBottom: 24, fontSize: 14 }}>{STEPS[stepIndex].description}</p>

      {/* ── Step 1: Program ── */}
      {step === "program" && (
        <div>
          <label style={LABEL}>Program name *</label>
          <input style={INPUT} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="AI Marketing Mastery — Cohort 4" />
          <label style={LABEL}>Program goal *</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What will students be able to do after this program?" />
          <label style={LABEL}>Outcome promise (student-facing)</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.outcome} onChange={(e) => setM("outcome", e.target.value)} placeholder="By the end of this program you will…" />
          <label style={LABEL}>Duration</label>
          <input style={INPUT} value={meta.duration} onChange={(e) => setM("duration", e.target.value)} placeholder="8 weeks, 90 days, self-paced…" />
          <label style={LABEL}>Price</label>
          <input style={INPUT} value={meta.price} onChange={(e) => setM("price", e.target.value)} placeholder="$997 / $2,997 / free" />
        </div>
      )}

      {/* ── Step 2: Audience ── */}
      {step === "audience" && (
        <div>
          <label style={LABEL}>Who is this for?</label>
          <textarea style={{ ...INPUT, minHeight: 100, resize: "vertical" }} value={meta.who_its_for} onChange={(e) => setM("who_its_for", e.target.value)} placeholder="Who is the ideal student? Skill level, role, pains…" />
          <label style={LABEL}>Expected cohort size</label>
          <input style={INPUT} value={meta.cohort_size} onChange={(e) => setM("cohort_size", e.target.value)} placeholder="20 students, 100 students…" />
        </div>
      )}

      {/* ── Step 3: Content ── */}
      {step === "content" && (
        <div>
          <label style={LABEL}>Delivery style</label>
          <input style={INPUT} value={meta.delivery_style} onChange={(e) => setM("delivery_style", e.target.value)} placeholder="Live cohort, pre-recorded, hybrid, 1:1 coaching…" />
          <label style={LABEL}>Notes for agents</label>
          <textarea style={{ ...INPUT, minHeight: 100, resize: "vertical" }} value={meta.notes} onChange={(e) => setM("notes", e.target.value)} placeholder="Curriculum philosophy, tone, any existing materials, things to avoid…" />
          <label style={{ ...CARD, display: "flex", alignItems: "center", gap: 12 }}>
            <input type="checkbox" checked={meta.enable_student_health_tracking} onChange={(e) => setM("enable_student_health_tracking", e.target.checked)} style={{ width: 16, height: 16 }} />
            <div>
              <div style={{ fontWeight: 600, fontSize: 14 }}>Enable Student Health Dashboard</div>
              <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>Track attendance, completion, risk score, and upsell readiness per student.</div>
            </div>
          </label>
        </div>
      )}

      {/* ── Step 4: Cohort (task selection + approval) ── */}
      {step === "cohort" && (
        <div>
          <div style={{ marginBottom: 20 }}>
            {(
              [
                { key: "require_content_approval", label: "Approve all student-facing content before publishing" },
                { key: "require_send_approval",    label: "Approve emails and reminders before sending" },
              ] as { key: keyof ProgramMeta; label: string }[]
            ).map(({ key, label }) => (
              <label key={key} style={{ ...CARD, display: "flex", alignItems: "center", gap: 12 }}>
                <input type="checkbox" checked={!!meta[key]} onChange={(e) => setM(key, e.target.checked)} style={{ width: 16, height: 16 }} />
                <span style={{ fontSize: 14 }}>{label}</span>
              </label>
            ))}
          </div>
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 10 }}>Task selection</h3>
          {SCHOOL_TASK_CATALOG.map((t) => {
            const on = selectedTaskIds.includes(t.id);
            return (
              <div key={t.id} onClick={() => toggleTask(t.id)}
                style={{ ...CARD, background: on ? "var(--accent-subtle)" : "transparent", borderColor: on ? "var(--accent)" : "var(--border)" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                  <div>
                    <div style={{ fontWeight: 600, fontSize: 14 }}>{t.title}</div>
                    <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginTop: 2 }}>{t.assigned_agent}</div>
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    {t.approval_required && <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: "rgba(234,179,8,0.2)", color: "#eab308" }}>Approval</span>}
                    <span style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>S{t.screen}</span>
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
          <div style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>Program</div>
            <div style={{ fontWeight: 700, fontSize: 18 }}>{title || "—"}</div>
            <div style={{ fontSize: 14, marginTop: 4 }}>{goal || "—"}</div>
            {meta.duration && <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginTop: 4 }}>Duration: {meta.duration}</div>}
            {meta.price && <div style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>Price: {meta.price}</div>}
          </div>
          <div style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 8 }}>Selected tasks ({selectedTaskIds.length})</div>
            {selectedTaskIds.map((id) => {
              const t = SCHOOL_TASK_CATALOG.find((c) => c.id === id);
              return t ? (
                <div key={id} style={{ fontSize: 14, padding: "4px 0", borderBottom: "1px solid var(--border)" }}>
                  {t.title}
                  {t.approval_required && <span style={{ fontSize: 11, marginLeft: 8, color: "#eab308" }}>needs approval</span>}
                </div>
              ) : null;
            })}
          </div>
          {error && <div style={{ color: "#f87171", marginBottom: 12, fontSize: 14 }}>{error}</div>}
        </div>
      )}

      {/* Nav */}
      <div style={{ display: "flex", justifyContent: "space-between", marginTop: 32 }}>
        <button onClick={() => setStepIndex((i) => i - 1)} disabled={stepIndex === 0}
          style={{ padding: "8px 20px", borderRadius: 6, border: "1px solid var(--border)", background: "transparent", color: "inherit", cursor: stepIndex === 0 ? "not-allowed" : "pointer", opacity: stepIndex === 0 ? 0.4 : 1 }}>
          Back
        </button>
        {stepIndex < STEPS.length - 1 ? (
          <button onClick={() => setStepIndex((i) => i + 1)} disabled={!stepValid}
            style={{ padding: "8px 20px", borderRadius: 6, border: "none", background: "var(--accent)", color: "#fff", cursor: stepValid ? "pointer" : "not-allowed", opacity: stepValid ? 1 : 0.5 }}>
            Next
          </button>
        ) : (
          <button onClick={handleLaunch} disabled={busy}
            style={{ padding: "8px 24px", borderRadius: 6, border: "none", background: "var(--accent)", color: "#fff", cursor: "pointer", fontWeight: 600 }}>
            {busy ? "Launching…" : "Launch Program"}
          </button>
        )}
      </div>
    </div>
  );
}
