import { Suspense } from 'react';
import DeepResearchClient from './DeepResearchClient';

export const metadata = {
  title: 'Deep Research | OpenFang',
  description: 'Multi-step agent research with source synthesis and structured reporting',
};

function DeepResearchPageFallback() {
  return (
    <div style={{ padding: 24, color: 'var(--text-dim)' }}>
      Loading research workspace...
    </div>
  );
}

export default function DeepResearchPage() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Keep this Suspense boundary: DeepResearchClient reads useSearchParams(), and Next.js
          requires a page-level boundary here to avoid prerender failures in production builds. */}
      <Suspense fallback={<DeepResearchPageFallback />}>
        <DeepResearchClient />
      </Suspense>
    </div>
  );
}
