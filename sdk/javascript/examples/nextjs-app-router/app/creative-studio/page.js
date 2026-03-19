import CreativeStudioPage from './CreativeStudioPage';

export const metadata = {
  title: 'Creative Studio — OpenFang',
  description: 'Generate images, videos, and full creative campaigns with AI',
};

export default async function CreativeStudioRoute() {
  // Optimistic initial load — if the API is unavailable the client handles it gracefully
  let initialProjects = [];
  try {
    const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';
    const res = await fetch(`${BASE}/api/creative-projects`, { cache: 'no-store' });
    if (res.ok) {
      const data = await res.json();
      initialProjects = Array.isArray(data) ? data : data?.items ?? [];
    }
  } catch {
    // daemon not running — client will fetch when switching tabs
  }
  return <CreativeStudioPage initialProjects={initialProjects} />;
}
