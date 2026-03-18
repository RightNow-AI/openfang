// ────────────────────────────────────────────────────────────────────────────
// Shared mode primitives
// ────────────────────────────────────────────────────────────────────────────

export type BusinessMode = "agency" | "growth" | "school";

export type ApprovalType =
  | "draft_approval"
  | "tool_use_approval"
  | "send_approval"
  | "publish_approval"
  | "client_delivery_approval"
  | "student_facing_content_approval"
  | "spend_approval";

export type TaskStatus =
  | "draft"
  | "pending_approval"
  | "approved"
  | "running"
  | "completed"
  | "failed";

export type ApprovalStatus = "none" | "pending" | "approved" | "rejected";

export type Priority = "low" | "medium" | "high" | "critical";

// ────────────────────────────────────────────────────────────────────────────
// Shared record — every mode uses this as its "entity" (client, campaign, program)
// ────────────────────────────────────────────────────────────────────────────

export type ModeRecord = {
  id: string;
  mode: BusinessMode;
  title: string;           // client name | campaign name | program name
  subtitle: string;        // business name | audience | cohort
  goal: string;
  status: "active" | "draft" | "archived";
  created_at: string;
  updated_at: string;
  meta: Record<string, unknown>; // mode-specific extra fields
};

// ────────────────────────────────────────────────────────────────────────────
// Shared task
// ────────────────────────────────────────────────────────────────────────────

export type ModeTask = {
  id: string;
  record_id: string;       // parent ModeRecord.id
  mode: BusinessMode;
  catalog_id: string;      // from mode task catalog
  title: string;
  assigned_agent: string;
  required_tools: string[];
  approval_required: boolean;
  approval_type: ApprovalType | null;
  status: TaskStatus;
  approval_status: ApprovalStatus;
  priority: Priority;
  depends_on: string[];    // task ids
  output_summary: string;
};

// ────────────────────────────────────────────────────────────────────────────
// Shared approval
// ────────────────────────────────────────────────────────────────────────────

export type ModeApproval = {
  id: string;
  task_id: string;
  record_id: string;
  mode: BusinessMode;
  approval_type: ApprovalType;
  requested_by: string;
  status: ApprovalStatus;
  preview_summary: string;
  tool_actions: string[];
  created_at: string;
};

// ────────────────────────────────────────────────────────────────────────────
// Shared result
// ────────────────────────────────────────────────────────────────────────────

export type ModeResult = {
  id: string;
  task_id: string;
  record_id: string;
  mode: BusinessMode;
  title: string;
  output_type: string;
  content_markdown: string;
  what_worked: string;
  what_failed: string;
  next_action: string;
  owner: string;
  status: "completed" | "failed";
  started_at: string;
  completed_at: string;
};

// ────────────────────────────────────────────────────────────────────────────
// Task catalog entry (template)
// ────────────────────────────────────────────────────────────────────────────

export type TaskTemplate = {
  id: string;
  mode: BusinessMode;
  title: string;
  description: string;
  screen: 1 | 2 | 3 | 4 | 5;
  assigned_agent: string;
  required_tools: string[];
  default_approval_type: ApprovalType | null;
  needs_approval_by_default: boolean;
  /** Live override — whether this task instance requires approval (defaults to needs_approval_by_default when absent). */
  approval_required?: boolean;
};

// ────────────────────────────────────────────────────────────────────────────
// Finance layer
// ────────────────────────────────────────────────────────────────────────────

export type FinanceSummary = {
  record_id: string;
  mode: BusinessMode;
  ltv_estimate: number;
  cac_estimate: number;
  margin_pct: number;
  renewal_date: string | null;
  revenue_this_period: number;
  currency: string;
};

// ────────────────────────────────────────────────────────────────────────────
// AGENCY TYPES
// ────────────────────────────────────────────────────────────────────────────

export type AgencyClient = ModeRecord & {
  meta: {
    // Screen 1 fields
    service_requested: string;
    deadline: string;
    budget_band: string;
    point_of_contact: string;
    approval_owner: string;
    // Screen 2 fields
    business_summary: string;
    offer_summary: string;
    audience_summary: string;
    competitor_list: string;
    existing_assets: string;
    constraints: string;
    brand_voice_notes: string;
    // approval settings
    require_draft_approval: boolean;
    require_tool_use_approval: boolean;
    require_send_approval: boolean;
    require_publish_approval: boolean;
    require_handoff_approval: boolean;
  };
};

export const AGENCY_TASK_CATALOG: TaskTemplate[] = [
  { id: "intake_client_brief",      mode: "agency", title: "Create Client Brief",          description: "Turns answers into a structured brief",               screen: 1, assigned_agent: "Intake Agent",           required_tools: [],                               default_approval_type: null,                     needs_approval_by_default: false },
  { id: "scope_service",            mode: "agency", title: "Scope Service Request",         description: "Clarifies gaps and defines deliverables",              screen: 1, assigned_agent: "Scope Agent",             required_tools: [],                               default_approval_type: null,                     needs_approval_by_default: false },
  { id: "summarize_business",       mode: "agency", title: "Summarize Business Context",    description: "Reads website, docs, and notes",                       screen: 2, assigned_agent: "Business Context Agent",  required_tools: ["website_summarizer"],           default_approval_type: null,                     needs_approval_by_default: false },
  { id: "research_competitors",     mode: "agency", title: "Research Competitors",          description: "Finds market clues and proof points",                  screen: 2, assigned_agent: "Research Agent",          required_tools: ["web_search", "scraper"],        default_approval_type: null,                     needs_approval_by_default: false },
  { id: "build_brand_voice",        mode: "agency", title: "Build Brand Voice Guide",       description: "Creates voice guardrails from samples",                screen: 2, assigned_agent: "Brand Voice Agent",       required_tools: [],                               default_approval_type: null,                     needs_approval_by_default: false },
  { id: "build_delivery_plan",      mode: "agency", title: "Build Delivery Plan",           description: "Breaks service request into task list",                screen: 3, assigned_agent: "Task Planner Agent",      required_tools: ["task_planner"],                 default_approval_type: null,                     needs_approval_by_default: false },
  { id: "assign_tasks",             mode: "agency", title: "Assign Tasks to Agents",        description: "Routes tasks to best agent or human",                  screen: 3, assigned_agent: "Assignment Agent",        required_tools: [],                               default_approval_type: null,                     needs_approval_by_default: false },
  { id: "draft_client_copy",        mode: "agency", title: "Draft Client-Facing Content",   description: "Creates drafts for client review",                     screen: 4, assigned_agent: "Writer Agent",            required_tools: ["copy_generator"],               default_approval_type: "draft_approval",         needs_approval_by_default: true },
  { id: "send_client_email",        mode: "agency", title: "Draft and Send Client Email",   description: "Drafts and sends through MCP email",                   screen: 4, assigned_agent: "Email Agent",             required_tools: ["mcp_email"],                    default_approval_type: "send_approval",          needs_approval_by_default: true },
  { id: "package_delivery",         mode: "agency", title: "Package Delivery",              description: "Bundles outputs for client handoff",                   screen: 4, assigned_agent: "Approval Agent",          required_tools: [],                               default_approval_type: "client_delivery_approval", needs_approval_by_default: true },
  { id: "capture_followups",        mode: "agency", title: "Capture Follow-up Tasks",       description: "Logs next actions and renewal signals",                screen: 5, assigned_agent: "Account Manager Agent",   required_tools: ["task_logger"],                  default_approval_type: null,                     needs_approval_by_default: false },
  { id: "identify_upsells",         mode: "agency", title: "Identify Upsell Opportunities", description: "Finds expansion offers from delivery context",          screen: 5, assigned_agent: "Upsell Agent",            required_tools: [],                               default_approval_type: null,                     needs_approval_by_default: false },
];

// ────────────────────────────────────────────────────────────────────────────
// GROWTH TYPES
// ────────────────────────────────────────────────────────────────────────────

export type GrowthCampaign = ModeRecord & {
  meta: {
    // Screen 1 fields
    campaign_goal: string;
    offer: string;
    audience: string;
    channel: string;
    budget: string;
    cta: string;
    timeline: string;
    brand_rules: string;
    // Screen 2 fields
    competitor_ads: string;
    top_hooks: string;
    existing_creatives: string;
    winning_offers: string;
    audience_pain_points: string;
    objections: string;
    proof_assets: string;
    // approval settings
    require_spend_approval: boolean;
    require_publish_approval: boolean;
  };
};

// Video Ad Studio sub-flow step
export type VideoAdStep =
  | "brief"
  | "hook"
  | "script"
  | "shot_list"
  | "asset_request"
  | "video_draft"
  | "approval"
  | "publish"
  | "measure";

export type VideoAdProject = {
  id: string;
  campaign_id: string;
  title: string;
  step: VideoAdStep;
  brief: string;
  hook: string;
  script: string;
  shot_list: string;
  asset_notes: string;
  approval_status: ApprovalStatus;
  performance: {
    ctr: number;
    cpc: number;
    cpa: number;
    roas: number;
    watch_time_pct: number;
  } | null;
  created_at: string;
};

export const GROWTH_TASK_CATALOG: TaskTemplate[] = [
  { id: "write_campaign_brief",       mode: "growth", title: "Write Campaign Brief",          description: "Captures goal, offer, audience, and rules",           screen: 1, assigned_agent: "Growth Strategist",       required_tools: [],                               default_approval_type: null,                  needs_approval_by_default: false },
  { id: "sharpen_offer",              mode: "growth", title: "Sharpen Offer",                 description: "Clarifies value proposition",                          screen: 1, assigned_agent: "Offer Agent",             required_tools: [],                               default_approval_type: null,                  needs_approval_by_default: false },
  { id: "research_competitor_ads",    mode: "growth", title: "Research Competitor Ads",       description: "Scans ad library for patterns and formats",            screen: 2, assigned_agent: "Competitor Research Agent",required_tools: ["web_search", "ad_library"],     default_approval_type: null,                  needs_approval_by_default: false },
  { id: "build_creative_intelligence",mode: "growth", title: "Build Creative Intelligence Pack", description: "Hooks, angles, proof, and objections",             screen: 2, assigned_agent: "Ad Intelligence Agent",   required_tools: ["web_search"],                   default_approval_type: null,                  needs_approval_by_default: false },
  { id: "generate_hooks",             mode: "growth", title: "Generate Hook List",            description: "Creates 10+ hooks by angle type",                     screen: 3, assigned_agent: "Hook Writer Agent",        required_tools: ["copy_generator"],               default_approval_type: "draft_approval",      needs_approval_by_default: true },
  { id: "develop_angles",             mode: "growth", title: "Develop Angles",               description: "Finds strong positioning angles",                      screen: 3, assigned_agent: "Angle Analyst Agent",     required_tools: [],                               default_approval_type: "draft_approval",      needs_approval_by_default: true },
  { id: "write_scripts",              mode: "growth", title: "Write Ad Scripts",             description: "Creates video scripts and copy variants",              screen: 3, assigned_agent: "Script Writer Agent",     required_tools: ["copy_generator"],               default_approval_type: "draft_approval",      needs_approval_by_default: true },
  { id: "write_email_variants",       mode: "growth", title: "Write Email Variants",         description: "Email copy for sequence",                              screen: 3, assigned_agent: "Copy Agent",             required_tools: ["mcp_email"],                    default_approval_type: "draft_approval",      needs_approval_by_default: true },
  { id: "video_studio_flow",          mode: "growth", title: "Video Ad Studio",              description: "Full brief→hook→script→shot→draft→publish loop",       screen: 4, assigned_agent: "Video Production Agent",  required_tools: ["video_toolkit", "asset_store"], default_approval_type: "publish_approval",    needs_approval_by_default: true },
  { id: "design_statics",             mode: "growth", title: "Design Static Assets",         description: "Thumbnails, statics, overlays",                        screen: 4, assigned_agent: "Design Agent",           required_tools: ["design_toolkit"],               default_approval_type: "publish_approval",    needs_approval_by_default: true },
  { id: "creative_qa",                mode: "growth", title: "Creative QA",                  description: "Brand, claim, and clarity check",                      screen: 4, assigned_agent: "Creative QA Agent",       required_tools: [],                               default_approval_type: "draft_approval",      needs_approval_by_default: true },
  { id: "publish_assets",             mode: "growth", title: "Publish Approved Assets",      description: "Moves assets to channel workflows",                    screen: 4, assigned_agent: "Publishing Agent",        required_tools: ["channel_publisher"],            default_approval_type: "publish_approval",    needs_approval_by_default: true },
  { id: "read_performance",           mode: "growth", title: "Read Performance Data",        description: "Pull CTR, CPA, ROAS, watch time",                      screen: 5, assigned_agent: "Performance Analyst",     required_tools: ["analytics_api"],                default_approval_type: null,                  needs_approval_by_default: false },
  { id: "build_optimization_plan",    mode: "growth", title: "Build Optimization Plan",      description: "Next moves based on results",                          screen: 5, assigned_agent: "Optimization Agent",      required_tools: [],                               default_approval_type: null,                  needs_approval_by_default: false },
  { id: "plan_next_experiments",      mode: "growth", title: "Plan Next Experiments",        description: "Next batch of test variants",                          screen: 5, assigned_agent: "Experiment Planner",      required_tools: [],                               default_approval_type: "spend_approval",      needs_approval_by_default: true },
];

// ────────────────────────────────────────────────────────────────────────────
// SCHOOL TYPES
// ────────────────────────────────────────────────────────────────────────────

export type SchoolProgram = ModeRecord & {
  meta: {
    // Screen 1 fields
    who_its_for: string;
    outcome: string;
    duration: string;
    delivery_style: "cohort" | "evergreen" | "hybrid";
    price: string;
    support_model: string;
    approval_owner: string;
    // Screen 2 fields
    student_type: string;
    skill_level: string;
    desired_outcome: string;
    competitor_programs: string;
    curriculum_goals: string;
    community_needs: string;
    // approval settings
    require_student_facing_approval: boolean;
    require_publish_approval: boolean;
  };
};

export type StudentHealthRecord = {
  student_id: string;
  program_id: string;
  attendance_pct: number;
  assignment_completion_pct: number;
  community_participation_score: number; // 0-10
  support_requests: number;
  risk_score: number;       // 0-100, higher = at risk
  upsell_readiness: number; // 0-100
  last_seen: string;
  flags: Array<{ type: "at_risk" | "upsell_ready" | "star_student"; note: string }>;
};

export const SCHOOL_TASK_CATALOG: TaskTemplate[] = [
  { id: "define_program_brief",      mode: "school", title: "Define Program Brief",           description: "Sets outcome, format, and promise",                   screen: 1, assigned_agent: "Program Architect",       required_tools: [],                               default_approval_type: null,                              needs_approval_by_default: false },
  { id: "sharpen_enrollment_offer",  mode: "school", title: "Sharpen Enrollment Offer",        description: "Clarifies positioning and enrollment message",         screen: 1, assigned_agent: "Offer Agent",             required_tools: [],                               default_approval_type: null,                              needs_approval_by_default: false },
  { id: "map_student_needs",         mode: "school", title: "Map Student Needs",              description: "Surveys student type, pain, and desired outcome",      screen: 2, assigned_agent: "Student Research Agent",   required_tools: ["survey_tool", "web_search"],    default_approval_type: null,                              needs_approval_by_default: false },
  { id: "build_curriculum_outline",  mode: "school", title: "Build Curriculum Outline",        description: "Modules, lessons, and flow",                           screen: 2, assigned_agent: "Curriculum Architect",    required_tools: [],                               default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "write_lessons",             mode: "school", title: "Write Lessons",                  description: "Full lesson drafts with session plans",                screen: 3, assigned_agent: "Lesson Builder Agent",     required_tools: ["copy_generator"],               default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "design_assignments",        mode: "school", title: "Design Assignments",             description: "Practice work, rubrics, grading guides",               screen: 3, assigned_agent: "Assignment Designer Agent",required_tools: [],                               default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "build_resources",           mode: "school", title: "Build Course Resources",         description: "Worksheets, templates, guides",                        screen: 3, assigned_agent: "Resource Builder Agent",   required_tools: [],                               default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "run_cohort_onboarding",     mode: "school", title: "Run Cohort Onboarding",          description: "Onboarding tasks, welcome sequence, calendar",         screen: 4, assigned_agent: "Cohort Ops Agent",        required_tools: ["task_logger", "mcp_email"],     default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "send_email_reminders",      mode: "school", title: "Send Reminders and Nudges",      description: "Assignment reminders, check-in emails",                screen: 4, assigned_agent: "Email Agent",             required_tools: ["mcp_email"],                    default_approval_type: "send_approval",                   needs_approval_by_default: true },
  { id: "review_assignments",        mode: "school", title: "Review Assignments",             description: "Grade and return student work",                        screen: 4, assigned_agent: "Approval Agent",          required_tools: [],                               default_approval_type: "student_facing_content_approval", needs_approval_by_default: true },
  { id: "track_student_health",      mode: "school", title: "Track Student Health",           description: "Attendance, completion, risk scoring",                 screen: 5, assigned_agent: "Student Success Agent",    required_tools: ["analytics_api"],                default_approval_type: null,                              needs_approval_by_default: false },
  { id: "capture_testimonials",      mode: "school", title: "Capture Testimonials",           description: "Request and log student wins",                         screen: 5, assigned_agent: "Coach Support Agent",     required_tools: ["survey_tool", "mcp_email"],     default_approval_type: null,                              needs_approval_by_default: false },
  { id: "find_upsells",              mode: "school", title: "Find Upsell Opportunities",      description: "Identifies expansion-ready students",                  screen: 5, assigned_agent: "Upsell Agent",            required_tools: [],                               default_approval_type: null,                              needs_approval_by_default: false },
];

// ────────────────────────────────────────────────────────────────────────────
// Agent rosters per mode
// ────────────────────────────────────────────────────────────────────────────

export type AgentBundleEntry = { name: string; role: string; screen: 1 | 2 | 3 | 4 | 5 };

export const AGENCY_AGENTS: AgentBundleEntry[] = [
  { name: "Intake Agent",           role: "Turns answers into structured client profile",     screen: 1 },
  { name: "Scope Agent",            role: "Clarifies service request and gaps",                screen: 1 },
  { name: "Business Context Agent", role: "Reads website, docs, and notes",                   screen: 2 },
  { name: "Research Agent",         role: "Finds competitors, market clues, proof points",    screen: 2 },
  { name: "Brand Voice Agent",      role: "Builds voice guardrails",                          screen: 2 },
  { name: "Task Planner Agent",     role: "Breaks service request into execution plan",       screen: 3 },
  { name: "Assignment Agent",       role: "Assigns best agent or human",                      screen: 3 },
  { name: "Writer Agent",           role: "Creates client drafts",                            screen: 4 },
  { name: "Email Agent",            role: "Drafts or sends through MCP email",                screen: 4 },
  { name: "Approval Agent",         role: "Packages work for review",                         screen: 4 },
  { name: "Ops Agent",              role: "Moves tasks through the system",                   screen: 4 },
  { name: "Account Manager Agent",  role: "Handles follow-ups, renewals, and retention",      screen: 5 },
  { name: "Upsell Agent",           role: "Finds expansion offers",                           screen: 5 },
];

export const GROWTH_AGENTS: AgentBundleEntry[] = [
  { name: "Growth Strategist",         role: "Defines goal, offer, and channel plan",           screen: 1 },
  { name: "Offer Agent",               role: "Sharpens value proposition",                      screen: 1 },
  { name: "Competitor Research Agent", role: "Pulls market and ad signals",                     screen: 2 },
  { name: "Ad Intelligence Agent",     role: "Studies ad patterns and formats",                 screen: 2 },
  { name: "Voice Agent",               role: "Applies brand tone to all assets",                screen: 2 },
  { name: "Hook Writer Agent",         role: "Generates hooks by angle type",                   screen: 3 },
  { name: "Angle Analyst Agent",       role: "Finds strong positioning angles",                 screen: 3 },
  { name: "Script Writer Agent",       role: "Creates ad scripts and video copy",               screen: 3 },
  { name: "Copy Agent",                role: "Writes emails, captions, landing pages",          screen: 3 },
  { name: "Video Production Agent",    role: "Builds video drafts and variants",                screen: 4 },
  { name: "Design Agent",              role: "Generates statics, thumbs, overlays",             screen: 4 },
  { name: "Creative QA Agent",         role: "Checks brand, claim, and clarity",                screen: 4 },
  { name: "Publishing Agent",          role: "Moves approved assets to channels",               screen: 4 },
  { name: "Performance Analyst",       role: "Reads CTR, CPA, ROAS, watch time",               screen: 5 },
  { name: "Optimization Agent",        role: "Suggests next moves based on data",               screen: 5 },
  { name: "Experiment Planner",        role: "Builds next test batch",                          screen: 5 },
];

export const SCHOOL_AGENTS: AgentBundleEntry[] = [
  { name: "Program Architect",        role: "Defines the offer and promise",                    screen: 1 },
  { name: "Offer Agent",              role: "Sharpens positioning and enrollment message",      screen: 1 },
  { name: "Student Research Agent",   role: "Maps student needs and objections",               screen: 2 },
  { name: "Curriculum Architect",     role: "Builds program structure",                        screen: 2 },
  { name: "Lesson Builder Agent",     role: "Writes lessons and session plans",                screen: 3 },
  { name: "Assignment Designer Agent",role: "Creates practice work and rubrics",               screen: 3 },
  { name: "Resource Builder Agent",   role: "Creates worksheets, guides, templates",           screen: 3 },
  { name: "Cohort Ops Agent",         role: "Schedules and runs cohort workflows",             screen: 4 },
  { name: "Email Agent",              role: "Handles reminders, onboarding, follow-ups",       screen: 4 },
  { name: "Approval Agent",           role: "Blocks student-facing actions until reviewed",    screen: 4 },
  { name: "Community Manager Agent",  role: "Runs prompts, moderation, and engagement",        screen: 4 },
  { name: "Student Success Agent",    role: "Tracks progress and risk scoring",                screen: 5 },
  { name: "Coach Support Agent",      role: "Packages context for mentors and coaches",        screen: 5 },
  { name: "Upsell Agent",             role: "Finds expansion offers for ready students",        screen: 5 },
];
