"use client";

import GrowthWizard from "../components/GrowthWizard";

export default function GrowthNewPage() {
  return (
    <main>
      <div style={{ padding: "24px 32px 8px", borderBottom: "1px solid var(--border, #333)" }}>
        <h1 style={{ fontSize: 22, fontWeight: 700 }}>Growth Mode</h1>
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14, marginTop: 4 }}>
          Launch a demand-generation campaign — research, creative, video ads, publishing, and performance.
        </p>
      </div>
      <GrowthWizard />
    </main>
  );
}
