import { api } from '../../lib/api-server';
import CommsPageV2 from './CommsPageV2';

export default async function CommsPage() {
  let topology = { nodes: [], edges: [] };
  let events = [];
  try {
    const [topoData, eventsData] = await api.gather([
      '/api/comms/topology',
      '/api/comms/events?limit=50',
    ]);
    if (topoData) topology = topoData;
    if (Array.isArray(eventsData)) events = eventsData;
  } catch {
    // handled by client error state
  }
  return <CommsPageV2 initialTopology={topology} initialEvents={events} />;
}
