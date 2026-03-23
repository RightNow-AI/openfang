import { notFound } from 'next/navigation';

import StudioWorkspaceDashboard from '../components/StudioWorkspaceDashboard';
import { getStudioWorkspaceDashboard } from '../lib/studio-data';

type Props = {
  params: Promise<{ workspaceId: string }>;
};

export default async function StudioWorkspaceRoute({ params }: Props) {
  const { workspaceId } = await params;
  const payload = await getStudioWorkspaceDashboard(workspaceId);
  if (!payload) notFound();
  return <StudioWorkspaceDashboard payload={payload} />;
}
