import { api } from '../../lib/api-server';
import ChannelsClient from './ChannelsClient';

export default async function ChannelsPage() {
  let channels = [];
  try {
    const data = await api.get('/api/channels');
    channels = Array.isArray(data) ? data : (data?.channels ?? []);
  } catch {
    // error shown by client
  }
  return <ChannelsClient initialChannels={channels} />;
}

