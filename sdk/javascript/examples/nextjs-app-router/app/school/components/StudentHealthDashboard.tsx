"use client";

import type { StudentHealthRecord } from "../../../lib/mode-types";

type Props = {
  students: StudentHealthRecord[];
};

const FLAG_COLOR: Record<string, string> = {
  at_risk:      "#ef4444",
  upsell_ready: "#a78bfa",
  star_student: "#eab308",
};

function RiskBadge({ score }: { score: number }) {
  const color  = score >= 70 ? "#ef4444" : score >= 40 ? "#eab308" : "#22c55e";
  const label  = score >= 70 ? "High Risk" : score >= 40 ? "Watch" : "On Track";
  return (
    <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: `${color}22`, color, fontWeight: 600 }}>
      {label}
    </span>
  );
}

function ProgressBar({ pct, color = "var(--accent, #7c3aed)" }: { pct: number; color?: string }) {
  return (
    <div style={{ height: 5, background: "var(--border, #333)", borderRadius: 999, overflow: "hidden" }}>
      <div style={{ height: "100%", background: color, borderRadius: 999, width: `${Math.min(100, pct)}%`, transition: "width 0.3s" }} />
    </div>
  );
}

export default function StudentHealthDashboard({ students }: Props) {
  if (students.length === 0) {
    return (
      <div style={{ padding: 24, textAlign: "center", color: "var(--text-muted, #888)", fontSize: 14, border: "1px dashed var(--border, #333)", borderRadius: 8 }}>
        No student health records yet.
      </div>
    );
  }

  const atRisk      = students.filter((s) => s.risk_score >= 70).length;
  const watching    = students.filter((s) => s.risk_score >= 40 && s.risk_score < 70).length;
  const upsellReady = students.filter((s) => s.upsell_readiness >= 70).length;
  const avgCompletion = students.reduce((a, s) => a + s.assignment_completion_pct, 0) / students.length;

  return (
    <div>
      {/* Summary row */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))", gap: 12, marginBottom: 24 }}>
        {[
          { label: "Total Students",  value: students.length,                color: "inherit" },
          { label: "At Risk",         value: atRisk,                         color: atRisk > 0 ? "#ef4444" : "#22c55e" },
          { label: "Watching",        value: watching,                       color: watching > 0 ? "#eab308" : "inherit" },
          { label: "Upsell Ready",    value: upsellReady,                    color: "#a78bfa" },
          { label: "Avg Completion",  value: `${avgCompletion.toFixed(0)}%`, color: "inherit" },
        ].map(({ label, value, color }) => (
          <div key={label} style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: "12px 14px", textAlign: "center" }}>
            <div style={{ fontSize: 22, fontWeight: 700, color }}>{value}</div>
            <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 2 }}>{label}</div>
          </div>
        ))}
      </div>

      {/* Student rows */}
      <div>
        {students
          .slice()
          .sort((a, b) => b.risk_score - a.risk_score)
          .map((s) => (
            <div key={s.student_id} style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: "14px 16px", marginBottom: 10 }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 12 }}>
                <div>
                  <div style={{ fontWeight: 600, fontSize: 15 }}>{s.student_id}</div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 1 }}>Last seen: {s.last_seen}</div>
                  {s.flags.length > 0 && (
                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 5 }}>
                      {s.flags.map((f) => (
                        <span key={f.type} title={f.note} style={{ fontSize: 10, padding: "1px 7px", borderRadius: 999, background: `${FLAG_COLOR[f.type] ?? "#888"}22`, color: FLAG_COLOR[f.type] ?? "#888" }}>
                          {f.type.replace("_", " ")}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  {s.upsell_readiness >= 70 && (
                    <span style={{ fontSize: 11, padding: "2px 8px", borderRadius: 999, background: "rgba(167,139,250,0.2)", color: "#a78bfa", fontWeight: 600 }}>
                      Upsell Ready
                    </span>
                  )}
                  <RiskBadge score={s.risk_score} />
                </div>
              </div>

              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
                <div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginBottom: 3 }}>
                    Attendance: {s.attendance_pct.toFixed(0)}%
                  </div>
                  <ProgressBar pct={s.attendance_pct} color={s.attendance_pct < 50 ? "#ef4444" : "#22c55e"} />
                </div>
                <div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginBottom: 3 }}>
                    Completion: {s.assignment_completion_pct.toFixed(0)}%
                  </div>
                  <ProgressBar pct={s.assignment_completion_pct} color={s.assignment_completion_pct < 50 ? "#eab308" : "#22c55e"} />
                </div>
                <div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginBottom: 3 }}>
                    Community: {s.community_participation_score.toFixed(1)}/10
                  </div>
                  <ProgressBar pct={s.community_participation_score * 10} color="#3b82f6" />
                </div>
                <div>
                  <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginBottom: 3 }}>
                    Upsell readiness: {s.upsell_readiness.toFixed(0)}/100
                  </div>
                  <ProgressBar pct={s.upsell_readiness} color="#a78bfa" />
                </div>
              </div>

              {s.support_requests > 0 && (
                <div style={{ fontSize: 12, color: "#eab308", marginTop: 8 }}>
                  ⚠ {s.support_requests} support request{s.support_requests !== 1 ? "s" : ""}
                </div>
              )}
            </div>
          ))}
      </div>
    </div>
  );
}
