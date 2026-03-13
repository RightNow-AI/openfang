import { ComingSoon } from '../components/ComingSoon';

export default function InboxPage() {
  return (
    <ComingSoon
      title="Inbox"
      description="Your agent inbox collects tasks, messages, and notifications that need your attention. Items routed here become inputs for today's plan."
      links={[
        { href: '/today', label: 'View today plan' },
        { href: '/sessions', label: 'View agents' },
      ]}
    />
  );
}
