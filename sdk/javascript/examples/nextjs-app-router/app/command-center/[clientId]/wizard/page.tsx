import CommandCenterWizard from "../../components/CommandCenterWizard";

type Props = {
  params: Promise<{ clientId: string }>;
};

export default async function ExistingWizardPage({ params }: Props) {
  const { clientId } = await params;
  return (
    <main style={{ padding: "24px 32px", maxWidth: 960, margin: "0 auto" }}>
      <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 4 }}>Command Center</h1>
      <CommandCenterWizard initialClientId={clientId} />
    </main>
  );
}
