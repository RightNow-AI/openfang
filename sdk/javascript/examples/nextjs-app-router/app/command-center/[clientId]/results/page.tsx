"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getResults } from "../../../../lib/command-center-api";
import type { RunResult } from "../../../../lib/command-center-types";
import ResultsFeed from "../../components/ResultsFeed";

type Props = {
  params: Promise<{ clientId: string }>;
};

export default function ResultsPage({ params }: Props) {
  const [clientId, setClientId] = useState("");
  const [results, setResults] = useState<RunResult[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    params.then(({ clientId: cid }) => {
      setClientId(cid);
      getResults(cid)
        .then((data) => setResults(data.results))
        .catch((e: Error) => setError(e.message));
    });
  }, [params]);

  if (error) return <main style={{ padding: 24 }}>Error: {error}</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 16, marginBottom: 24 }}>
        <h1 style={{ fontSize: 22, fontWeight: 700 }}>Results</h1>
        {clientId && (
          <Link href={`/command-center/${clientId}`}
            style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>
            ← Back to overview
          </Link>
        )}
      </div>
      <ResultsFeed results={results} />
    </main>
  );
}
