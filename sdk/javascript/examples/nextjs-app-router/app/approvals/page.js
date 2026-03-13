import { ComingSoon } from '../components/ComingSoon';

export default function ApprovalsPage() {
  return (
    <ComingSoon
      title="Approvals"
      description="Human-in-the-loop tasks waiting for your decision. When agents request permission to take a consequential action, approval requests appear here."
      links={[
        { href: '/sessions', label: 'View agent sessions' },
        { href: '/logs', label: 'Audit logs' },
      ]}
    />
  );
}
