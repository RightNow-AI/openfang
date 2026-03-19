"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { getModeResults } from "../../../../lib/mode-api";
import type { ModeResult } from "../../../../lib/mode-types";

function MarkdownCard({ text }: { text?: string }) {
  if (!text) return null;
  return (
    <div style={{ padding: "10px 14px", background: "rgba(255,255,255,0.04)", borderRadius: 6, fontSize: 13, whiteSpace: "pre-wrap", lineHeight: 1.6 }}>
      {text}
    </div>
  );
}

export default function SchoolResultsPage() {
  const { programId } = useParams<{ programId: string }>();
  const [results, setResults]       = useState<ModeResult[]>([]);
  const [selected, setSelected]     = useState<ModeResult | null>(null);
  const [loading, setLoading]       = useState(true);

  useEffect(() => {
    if (!programId) return;
    getModeResults("school", programId).then((r) => {
      setResults(r.results);
      if (r.results.length > 0) setSelected(r.results[0]);
      setLoading(false);
    });
  }, [programId]);

  return (
    <div style={{ padding: 32, maxWidth: 1100, margin: "0 auto" }}>
      <div style={{ marginBottom: 12 }}>
        <Link href={`/school/${programId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Program Overview</Link>
      </div>
      <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 20 }}>Results</h1>

      {loading ? (
        <div style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>Loading…</div>
      ) : results.length === 0 ? (
        <div style={{ padding: 24, border: "1px dashed var(--border)", borderRadius: 8, textAlign: "center", color: "var(--text-muted, #888)", fontSize: 14 }}>
          No results yet. Run tasks to generate outputs.
        </div>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "280px 1fr", gap: 20, alignItems: "start" }}>
          {/* List */}
          <div>
            {results.map((r) => (
              <button
                key={r.id}
                onClick={() => setSelected(r)}
                style={{ display: "block", width: "100%", textAlign: "left", padding: "10px 12px", borderRadius: 8, border: `1px solid ${selected?.id === r.id ? "var(--accent)" : "var(--border)"}`, background: selected?.id === r.id ? "var(--accent-subtle)" : "transparent", cursor: "pointer", marginBottom: 6 }}
              >
                <div style={{ fontSize: 13, fontWeight: 600 }}>{r.title}</div>
                <div style={{ fontSize: 11, color: "var(--text-muted, #888)", marginTop: 2 }}>
                  {r.completed_at ? new Date(r.completed_at).toLocaleDateString() : "–"}
                </div>
              </button>
            ))}
          </div>

          {/* Detail */}
          {selected && (
            <div style={{ border: "1px solid var(--border)", borderRadius: 10, padding: 20 }}>
              <h2 style={{ fontSize: 17, fontWeight: 700, marginBottom: 12 }}>
                {selected.title}
              </h2>
              {selected.content_markdown && (
                <div style={{ marginBottom: 14 }}>
                  <div style={{ fontSize: 12, fontWeight: 600, color: "var(--text-muted, #888)", marginBottom: 4, textTransform: "uppercase", letterSpacing: "0.05em" }}>Output</div>
                  <MarkdownCard text={selected.content_markdown} />
                </div>
              )}
              {selected.what_worked && (
                <div style={{ marginBottom: 10 }}>
                  <div style={{ fontSize: 12, fontWeight: 600, color: "#22c55e", marginBottom: 4 }}>✓ What Worked</div>
                  <MarkdownCard text={selected.what_worked} />
                </div>
              )}
              {selected.what_failed && (
                <div style={{ marginBottom: 10 }}>
                  <div style={{ fontSize: 12, fontWeight: 600, color: "#ef4444", marginBottom: 4 }}>✗ What Failed</div>
                  <MarkdownCard text={selected.what_failed} />
                </div>
              )}
              {selected.next_action && (
                <div>
                  <div style={{ fontSize: 12, fontWeight: 600, color: "#3b82f6", marginBottom: 4 }}>→ Next Action</div>
                  <MarkdownCard text={selected.next_action} />
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
