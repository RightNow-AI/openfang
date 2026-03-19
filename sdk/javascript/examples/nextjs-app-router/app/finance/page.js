import FinancePage from './FinancePage';

export const metadata = {
  title: 'Finance — OpenFang',
  description: 'Business finance layer: approvals, decisions, and money signals.',
};

export default async function Page({ searchParams }) {
  const params = await searchParams;
  const tab = params?.tab || null;
  const view = params?.view || null;
  const wizard = params?.wizard === '1';

  return (
    <FinancePage
      initialTab={tab}
      initialView={view}
      autoOpenWizard={wizard}
    />
  );
}
