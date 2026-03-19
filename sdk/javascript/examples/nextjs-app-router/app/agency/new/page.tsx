"use client";

import AgencyWizard from "../components/AgencyWizard";

export default function AgencyNewPage() {
  return (
    <main>
      <div style={{ padding: "24px 32px 8px", borderBottom: "1px solid var(--border)" }}>
        <h1 style={{ fontSize: 22, fontWeight: 700 }}>Agency Mode</h1>
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14, marginTop: 4 }}>
          Set up a new client engagement — intake, scope, research, draft, delivery.
        </p>
      </div>
      <AgencyWizard />
    </main>
  );
}
