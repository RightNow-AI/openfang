export type ApprovalMode = "none" | "required" | "conditional";
export type TaskStatus =
  | "draft"
  | "pending_approval"
  | "approved"
  | "running"
  | "completed"
  | "failed";

export type ApprovalStatus = "none" | "pending" | "approved" | "rejected";

export type TaskType =
  | "summarize_business"
  | "research_competitors"
  | "draft_outreach_emails"
  | "assign_followup_chores"
  | "prepare_weekly_task_plan";

export type ClientProfile = {
  id: string;
  client_name: string;
  business_name: string;
  industry: string;
  main_goal: string;
  website_url: string;
  offer: string;
  customer: string;
  notes: string;
  approval_mode: ApprovalMode;
  approvers: Array<{ name: string; email: string }>;
  require_approval_for_email: boolean;
  require_approval_for_tool_use: boolean;
  require_approval_for_assignment: boolean;
  created_at: string;
  updated_at: string;
};

export type PlannedTask = {
  id: string;
  client_id: string;
  title: string;
  type: TaskType;
  status: TaskStatus;
  priority: "low" | "medium" | "high";
  assigned_agent: string;
  required_tools: string[];
  approval_required: boolean;
  approval_status: ApprovalStatus;
  input_snapshot: Record<string, unknown>;
};

export type ApprovalItem = {
  id: string;
  task_id: string;
  client_id: string;
  requested_by: string;
  status: ApprovalStatus;
  preview_summary: string;
  tool_actions: string[];
};

export type RunResult = {
  id: string;
  task_id: string;
  client_id: string;
  status: "completed" | "failed";
  output_type: string;
  title: string;
  content_markdown: string;
  started_at: string;
  completed_at: string;
};

export type GeneratePlanRequest = {
  client_id: string;
  selected_task_types: TaskType[];
};

export const TASK_CATALOG: Array<{
  id: TaskType;
  title: string;
  description: string;
  requiresApproval: boolean;
  assignedAgent: string;
  estimatedTime: string;
}> = [
  {
    id: "summarize_business",
    title: "Summarize business",
    description: "Turn the intake into a clean operating summary.",
    requiresApproval: false,
    assignedAgent: "Business Context Agent",
    estimatedTime: "1 min",
  },
  {
    id: "research_competitors",
    title: "Research competitors",
    description: "Find positioning gaps and competitor patterns.",
    requiresApproval: false,
    assignedAgent: "Research Agent",
    estimatedTime: "2 min",
  },
  {
    id: "draft_outreach_emails",
    title: "Draft outreach emails",
    description: "Create approval-ready outreach drafts.",
    requiresApproval: true,
    assignedAgent: "Email Agent",
    estimatedTime: "2 min",
  },
  {
    id: "assign_followup_chores",
    title: "Assign follow-up chores",
    description: "Break work into task assignments for people or agents.",
    requiresApproval: true,
    assignedAgent: "Ops Agent",
    estimatedTime: "1 min",
  },
  {
    id: "prepare_weekly_task_plan",
    title: "Prepare weekly task plan",
    description: "Build a weekly plan with owners and deadlines.",
    requiresApproval: false,
    assignedAgent: "Task Planner Agent",
    estimatedTime: "1 min",
  },
];
