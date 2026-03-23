import StudioLandingPage from './components/StudioLandingPage';
import { getStudioIndex } from './lib/studio-data';

export const metadata = {
  title: 'Studio Pipeline - OpenFang',
  description: 'Stage-based creator studio for workspaces, drafts, jobs, and approvals.',
};

export default async function StudioRoute() {
  const payload = await getStudioIndex();
  return <StudioLandingPage payload={payload} />;
}
