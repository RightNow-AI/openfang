import { api } from '../../lib/api-server';
import RuntimeClient from './RuntimeClient';

function normalizeRuntime(h, s, n, p) {
  const health = h ?? {};
  const status = s ?? {};
  const network = n ?? {};
  const peersRaw = Array.isArray(p) ? p : p?.peers ?? [];
  return {
    isUp: health.status === 'ok' || health.healthy === true,
    version: health.version ?? status.version ?? '',
    uptimeSec: health.uptime_seconds ?? status.uptime_seconds ?? null,
    agentCount: status.agent_count ?? status.agents ?? 0,
    networkUp: network.connected ?? network.status === 'connected' ?? false,
    nodeId: network.node_id ?? '',
    peerCount: network.peer_count ?? peersRaw.length,
    peers: peersRaw.map(peer => ({
      id: peer?.id ?? peer?.peer_id ?? '',
      address: peer?.address ?? peer?.addr ?? '',
      latencyMs: peer?.latency_ms ?? null,
      status: peer?.status ?? 'unknown',
    })),
  };
}

export default async function RuntimePage() {
  const [h, s, n, p] = await api.gather([
    '/api/health', '/api/status', '/api/network/status', '/api/peers',
  ]);
  const runtime = normalizeRuntime(h, s, n, p);
  return <RuntimeClient initialRuntime={runtime} />;
}

