import { ComingSoon } from '../components/ComingSoon';

export default function HandsPage() {
  return (
    <ComingSoon
      title="Hands"
      description="Browser automation, computer-use, and tool execution. Hands let agents take actions in the real world — clicking, typing, reading screens."
      links={[
        { href: '/skills', label: 'Skills' },
        { href: '/sessions', label: 'Agent sessions' },
      ]}
    />
  );
}
