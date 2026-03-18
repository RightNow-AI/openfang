import { api } from '../../lib/api-server';
import IntegrationsPageV2 from './IntegrationsPageV2';

export default async function IntegrationsPage() {
  let integrations = [];
  try {
    const data = await api.get('/api/integrations');
    integrations = data?.integrations
      ? data.integrations
      : Array.isArray(data)
      ? data
      : [];
  } catch {
    // Daemon may be offline — render empty
  }
  return <IntegrationsPageV2 initialIntegrations={integrations} />;
}
