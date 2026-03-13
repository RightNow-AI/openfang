import { ComingSoon } from '../components/ComingSoon';

export default function CommsPage() {
  return (
    <ComingSoon
      title="Comms"
      description="Agent-to-agent and agent-to-human communication threads. Monitor cross-agent conversations, A2A task exchanges, and outbound messages."
      links={[
        { href: '/channels', label: 'Channels' },
        { href: '/logs', label: 'Audit logs' },
      ]}
    />
  );
}
