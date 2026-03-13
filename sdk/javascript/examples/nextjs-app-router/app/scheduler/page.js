import { ComingSoon } from '../components/ComingSoon';

export default function SchedulerPage() {
  return (
    <ComingSoon
      title="Scheduler"
      description="Schedule recurring agent tasks, reminders, and automated workflows. Set cron-style triggers or event-based conditions."
      links={[
        { href: '/workflows', label: 'Workflows' },
        { href: '/today', label: 'Today plan' },
      ]}
    />
  );
}
