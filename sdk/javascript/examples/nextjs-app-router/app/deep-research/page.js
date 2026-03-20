import DeepResearchClient from './DeepResearchClient';

export const metadata = {
  title: 'Deep Research | OpenFang',
  description: 'Multi-step agent research with source synthesis and structured reporting',
};

export default function DeepResearchPage() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <DeepResearchClient />
    </div>
  );
}
