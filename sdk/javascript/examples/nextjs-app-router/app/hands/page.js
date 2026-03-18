import { api } from '../../lib/api-server';
import HandsPageV2 from './HandsPageV2';

export default async function HandsPage() {
  let hands = [];
  try {
    const data = await api.get('/api/hands');
    hands = Array.isArray(data?.hands) ? data.hands : Array.isArray(data) ? data : [];
  } catch {
    // handled by client error state
  }
  return <HandsPageV2 initialHands={hands} />;
}

