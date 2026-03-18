"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getModeResults } from "../../../../lib/mode-api";
import type { ModeResult } from "../../../../lib/mode-types";

type Props = { params: Promise<{ clientId: string }> };

export default function AgencyResultsPage({ params }: Props) {
  const [clientId, setClientId] = useState("");
  const [results, setResults]   = useState<ModeResult[]>([]);
  const [active, setActive]     = useState<ModeResult | null>(null);
  const [error, setError]       = useState("");

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      getModeResults("agency", cid)
        .then((r) => setResults(r.results))
        .catch((e: Error) => setError(e.message));
    });
  }, [params]);

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <div style={{ marginBottom: 20 }}>
        <Link href={`/agency/${clientId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Overview</Link>
        <h1 style={{ fontSize: 22, fontWeight: 700, marginTop: 8 }}>Results</h1>
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>Outputs from completed agency tasks.</p>
      </div>

      {results.length === 0 ? (
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>No results yet. Run tasks to generate outputs.</p>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: active ? "1fr 1fr" : "1fr", gap: 16 }}>
          <div>
            {results.map((r) => (
              <div
                key={r.id}
                onClick={() => setActive(r)}
                style={{
                  border: `1px solid ${active?.id === r.id ? "var(--accent, #7c3aed)" : "var(--border, #333)"}`,
                  borderRadius: 8, padding: "12px 16px", marginBottom: 8, cursor: "pointer",
                  background: active?.id === r.id ? "rgba(124,58,237,0.08)" : "transparent",
                }}
              >
                <div style={{ fontWeight: 600, fontSize: 15, marginBottom: 4 }}>{r.title}</div>
                <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>
                  {r.owner} · {new Date(r.completed_at).toLocaleDateString()}
                </div>
                {r.next_action && (
                  <div style={{ fontSize: 12, color: "#a78bfa", marginTop: 4 }}>Next: {r.next_action}</div>
                )}
              </div>
            ))}
          </div>

          {active && (
            <div style={{ border: "1px solid var(--border, #333)", borderRadius: 8, padding: 20, position: "sticky", top: 24, maxHeight: "80vh", overflowY: "auto" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
                <h2 style={{ fontSize: 16, fontWeight: 700 }}>{active.title}</h2>
                <button onClick={() => setActive(null)} style={{ background: "none", border: "none", cursor: "pointer", fontSize: 20, color: "var(--text-muted, #888)" }}>×</button>
              </div>
              <pre style={{ fontSize: 13, whiteSpace: "pre-wrap", lineHeight: 1.6 }}>{active.content_markdown}</pre>
              {active.what_worked && (
                <div style={{ marginTop: 16, padding: "10px 12px", borderRadius: 6, background: "rgba(34,197,94,0.1)", fontSize: 13 }}>
                  <strong>What worked:</strong> {active.what_worked}
                </div>
              )}
              {active.what_failed && (
                <div style={{ marginTop: 8, padding: "10px 12px", borderRadius: 6, background: "rgba(239,68,68,0.1)", fontSize: 13 }}>
                  <strong>What failed:</strong> {active.what_failed}
                </div>
              )}
              {active.next_action && (
                <div style={{ marginTop: 8, padding: "10px 12px", borderRadius: 6, background: "rgba(167,139,250,0.1)", fontSize: 13 }}>
                  <strong>Next action:</strong> {active.next_action}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </main>
  );
}
