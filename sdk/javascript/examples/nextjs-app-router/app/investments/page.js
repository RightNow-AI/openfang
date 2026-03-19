import InvestmentsPage from './InvestmentsPage';

export const metadata = {
  title: 'Investment Intelligence',
  description: 'Track markets, watch patterns, update your thesis, and approve important decisions before anything moves.',
};

export default function Page({ searchParams }) {
  const tab = searchParams?.tab || 'recommended';
  const view = searchParams?.view || 'simple';
  const wizard = searchParams?.wizard === '1' || searchParams?.wizard === 'true';

  return <InvestmentsPage defaultTab={tab} defaultView={view} defaultWizard={wizard} />;
}
