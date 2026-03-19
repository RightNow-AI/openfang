"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { GROWTH_TASK_CATALOG } from "../../../lib/mode-types";
import { createRecord, generateModePlan } from "../../../lib/mode-api";

type StepKey = "brief" | "market" | "creative" | "production" | "review";

const STEPS = [
  { key: "brief"      as StepKey, label: "Brief",      description: "Campaign goals and offer" },
  { key: "market"     as StepKey, label: "Market",     description: "Audience and competitive intel" },
  { key: "creative"   as StepKey, label: "Creative",   description: "Channel and creative preferences" },
  { key: "production" as StepKey, label: "Production", description: "Approval thresholds" },
  { key: "review"     as StepKey, label: "Review",     description: "Confirm and launch" },
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

type CampaignMeta = {
  campaign_goal: string;
  offer: string;
  audience: string;
  channel: string;
  competitor_ads: string;
  top_hooks: string;
  notes: string;
  require_hook_approval: boolean;
  require_script_approval: boolean;
  require_publish_approval: boolean;
  require_spend_approval: boolean;
  include_video_studio: boolean;
};

const EMPTY_META: CampaignMeta = {
  campaign_goal: "", offer: "", audience: "", channel: "",
  competitor_ads: "", top_hooks: "", notes: "",
  require_hook_approval: true, require_script_approval: true,
  require_publish_approval: true, require_spend_approval: true,
  include_video_studio: true,
};

export default function GrowthWizard() {
  const router = useRouter();
  const [stepIndex, setStepIndex] = useState(0);
  const step = STEPS[stepIndex].key;

  const [title, setTitle] = useState("");
  const [goal, setGoal]   = useState("");
  const [meta, setMeta]   = useState<CampaignMeta>(EMPTY_META);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([
    "write_campaign_brief", "sharpen_offer", "generate_hooks", "write_scripts", "video_studio_flow",
  ]);
  const [busy, setBusy]   = useState(false);
  const [error, setError] = useState("");

  const setM = (k: keyof CampaignMeta, v: string | boolean) =>
    setMeta((m) => ({ ...m, [k]: v }));

  const stepValid = useMemo(() => {
    if (step === "brief") return title.trim().length > 0 && goal.trim().length > 0;
    if (step === "production") return selectedTaskIds.length > 0;
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
      const { record } = await createRecord("growth", {
        title, subtitle: meta.channel || "Growth campaign",
        goal, status: "active",
        meta: meta as unknown as Record<string, unknown>,
      });
      await generateModePlan("growth", record.id, selectedTaskIds);
      router.push(`/growth/${record.id}`);
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

      {/* ── Step 1: Brief ── */}
      {step === "brief" && (
        <div>
          <label style={LABEL}>Campaign name *</label>
          <input style={INPUT} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Q3 Scaling Push — Facebook + Email" />
          <label style={LABEL}>Campaign goal *</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What does success look like? (leads, sales, ROAS…)" />
          <label style={LABEL}>Offer</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.offer} onChange={(e) => setM("offer", e.target.value)} placeholder="Describe the product/offer we're promoting" />
          <label style={LABEL}>Campaign-specific goal detail</label>
          <input style={INPUT} value={meta.campaign_goal} onChange={(e) => setM("campaign_goal", e.target.value)} placeholder="100 high-quality leads at < $40 CPL" />
        </div>
      )}

      {/* ── Step 2: Market ── */}
      {step === "market" && (
        <div>
          <label style={LABEL}>Target audience</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.audience} onChange={(e) => setM("audience", e.target.value)} placeholder="Who is the perfect customer? Demographics, pains, desires…" />
          <label style={LABEL}>Competitor ads (URLs or notes)</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.competitor_ads} onChange={(e) => setM("competitor_ads", e.target.value)} placeholder="Paste competitor ad examples or URLs to analyse" />
          <label style={LABEL}>Top hooks / angles already tested</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.top_hooks} onChange={(e) => setM("top_hooks", e.target.value)} placeholder="What hooks have worked before? What flopped?" />
        </div>
      )}

      {/* ── Step 3: Creative ── */}
      {step === "creative" && (
        <div>
          <label style={LABEL}>Channel</label>
          <input style={INPUT} value={meta.channel} onChange={(e) => setM("channel", e.target.value)} placeholder="Facebook, YouTube, Email, LinkedIn…" />
          <label style={LABEL}>Notes</label>
          <textarea style={{ ...INPUT, minHeight: 80, resize: "vertical" }} value={meta.notes} onChange={(e) => setM("notes", e.target.value)} placeholder="Tone, style, brand guidelines, don'ts…" />
          <label style={{ ...CARD, display: "flex", alignItems: "center", gap: 12 }}>
            <input type="checkbox" checked={meta.include_video_studio} onChange={(e) => setM("include_video_studio", e.target.checked)} style={{ width: 16, height: 16 }} />
            <div>
              <div style={{ fontWeight: 600, fontSize: 14 }}>Include Video Ad Studio</div>
              <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>Brief → Hook → Script → Shot list → Asset request → Video draft → Approve → Publish → Measure</div>
            </div>
          </label>
        </div>
      )}

      {/* ── Step 4: Production (task selection) ── */}
      {step === "production" && (
        <div>
          <p style={{ fontSize: 14, color: "var(--text-muted, #888)", marginBottom: 16 }}>Select tasks and set approval thresholds.</p>
          <div style={{ marginBottom: 20 }}>
            {(
              [
                { key: "require_hook_approval",    label: "Approve hooks before scripting" },
                { key: "require_script_approval",  label: "Approve scripts before production" },
                { key: "require_publish_approval", label: "Approve assets before publishing" },
                { key: "require_spend_approval",   label: "Approve spend before experiments launch" },
              ] as { key: keyof CampaignMeta; label: string }[]
            ).map(({ key, label }) => (
              <label key={key} style={{ ...CARD, display: "flex", alignItems: "center", gap: 12 }}>
                <input type="checkbox" checked={!!meta[key]} onChange={(e) => setM(key, e.target.checked)} style={{ width: 16, height: 16 }} />
                <span style={{ fontSize: 14 }}>{label}</span>
              </label>
            ))}
          </div>
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 10 }}>Task selection</h3>
          {GROWTH_TASK_CATALOG.map((t) => {
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
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 4 }}>Campaign</div>
            <div style={{ fontWeight: 700, fontSize: 18 }}>{title || "—"}</div>
            <div style={{ fontSize: 14, marginTop: 4 }}>{goal || "—"}</div>
            {meta.channel && <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginTop: 4 }}>Channel: {meta.channel}</div>}
          </div>
          <div style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 8 }}>Selected tasks ({selectedTaskIds.length})</div>
            {selectedTaskIds.map((id) => {
              const t = GROWTH_TASK_CATALOG.find((c) => c.id === id);
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
            {busy ? "Launching…" : "Launch Growth Campaign"}
          </button>
        )}
      </div>
    </div>
  );
}
