use openfang_types::planner::{
    PlannerAgentCatalogEntry, PlannerAgentRecommendation, PlannerRecommendationConfidence,
    PlannerTask,
};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct RegistryAgent {
    agent_id: &'static str,
    name: &'static str,
    description: &'static str,
    purpose: &'static str,
    best_for: &'static str,
    avoid_for: &'static str,
    example: &'static str,
}

const REGISTRY: &[RegistryAgent] = &[
    RegistryAgent {
        agent_id: "security-auditor",
        name: "Security Auditor",
        description: "Reviews code, auth flows, secrets, and trust boundaries for vulnerabilities.",
        purpose: "Security review, auth analysis, threat modeling, and remediation guidance.",
        best_for: "Auth reviews, threat modeling, secrets, and risky trust boundaries.",
        avoid_for: "Routine writing or broad planning without a security question.",
        example: "Review auth flow before launch.",
    },
    RegistryAgent {
        agent_id: "writer",
        name: "Writer",
        description: "Creates polished launch notes, docs, announcements, and technical writing.",
        purpose: "Long-form writing, editing, release communication, and docs drafting.",
        best_for: "Launch notes, announcements, docs, and polished written output.",
        avoid_for: "Code validation, security review, or tasks that still need scoping.",
        example: "Write launch notes for a release.",
    },
    RegistryAgent {
        agent_id: "test-engineer",
        name: "Test Engineer",
        description: "Designs tests, validates edge cases, and improves coverage.",
        purpose: "QA strategy, validation, regression testing, and test plan design.",
        best_for: "Regression coverage, edge cases, and validation-heavy tasks.",
        avoid_for: "Writing tasks or open-ended project planning.",
        example: "Design regression tests for auth changes.",
    },
    RegistryAgent {
        agent_id: "translator",
        name: "Translator",
        description: "Handles translation and localization tasks across languages.",
        purpose: "Translation, localization, and multilingual content review.",
        best_for: "Translation, localization, and language-specific review.",
        avoid_for: "General writing when no language conversion is needed.",
        example: "Translate onboarding email into Spanish.",
    },
    RegistryAgent {
        agent_id: "tutor",
        name: "Tutor",
        description: "Explains complex topics clearly and adapts to the learner’s level.",
        purpose: "Teaching, walkthroughs, and explanation-heavy work.",
        best_for: "Walkthroughs, teaching, and explanation-heavy tasks.",
        avoid_for: "Final deliverables that need execution more than explanation.",
        example: "Explain a new auth flow to the team.",
    },
    RegistryAgent {
        agent_id: "researcher",
        name: "Researcher",
        description: "Investigates topics, synthesizes findings, and compares options.",
        purpose: "Research, comparison, fact finding, and synthesis.",
        best_for: "Option comparison, investigation, and synthesis before a decision.",
        avoid_for: "Straightforward execution when the task is already clear.",
        example: "Compare auth provider options.",
    },
];

pub(crate) fn catalog(preferences: &HashMap<String, bool>) -> Vec<PlannerAgentCatalogEntry> {
    REGISTRY
        .iter()
        .map(|agent| PlannerAgentCatalogEntry {
            agent_id: agent.agent_id.to_string(),
            name: agent.name.to_string(),
            description: agent.description.to_string(),
            purpose: agent.purpose.to_string(),
            best_for: agent.best_for.to_string(),
            avoid_for: agent.avoid_for.to_string(),
            example: agent.example.to_string(),
            enabled: preferences.get(agent.agent_id).copied().unwrap_or(true),
        })
        .collect()
}

pub(crate) fn recommend_for_task(
    task: &PlannerTask,
    preferences: &HashMap<String, bool>,
) -> Option<PlannerAgentRecommendation> {
    let text = format!(
        "{} {} {}",
        task.title,
        task.next_action,
        task.blocked_by.join(" ")
    )
    .to_lowercase();

    let mut scored = vec![
        score_agent(
            preferences,
            "security-auditor",
            &text,
            &[
                ("security review", 6),
                ("auth flow", 6),
                ("authentication", 5),
                ("authorization", 5),
                ("security", 4),
                ("audit", 4),
                ("vulnerability", 4),
                ("threat", 4),
                ("secret", 4),
                ("token", 3),
            ],
            "Fits a security review task.",
        ),
        score_agent(
            preferences,
            "writer",
            &text,
            &[
                ("write", 4),
                ("draft", 4),
                ("launch notes", 7),
                ("release notes", 7),
                ("announcement", 5),
                ("docs", 4),
                ("documentation", 4),
            ],
            "Fits a writing task.",
        ),
        score_agent(
            preferences,
            "test-engineer",
            &text,
            &[
                ("test", 5),
                ("qa", 4),
                ("regression", 5),
                ("coverage", 4),
                ("validate", 4),
            ],
            "Fits a validation-heavy task.",
        ),
        score_agent(
            preferences,
            "translator",
            &text,
            &[("translate", 6), ("translation", 6), ("localize", 5), ("localization", 5)],
            "Fits a translation task.",
        ),
        score_agent(
            preferences,
            "tutor",
            &text,
            &[("teach", 5), ("explain", 5), ("walkthrough", 5), ("learn", 4)],
            "Fits an explanation-focused task.",
        ),
        score_agent(
            preferences,
            "researcher",
            &text,
            &[("research", 6), ("investigate", 5), ("compare", 4), ("analyze", 4)],
            "Fits a research task.",
        ),
    ];

    scored.sort_by(|left, right| right.score.cmp(&left.score));
    if let Some(best) = scored.into_iter().find(|entry| entry.score >= 4) {
        return Some(best.into_recommendation());
    }

    None
}

struct ScoredAgent {
    agent: RegistryAgent,
    score: i32,
    reason: &'static str,
}

impl ScoredAgent {
    fn into_recommendation(self) -> PlannerAgentRecommendation {
        PlannerAgentRecommendation {
            agent_id: self.agent.agent_id.to_string(),
            name: self.agent.name.to_string(),
            reason: self.reason.to_string(),
            confidence: if self.score >= 7 {
                PlannerRecommendationConfidence::High
            } else if self.score >= 4 {
                PlannerRecommendationConfidence::Medium
            } else {
                PlannerRecommendationConfidence::Low
            },
        }
    }
}

fn score_agent(
    preferences: &HashMap<String, bool>,
    agent_id: &'static str,
    text: &str,
    patterns: &[(&'static str, i32)],
    reason: &'static str,
) -> ScoredAgent {
    let agent = REGISTRY
        .iter()
        .find(|candidate| candidate.agent_id == agent_id)
        .copied()
        .expect("registry entry should exist");

    if !preferences.get(agent_id).copied().unwrap_or(true) {
        return ScoredAgent {
            agent,
            score: -1,
            reason,
        };
    }

    let score = patterns
        .iter()
        .filter(|(pattern, _)| text.contains(*pattern))
        .map(|(_, weight)| *weight)
        .sum();

    ScoredAgent { agent, score, reason }
}