import type { RunResult } from "../../../lib/command-center-types";

type Props = {
  results: RunResult[];
};

export default function ResultsFeed({ results }: Props) {
  if (results.length === 0)
    return <p style={{ color: "var(--text-muted, #888)", fontSize: 14 }}>No results yet.</p>;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {results.map((result) => (
        <article
          key={result.id}
          style={{
            padding: "18px 20px",
            border: "1px solid var(--border, #333)",
            borderRadius: 8,
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 10 }}>
            <div>
              <div style={{ fontWeight: 700, fontSize: 16, marginBottom: 2 }}>{result.title}</div>
              <div style={{ fontSize: 12, color: "var(--text-muted, #888)" }}>
                {result.output_type} · completed {new Date(result.completed_at).toLocaleString()}
              </div>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                style={{
                  padding: "4px 12px",
                  background: "var(--success, #22c55e)",
                  color: "#fff",
                  border: "none",
                  borderRadius: 5,
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "pointer",
                }}
              >
                Approve
              </button>
              <button
                style={{
                  padding: "4px 12px",
                  background: "transparent",
                  color: "var(--text-muted, #888)",
                  border: "1px solid var(--border, #333)",
                  borderRadius: 5,
                  fontSize: 12,
                  cursor: "pointer",
                }}
              >
                Archive
              </button>
            </div>
          </div>
          <pre
            style={{
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              fontSize: 13,
              background: "var(--surface2, #1a1a1a)",
              padding: "12px 14px",
              borderRadius: 6,
              margin: 0,
              maxHeight: 300,
              overflowY: "auto",
            }}
          >
            {result.content_markdown}
          </pre>
        </article>
      ))}
    </div>
  );
}
