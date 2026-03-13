import { ComingSoon } from '../components/ComingSoon';

export default function AgentCatalogPage() {
  return (
    <ComingSoon
      title="Agent Catalog"
      description="Browse, import, and manage agent templates. Install pre-built agents from the OpenFang catalog or create your own from agent.toml files."
      links={[
        { href: '/sessions', label: 'Running agents' },
        { href: '/overview', label: 'Overview' },
      ]}
    />
  );
}
