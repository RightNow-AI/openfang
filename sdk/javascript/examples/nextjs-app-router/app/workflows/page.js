import { ComingSoon } from '../components/ComingSoon';

export default function WorkflowsPage() {
  return (
    <ComingSoon
      title="Workflows"
      description="Define and run multi-step automation workflows. Chain agent tasks, conditional logic, and external tool calls into repeatable pipelines."
      links={[
        { href: '/scheduler', label: 'Scheduler' },
        { href: '/skills', label: 'Skills' },
      ]}
    />
  );
}
