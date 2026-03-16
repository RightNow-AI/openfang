use crate::definition::{
    AgentNode, AgentSelector, ApprovalNode, ApprovalRejection, RouteNode, RouteRule,
    WorkflowDefinition, WorkflowNode,
};
use std::collections::HashMap;

/// Full support-triage workflow:
///   intake (support-responder) →
///   route-specialist (keyword router) →
///   [frontend-specialist | security-specialist | general-specialist] (each jumps to review) →
///   review (reviewer) →
///   approval-gate (pauses for human) →
///   complete (support-responder drafts final response)
pub fn support_triage_workflow() -> WorkflowDefinition {
    WorkflowDefinition {
        id: "support-triage".to_string(),
        name: "Support Triage".to_string(),
        description: "Triage a support request, route to the right specialist, review, get approval, then draft the final response.".to_string(),
        steps: vec![
            // 0
            WorkflowNode::Agent(AgentNode {
                id: "intake".to_string(),
                title: "Intake".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "support-responder".to_string(),
                },
                prompt: "Triage this support request. Respond with: \
                    Category: [frontend|backend|security|data|infra], \
                    Urgency: [critical|high|normal|low], \
                    Summary: one sentence, \
                    Route to: [frontend-specialist|security-specialist|general-specialist]. \
                    Request: {{input}}".to_string(),
                store_as: Some("triage".to_string()),
                next_step_id: None,
            }),
            // 1 — Route based on intake output
            WorkflowNode::Route(RouteNode {
                id: "route-specialist".to_string(),
                title: "Route Specialist".to_string(),
                rules: vec![
                    RouteRule {
                        when_contains: "frontend-specialist".to_string(),
                        next_step_id: "frontend-specialist".to_string(),
                    },
                    RouteRule {
                        when_contains: "security-specialist".to_string(),
                        next_step_id: "security-specialist".to_string(),
                    },
                    // Keyword shortcuts in triage text
                    RouteRule {
                        when_contains: "frontend".to_string(),
                        next_step_id: "frontend-specialist".to_string(),
                    },
                    RouteRule {
                        when_contains: "security".to_string(),
                        next_step_id: "security-specialist".to_string(),
                    },
                ],
                fallback_step_id: "general-specialist".to_string(),
            }),
            // 2 — Frontend specialist, then jump to review
            WorkflowNode::Agent(AgentNode {
                id: "frontend-specialist".to_string(),
                title: "Frontend Specialist".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "frontend-developer".to_string(),
                },
                prompt: "Investigate this frontend issue and propose a resolution plan. \
                    Triage: {{triage}} \
                    Original request: {{input}}".to_string(),
                store_as: Some("specialist_analysis".to_string()),
                next_step_id: Some("review".to_string()),
            }),
            // 3 — Security specialist, then jump to review
            WorkflowNode::Agent(AgentNode {
                id: "security-specialist".to_string(),
                title: "Security Specialist".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "security-auditor".to_string(),
                },
                prompt: "Assess this security issue for risk and recommend immediate actions. \
                    Triage: {{triage}} \
                    Original request: {{input}}".to_string(),
                store_as: Some("specialist_analysis".to_string()),
                next_step_id: Some("review".to_string()),
            }),
            // 4 — General specialist (fallback), then jump to review
            WorkflowNode::Agent(AgentNode {
                id: "general-specialist".to_string(),
                title: "General Specialist".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "support-responder".to_string(),
                },
                prompt: "Handle this support issue directly. Provide a resolution recommendation. \
                    Triage: {{triage}} \
                    Original request: {{input}}".to_string(),
                store_as: Some("specialist_analysis".to_string()),
                next_step_id: Some("review".to_string()),
            }),
            // 5 — Review
            WorkflowNode::Agent(AgentNode {
                id: "review".to_string(),
                title: "Review".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "reviewer".to_string(),
                },
                prompt: "Review this support analysis for quality, completeness, and accuracy. \
                    Analysis: {{specialist_analysis}} \
                    Original request: {{input}}".to_string(),
                store_as: Some("review_output".to_string()),
                next_step_id: None,
            }),
            // 6 — Approval gate
            WorkflowNode::Approval(ApprovalNode {
                id: "approval-gate".to_string(),
                title: "Approval Gate".to_string(),
                prompt: "Approve sending this support response to the customer?".to_string(),
                on_rejected: ApprovalRejection::CompleteRun {
                    message: "Response not approved — not sent to customer.".to_string(),
                },
            }),
            // 7 — Complete: draft final response
            WorkflowNode::Agent(AgentNode {
                id: "complete".to_string(),
                title: "Complete".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "support-responder".to_string(),
                },
                prompt: "Draft the final customer-facing response for this support issue. \
                    Specialist analysis: {{specialist_analysis}} \
                    Review: {{review_output}} \
                    Original request: {{input}}".to_string(),
                store_as: Some("final_response".to_string()),
                next_step_id: None,
            }),
        ],
    }
}

pub fn render_prompt(
    template: &str,
    input: &str,
    last_output: Option<&str>,
    outputs: &HashMap<String, String>,
) -> String {
    let mut rendered = template.replace("{{input}}", input);
    let last_output = last_output.unwrap_or(input);
    rendered = rendered.replace("{{last_output}}", last_output);

    for (key, value) in outputs {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }

    rendered
}

