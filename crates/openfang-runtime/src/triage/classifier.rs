//! Cyber-agent classifier pipeline.
//!
//! Sits downstream of the [`super::heuristic`] and [`super::moonlock`] scanners.
//! Takes their `ScanOutcome`s plus a content summary and passes them to a
//! frontier LLM (the "cyber-agent") whose system prompt is loaded from
//! `agents/cyber/agent.toml` plus an Obsidian cyber-intel vault subdirectory.
//! The agent returns a single JSON object that this module parses into a
//! [`ClassifierDecision`] which the Phase 5.4 pinboard / KG writer consumes.
//!
//! The agent receives:
//! * scanner verdicts + findings
//! * a short content summary (caller assembles)
//! * cyber-intel excerpts (caller assembles from the vault)
//!
//! It produces:
//! * `verdict` — Safe | Questionable | Malicious (mapped to [`Verdict`])
//! * `rationale` — operator-readable explanation
//! * `recommended_action` — short action label, e.g. "release", "pinboard",
//!   "quarantine", "rotate_credentials"
//! * `confidence` — 0.0..=1.0, used by the KG writer to set memory confidence
//!
//! The pipeline is **fail-closed**: any parse / LLM failure routes to
//! `ClassifierDecision::scan_failed_pinboard()` so questionable content
//! goes to the pinboard rather than quietly to memory.

use super::{ScanOutcome, Verdict};
use crate::llm_driver::{CompletionRequest, LlmDriver, LlmError};
use openfang_types::message::Message;
use serde::{Deserialize, Serialize};

/// Final classifier output the pipeline writes to disk and the KG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifierDecision {
    /// Translated verdict (questionable → Suspicious in the triage `Verdict`).
    pub verdict: Verdict,
    /// 1–3 sentence rationale from the cyber-agent.
    pub rationale: String,
    /// Short label — e.g. `"release"`, `"pinboard"`, `"quarantine"`,
    /// `"rotate_credentials"`. Free-form but conventionally lowercase
    /// snake-case.
    pub recommended_action: String,
    /// 0.0–1.0 confidence used downstream as memory-fragment confidence.
    pub confidence: f32,
    /// Scanner outcomes that fed this decision (for audit trail).
    pub scan_outcomes: Vec<ScanOutcome>,
    /// Raw classifier response in case the operator wants to re-read.
    pub raw_response: String,
}

impl ClassifierDecision {
    /// Construct a fail-closed decision. Used on any LLM/parse error so the
    /// content lands on the pinboard rather than being trusted.
    pub fn scan_failed_pinboard(scan_outcomes: Vec<ScanOutcome>, reason: &str) -> Self {
        Self {
            verdict: Verdict::ScanFailed,
            rationale: format!("Classifier failed; routing to pinboard. Reason: {reason}"),
            recommended_action: "pinboard".to_string(),
            confidence: 0.0,
            scan_outcomes,
            raw_response: String::new(),
        }
    }

    /// True iff the content can be released to memory without operator review.
    pub fn allows_release(&self) -> bool {
        matches!(self.verdict, Verdict::Safe)
    }
}

/// Errors specific to the classifier layer. Callers usually map these to
/// `ClassifierDecision::scan_failed_pinboard` via [`run_classifier`].
#[derive(Debug)]
pub enum ClassifierError {
    /// Underlying LLM call failed.
    Llm(String),
    /// LLM returned text that didn't decode to the expected schema.
    ParseResponse(String),
}

impl std::fmt::Display for ClassifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llm(m) => write!(f, "classifier llm error: {m}"),
            Self::ParseResponse(m) => write!(f, "classifier parse error: {m}"),
        }
    }
}

impl std::error::Error for ClassifierError {}

impl From<LlmError> for ClassifierError {
    fn from(e: LlmError) -> Self {
        Self::Llm(e.to_string())
    }
}

/// Build the system prompt the cyber-agent runs against. Caller appends the
/// per-incident user message via [`build_user_message`].
pub fn build_system_prompt(cyber_intel_excerpts: &str) -> String {
    let intel_block = if cyber_intel_excerpts.trim().is_empty() {
        "(no cyber-intelligence excerpts loaded)".to_string()
    } else {
        cyber_intel_excerpts.trim().to_string()
    };
    format!(
        "You are the cyber-agent for the OpenFang Agent OS — a senior security \
         analyst whose only job is to triage suspect external content surfaced \
         by the upstream heuristic and Moonlock scanners.\n\
         \n\
         You MUST respond with a single JSON object and nothing else:\n\
         {{\n\
           \"verdict\": \"safe\" | \"questionable\" | \"malicious\",\n\
           \"rationale\": \"one paragraph (1-3 sentences)\",\n\
           \"recommended_action\": \"release\" | \"pinboard\" | \"quarantine\" | \"rotate_credentials\" | <other short snake_case>,\n\
           \"confidence\": <float 0.0 to 1.0>\n\
         }}\n\
         \n\
         No prose, no code fences, no other keys. Be specific in the rationale \
         — name the indicator that drove the verdict. Bias toward `questionable` \
         when uncertain; never mark `safe` if any scanner returned a finding \
         that you cannot affirmatively explain as benign.\n\
         \n\
         Cyber-intelligence reference (curated by the operator):\n\
         <cyber_intel>\n{intel_block}\n</cyber_intel>"
    )
}

/// Build the user-facing message that ships per-incident inputs.
pub fn build_user_message(scan_outcomes: &[ScanOutcome], content_summary: &str) -> String {
    let mut s = String::new();
    s.push_str("Scanner outcomes:\n");
    for o in scan_outcomes {
        let findings = if o.findings.is_empty() {
            "(no findings)".to_string()
        } else {
            o.findings.join(", ")
        };
        s.push_str(&format!(
            "- scanner={}, verdict={:?}, findings=[{}]\n",
            o.scanner, o.verdict, findings
        ));
    }
    s.push_str("\nContent summary:\n");
    s.push_str(content_summary.trim());
    s
}

/// Run the full classifier pipeline against a frontier LLM driver.
///
/// On success, returns a [`ClassifierDecision`] populated from the LLM's
/// JSON output. On any failure (LLM error, parse error), returns a
/// fail-closed decision routed to the pinboard.
pub async fn run_classifier(
    driver: &dyn LlmDriver,
    model: &str,
    cyber_intel_excerpts: &str,
    scan_outcomes: Vec<ScanOutcome>,
    content_summary: &str,
) -> ClassifierDecision {
    let system = build_system_prompt(cyber_intel_excerpts);
    let user = build_user_message(&scan_outcomes, content_summary);

    let request = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message::user(user)],
        tools: Vec::new(),
        max_tokens: 1024,
        temperature: 0.0,
        system: Some(system),
        thinking: None,
    };

    let response = match driver.complete(request).await {
        Ok(r) => r,
        Err(e) => {
            return ClassifierDecision::scan_failed_pinboard(
                scan_outcomes,
                &format!("llm error: {e}"),
            );
        }
    };

    let raw = response.text();
    match parse_classifier_response(&raw, scan_outcomes.clone(), &raw) {
        Ok(d) => d,
        Err(e) => ClassifierDecision::scan_failed_pinboard(scan_outcomes, &format!("{e}")),
    }
}

/// Pure parser for the JSON the cyber-agent returns. Tolerates a leading
/// ```json fence the same way the reflection module does.
pub fn parse_classifier_response(
    raw: &str,
    scan_outcomes: Vec<ScanOutcome>,
    raw_full: &str,
) -> Result<ClassifierDecision, ClassifierError> {
    let stripped = strip_code_fence(raw.trim());

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        verdict: String,
        rationale: String,
        recommended_action: String,
        confidence: f32,
    }

    let r: Response = serde_json::from_str(stripped)
        .map_err(|e| ClassifierError::ParseResponse(e.to_string()))?;

    let verdict = match r.verdict.as_str() {
        "safe" => Verdict::Safe,
        "questionable" | "suspicious" => Verdict::Suspicious,
        "malicious" => Verdict::Malicious,
        other => {
            return Err(ClassifierError::ParseResponse(format!(
                "unknown verdict {other:?} (expected safe/questionable/malicious)"
            )));
        }
    };

    if r.rationale.trim().is_empty() {
        return Err(ClassifierError::ParseResponse(
            "rationale is empty".to_string(),
        ));
    }
    if r.recommended_action.trim().is_empty() {
        return Err(ClassifierError::ParseResponse(
            "recommended_action is empty".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&r.confidence) {
        return Err(ClassifierError::ParseResponse(format!(
            "confidence {} out of [0.0, 1.0]",
            r.confidence
        )));
    }

    Ok(ClassifierDecision {
        verdict,
        rationale: r.rationale,
        recommended_action: r.recommended_action,
        confidence: r.confidence,
        scan_outcomes,
        raw_response: raw_full.to_string(),
    })
}

fn strip_code_fence(s: &str) -> &str {
    if let Some(after) = s.strip_prefix("```json\n") {
        if let Some(end) = after.rfind("```") {
            return &after[..end];
        }
    }
    if let Some(after) = s.strip_prefix("```\n") {
        if let Some(end) = after.rfind("```") {
            return &after[..end];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(scanner: &str, verdict: Verdict, findings: &[&str]) -> ScanOutcome {
        ScanOutcome {
            scanner: scanner.to_string(),
            verdict,
            findings: findings.iter().map(|s| s.to_string()).collect(),
            raw: None,
        }
    }

    #[test]
    fn system_prompt_mentions_required_keys() {
        let p = build_system_prompt("");
        assert!(p.contains("\"verdict\""));
        assert!(p.contains("safe"));
        assert!(p.contains("questionable"));
        assert!(p.contains("malicious"));
        assert!(p.contains("rationale"));
        assert!(p.contains("recommended_action"));
        assert!(p.contains("confidence"));
        // Conservative bias documented:
        assert!(p.contains("Bias toward `questionable` when uncertain"));
    }

    #[test]
    fn system_prompt_includes_cyber_intel_excerpts() {
        let p = build_system_prompt("MITRE ATT&CK T1059: Command and Scripting Interpreter…");
        assert!(p.contains("MITRE ATT&CK T1059"));
    }

    #[test]
    fn system_prompt_handles_empty_intel_with_explicit_fallback() {
        let p = build_system_prompt("   ");
        assert!(p.contains("(no cyber-intelligence excerpts loaded)"));
    }

    #[test]
    fn user_message_lists_each_scanner() {
        let outs = vec![
            outcome("heuristic", Verdict::Suspicious, &["jailbreak.dan_mode"]),
            outcome("moonlock", Verdict::Safe, &[]),
        ];
        let m = build_user_message(&outs, "Scraped a forum thread about RAG.");
        assert!(m.contains("scanner=heuristic"));
        assert!(m.contains("jailbreak.dan_mode"));
        assert!(m.contains("scanner=moonlock"));
        assert!(m.contains("(no findings)"));
        assert!(m.contains("Scraped a forum thread"));
    }

    #[test]
    fn parse_safe_verdict() {
        let raw = r#"{"verdict":"safe","rationale":"benign prose","recommended_action":"release","confidence":0.95}"#;
        let d = parse_classifier_response(raw, vec![], raw).unwrap();
        assert_eq!(d.verdict, Verdict::Safe);
        assert!(d.allows_release());
        assert_eq!(d.recommended_action, "release");
        assert_eq!(d.confidence, 0.95);
    }

    #[test]
    fn parse_questionable_maps_to_suspicious() {
        let raw = r#"{"verdict":"questionable","rationale":"unclear","recommended_action":"pinboard","confidence":0.5}"#;
        let d = parse_classifier_response(raw, vec![], raw).unwrap();
        assert_eq!(d.verdict, Verdict::Suspicious);
        assert!(!d.allows_release());
    }

    #[test]
    fn parse_suspicious_alias_also_maps_to_suspicious() {
        // Defensive: some prompt variants might emit "suspicious" instead of "questionable".
        let raw = r#"{"verdict":"suspicious","rationale":"x","recommended_action":"pinboard","confidence":0.4}"#;
        let d = parse_classifier_response(raw, vec![], raw).unwrap();
        assert_eq!(d.verdict, Verdict::Suspicious);
    }

    #[test]
    fn parse_malicious_verdict() {
        let raw = r#"{"verdict":"malicious","rationale":"contains active payload","recommended_action":"quarantine","confidence":0.9}"#;
        let d = parse_classifier_response(raw, vec![], raw).unwrap();
        assert_eq!(d.verdict, Verdict::Malicious);
        assert_eq!(d.recommended_action, "quarantine");
    }

    #[test]
    fn parse_rejects_unknown_verdict() {
        let raw = r#"{"verdict":"weird","rationale":"x","recommended_action":"pinboard","confidence":0.5}"#;
        let err = parse_classifier_response(raw, vec![], raw).unwrap_err();
        assert!(matches!(err, ClassifierError::ParseResponse(_)));
    }

    #[test]
    fn parse_rejects_extra_keys() {
        let raw = r#"{"verdict":"safe","rationale":"x","recommended_action":"release","confidence":0.5,"extra":"yes"}"#;
        let err = parse_classifier_response(raw, vec![], raw).unwrap_err();
        assert!(matches!(err, ClassifierError::ParseResponse(_)));
    }

    #[test]
    fn parse_rejects_empty_rationale() {
        let raw = r#"{"verdict":"safe","rationale":"   ","recommended_action":"release","confidence":0.5}"#;
        let err = parse_classifier_response(raw, vec![], raw).unwrap_err();
        assert!(matches!(err, ClassifierError::ParseResponse(_)));
    }

    #[test]
    fn parse_rejects_empty_recommended_action() {
        let raw = r#"{"verdict":"safe","rationale":"x","recommended_action":"","confidence":0.5}"#;
        let err = parse_classifier_response(raw, vec![], raw).unwrap_err();
        assert!(matches!(err, ClassifierError::ParseResponse(_)));
    }

    #[test]
    fn parse_rejects_out_of_range_confidence() {
        for c in &["1.5", "-0.1"] {
            let raw = format!(
                r#"{{"verdict":"safe","rationale":"x","recommended_action":"release","confidence":{c}}}"#
            );
            let err = parse_classifier_response(&raw, vec![], &raw).unwrap_err();
            assert!(matches!(err, ClassifierError::ParseResponse(_)));
        }
    }

    #[test]
    fn parse_tolerates_code_fence() {
        let raw = "```json\n{\"verdict\":\"safe\",\"rationale\":\"x\",\"recommended_action\":\"release\",\"confidence\":0.9}\n```";
        let d = parse_classifier_response(raw, vec![], raw).unwrap();
        assert_eq!(d.verdict, Verdict::Safe);
    }

    #[test]
    fn scan_failed_pinboard_helper_is_fail_closed() {
        let outs = vec![outcome("h", Verdict::Suspicious, &["dan_mode"])];
        let d = ClassifierDecision::scan_failed_pinboard(outs.clone(), "llm timed out");
        assert_eq!(d.verdict, Verdict::ScanFailed);
        assert_eq!(d.recommended_action, "pinboard");
        assert!(!d.allows_release());
        assert_eq!(d.confidence, 0.0);
        assert_eq!(d.scan_outcomes, outs);
        assert!(d.rationale.contains("llm timed out"));
    }

    #[test]
    fn allows_release_only_for_safe() {
        let make = |v| ClassifierDecision {
            verdict: v,
            rationale: "x".into(),
            recommended_action: "y".into(),
            confidence: 0.5,
            scan_outcomes: vec![],
            raw_response: String::new(),
        };
        assert!(make(Verdict::Safe).allows_release());
        assert!(!make(Verdict::Suspicious).allows_release());
        assert!(!make(Verdict::Malicious).allows_release());
        assert!(!make(Verdict::ScanFailed).allows_release());
    }
}
