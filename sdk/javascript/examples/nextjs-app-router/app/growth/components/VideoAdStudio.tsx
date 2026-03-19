"use client";

import { useState } from "react";
import type { VideoAdProject, VideoAdStep } from "../../../lib/mode-types";

const STEP_ORDER: VideoAdStep[] = [
  "brief", "hook", "script", "shot_list", "asset_request",
  "video_draft", "approval", "publish", "measure",
];

const STEP_LABELS: Record<VideoAdStep, string> = {
  brief:         "Campaign Brief",
  hook:          "Hook",
  script:        "Script",
  shot_list:     "Shot List",
  asset_request: "Asset Request",
  video_draft:   "Video Draft",
  approval:      "Approval",
  publish:       "Publish",
  measure:       "Measure",
};

const STEP_DESCRIPTIONS: Record<VideoAdStep, string> = {
  brief:         "Define the ad goal, offer, and audience for this video",
  hook:          "Write 5–10 scroll-stopping hooks for the first 3 seconds",
  script:        "Full ad script with hook, body, and CTA",
  shot_list:     "Scene-by-scene shot list for production",
  asset_request: "List every asset needed: footage, graphics, voiceover",
  video_draft:   "Review the assembled draft before final approval",
  approval:      "Human approval gate — approve before publishing",
  publish:       "Publish to approved channels",
  measure:       "Pull performance data: views, CTR, conversions, ROAS",
};

type Props = {
  project: VideoAdProject;
  onUpdate: (updated: VideoAdProject) => void;
};

export default function VideoAdStudio({ project, onUpdate }: Props) {
  const currentIdx = STEP_ORDER.indexOf(project.step);
  const [note, setNote] = useState("");

  function advanceStep() {
    if (currentIdx >= STEP_ORDER.length - 1) return;
    const next = STEP_ORDER[currentIdx + 1];
    onUpdate({
      ...project,
      step: next,
    });
    setNote("");
  }

  return (
    <div>
      {/* Progress trail */}
      <div style={{ display: "flex", gap: 2, marginBottom: 28, overflowX: "auto", paddingBottom: 4 }}>
        {STEP_ORDER.map((s, i) => {
          const done    = STEP_ORDER.indexOf(s) < currentIdx;
          const current = s === project.step;
          return (
            <div key={s} style={{ flex: "0 0 auto", minWidth: 72, textAlign: "center" }}>
              <div style={{
                width: 28, height: 28, borderRadius: "50%", margin: "0 auto 4px",
                display: "flex", alignItems: "center", justifyContent: "center",
                fontSize: 12, fontWeight: 700,
                background: done ? "#22c55e" : current ? "var(--accent)" : "var(--border)",
                color: done || current ? "#fff" : "var(--text-muted, #888)",
                border: current ? "2px solid #a78bfa" : "2px solid transparent",
              }}>
                {done ? "✓" : i + 1}
              </div>
              <div style={{ fontSize: 10, color: current ? "inherit" : "var(--text-muted, #888)", lineHeight: 1.2 }}>
                {STEP_LABELS[s]}
              </div>
            </div>
          );
        })}
      </div>

      {/* Active step */}
      <div style={{ border: "1px solid var(--accent)", borderRadius: 10, padding: 20, marginBottom: 20 }}>
        <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", marginBottom: 12 }}>
          <div>
            <div style={{ fontSize: 12, color: "var(--text-muted, #888)", marginBottom: 4 }}>
              Step {currentIdx + 1} of {STEP_ORDER.length}
            </div>
            <h3 style={{ fontSize: 18, fontWeight: 700 }}>{STEP_LABELS[project.step]}</h3>
            <p style={{ fontSize: 14, color: "var(--text-muted, #888)", marginTop: 4 }}>
              {STEP_DESCRIPTIONS[project.step]}
            </p>
          </div>
          {project.step === "approval" && (
            <span style={{ padding: "4px 10px", borderRadius: 999, background: "rgba(234,179,8,0.2)", color: "#eab308", fontSize: 12, fontWeight: 600 }}>
              Approval gate
            </span>
          )}
        </div>

        <textarea
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder={`Add notes or output for the ${STEP_LABELS[project.step]} step…`}
          style={{
            width: "100%", minHeight: 100, padding: "10px 12px",
            borderRadius: 6, border: "1px solid var(--border)",
            background: "var(--input-bg)", color: "inherit",
            fontSize: 14, resize: "vertical", boxSizing: "border-box", marginBottom: 14,
          }}
        />

        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button
            onClick={advanceStep}
            disabled={currentIdx >= STEP_ORDER.length - 1}
            style={{
              padding: "8px 20px", borderRadius: 6, border: "none",
              background: currentIdx >= STEP_ORDER.length - 1 ? "var(--border)" : "var(--accent)",
              color: "#fff", cursor: currentIdx >= STEP_ORDER.length - 1 ? "not-allowed" : "pointer",
              fontWeight: 600,
            }}
          >
            {currentIdx >= STEP_ORDER.length - 1 ? "Complete" : `Mark done → ${STEP_LABELS[STEP_ORDER[currentIdx + 1]]}`}
          </button>
        </div>
      </div>

      {/* Performance (visible after measure step) */}
      {currentIdx >= STEP_ORDER.indexOf("measure") && project.performance && (
        <div style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 16 }}>
          <h4 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>Performance</h4>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))", gap: 12 }}>
            {[
              ["CTR",        project.performance.ctr        ? `${(project.performance.ctr * 100).toFixed(1)}%` : "—"],
              ["CPC",        project.performance.cpc        ? `$${project.performance.cpc.toFixed(2)}`         : "—"],
              ["CPA",        project.performance.cpa        ? `$${project.performance.cpa.toFixed(2)}`         : "—"],
              ["ROAS",       project.performance.roas       ? `${project.performance.roas.toFixed(1)}x`        : "—"],
              ["Watch Time", project.performance.watch_time_pct ? `${(project.performance.watch_time_pct * 100).toFixed(1)}%` : "—"],
            ].map(([label, value]) => (
              <div key={label as string} style={{ textAlign: "center", padding: 10, border: "1px solid var(--border)", borderRadius: 8 }}>
                <div style={{ fontSize: 18, fontWeight: 700 }}>{value ?? "—"}</div>
                <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 2 }}>{label}</div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
