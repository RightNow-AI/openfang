export type ClientStatus = "active" | "paused" | "at_risk" | "completed";
export type HealthLevel = "green" | "yellow" | "red";
export type TaskStatus =
  | "backlog"
  | "this_week"
  | "today"
  | "waiting"
  | "approved"
  | "running"
  | "done"
  | "blocked"
  | "failed";
export type ApprovalStatus =
  | "needs_review"
  | "approved"
  | "rejected"
  | "changes_requested";
export type ActivityType = "note" | "task" | "approval" | "draft" | "delivery" | "message" | "workflow";
export type RiskSeverity = "low" | "medium" | "high";

export type ClientSummary = {
  id: string;
  name: string;
  industry: string;
  main_goal: string;
  approver_name: string;
  status: ClientStatus;
  health: HealthLevel;
  current_sprint_label: string | null;
  approvals_waiting: number;
  tasks_due_today: number;
  last_activity_at: string | null;
};

export type PriorityItem = {
  id: string;
  title: string;
  owner_label: string;
  due_at: string | null;
  risk_flag: boolean;
  linked_task_id: string | null;
};

export type ApprovalItem = {
  id: string;
  linked_task_id: string;
  title: string;
  reason: string;
  approval_type: "send" | "publish" | "delivery" | "tool_use" | "financial" | "assignment";
  status: ApprovalStatus;
  requested_by: string;
  created_at: string;
  preview_text: string | null;
  tools_involved: string[];
};

export type TaskItem = {
  id: string;
  title: string;
  description: string;
  status: TaskStatus;
  priority: "low" | "medium" | "high";
  owner_type: "human" | "agent";
  owner_label: string;
  due_at: string | null;
  blocked_by_ids: string[];
  unlocks_ids: string[];
  approval_required: boolean;
  estimated_minutes: number | null;
};

export type ActivityItem = {
  id: string;
  type: ActivityType;
  title: string;
  summary: string;
  created_at: string;
  actor_label: string;
};

export type DeadlineItem = {
  id: string;
  title: string;
  type: "deliverable" | "meeting" | "review" | "invoice" | "other";
  due_at: string;
};

export type BrandVoiceSummary = {
  summary: string;
  do_not_say: string[];
  preferred_phrases: string[];
  tone_notes: string[];
};

export type CompetitorSignal = {
  id: string;
  competitor_name: string;
  change_summary: string;
  impact: "low" | "medium" | "high";
  source_label: string;
  detected_at: string;
};

export type ClientMemoryFact = {
  id: string;
  label: string;
  value: string;
  source: "manual" | "agent" | "approval" | "result";
};

export type RiskOrOpportunity = {
  id: string;
  kind: "risk" | "opportunity";
  severity: RiskSeverity;
  title: string;
  description: string;
  suggested_next_step: string | null;
};

export type ResultItem = {
  id: string;
  title: string;
  type: "deliverable" | "email" | "content" | "report" | "file";
  status: "draft" | "ready" | "sent" | "published" | "archived";
  completed_at: string | null;
  url: string | null;
  summary: string;
};

export type FeedbackItem = {
  id: string;
  author_label: string;
  sentiment: "positive" | "neutral" | "negative";
  message: string;
  created_at: string;
};

export type NextActionItem = {
  id: string;
  title: string;
  reason: string;
  type: "task" | "renewal" | "upsell" | "follow_up" | "research";
};

export type FounderWorkspaceItem = {
  workspaceId: string;
  clientId: string;
  name: string;
  companyName: string;
  idea: string;
  stage: string;
  playbookDefaults: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
};

export type FounderRunItem = {
  runId: string;
  workspaceId: string;
  playbookId: string | null;
  prompt: string;
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  summary: string;
  citations: string[];
  nextActions: string[];
  createdAt: string;
  updatedAt: string;
};

export type FounderTaskStatus = 'pending' | 'in_progress' | 'completed' | 'dismissed';

export type FounderTaskItem = {
  taskId: string;
  workspaceId: string;
  runId: string;
  description: string;
  category: string;
  status: FounderTaskStatus;
  createdAt: string;
  updatedAt: string;
};

export type FounderWorkspaceSnapshot = {
  workspace: FounderWorkspaceItem | null;
  runs: FounderRunItem[];
  playbookLabels: Record<string, string>;
};

export type ClientHomeResponse = {
  client: ClientSummary;
  priorities: PriorityItem[];
  approvals_waiting: ApprovalItem[];
  blocked_tasks: TaskItem[];
  recent_activity: ActivityItem[];
  upcoming_deadlines: DeadlineItem[];
  health_summary: {
    level: HealthLevel;
    delivery_confidence: number;
    approval_lag_hours: number | null;
    renewal_likelihood: number | null;
  };
  founder_workspace: FounderWorkspaceItem | null;
};

export type ClientPulseResponse = {
  business_snapshot: {
    offer: string;
    audience: string;
    positioning: string;
    current_objective: string;
    constraints: string[];
  };
  brand_voice: BrandVoiceSummary;
  competitor_signals: CompetitorSignal[];
  project_context: {
    active_campaigns: string[];
    linked_deliverables: string[];
    source_links: string[];
    supporting_documents: string[];
  };
  missing_info: Array<{
    id: string;
    question: string;
    owner_label: string;
    requested_at: string | null;
  }>;
  memory_facts: ClientMemoryFact[];
  risks_and_opportunities: RiskOrOpportunity[];
};

export type ClientPlanResponse = {
  board: {
    backlog: TaskItem[];
    this_week: TaskItem[];
    today: TaskItem[];
    waiting: TaskItem[];
    done: TaskItem[];
  };
  dependencies: Array<{
    task_id: string;
    blocked_by_ids: string[];
    unlocks_ids: string[];
  }>;
  capacity: Array<{
    owner_label: string;
    owner_type: "human" | "agent";
    load_percent: number;
    overloaded: boolean;
  }>;
  approval_needed: TaskItem[];
};

export type ClientApprovalsResponse = {
  needs_review: ApprovalItem[];
  approved: ApprovalItem[];
  rejected: ApprovalItem[];
  changes_requested: ApprovalItem[];
  execution_queue: Array<{
    id: string;
    title: string;
    status: "ready" | "running" | "completed" | "failed" | "timed_out";
    source_approval_id: string | null;
  }>;
  approval_rules: Array<{
    key: "send" | "publish" | "delivery" | "tool_use" | "financial" | "assignment";
    enabled: boolean;
  }>;
};

export type ClientResultsResponse = {
  delivered_outputs: ResultItem[];
  performance_summary: {
    metrics: Array<{
      label: string;
      value: string;
      delta_label: string | null;
    }>;
  };
  lessons_learned: Array<{
    id: string;
    type: "win" | "miss" | "blocker";
    text: string;
  }>;
  feedback: FeedbackItem[];
  next_best_actions: NextActionItem[];
  weekly_review: {
    completed_count: number;
    slipped_count: number;
    summary: string;
  };
  case_study_candidates: Array<{
    id: string;
    title: string;
    proof_point: string;
    quote_candidate: string | null;
  }>;
  founder_workspace: FounderWorkspaceItem | null;
  founder_runs: FounderRunItem[];
};