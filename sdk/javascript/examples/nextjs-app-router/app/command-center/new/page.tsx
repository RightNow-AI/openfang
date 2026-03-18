import CommandCenterWizard from "../components/CommandCenterWizard";

export default function NewCommandCenterPage() {
  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 4 }}>Command Center</h1>
      <p style={{ color: "var(--text-muted, #888)", marginBottom: 28 }}>
        Set up the client, choose the work, then approve execution.
      </p>
      <CommandCenterWizard />
    </main>
  );
}
