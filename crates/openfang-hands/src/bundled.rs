//! Compile-time embedded Hand definitions.

use crate::{parse_hand_toml, HandDefinition, HandError};

/// Returns all bundled hand definitions as (id, HAND.toml content, SKILL.md content).
pub fn bundled_hands() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "clip",
            include_str!("../bundled/clip/HAND.toml"),
            include_str!("../bundled/clip/SKILL.md"),
        ),
        (
            "lead",
            include_str!("../bundled/lead/HAND.toml"),
            include_str!("../bundled/lead/SKILL.md"),
        ),
        (
            "collector",
            include_str!("../bundled/collector/HAND.toml"),
            include_str!("../bundled/collector/SKILL.md"),
        ),
        (
            "predictor",
            include_str!("../bundled/predictor/HAND.toml"),
            include_str!("../bundled/predictor/SKILL.md"),
        ),
        (
            "researcher",
            include_str!("../bundled/researcher/HAND.toml"),
            include_str!("../bundled/researcher/SKILL.md"),
        ),
        (
            "twitter",
            include_str!("../bundled/twitter/HAND.toml"),
            include_str!("../bundled/twitter/SKILL.md"),
        ),
        (
            "browser",
            include_str!("../bundled/browser/HAND.toml"),
            include_str!("../bundled/browser/SKILL.md"),
        ),
        (
            "trader",
            include_str!("../bundled/trader/HAND.toml"),
            include_str!("../bundled/trader/SKILL.md"),
        ),
    ]
}

/// Parse a bundled HAND.toml into a HandDefinition with its skill content attached.
pub fn parse_bundled(
    _id: &str,
    toml_content: &str,
    skill_content: &str,
) -> Result<HandDefinition, HandError> {
    let mut def: HandDefinition =
        parse_hand_toml(toml_content).map_err(|e| HandError::TomlParse(e.to_string()))?;
    if !skill_content.is_empty() {
        def.skill_content = Some(skill_content.to_string());
    }
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_hands_not_empty() {
        let hands = bundled_hands();
        assert!(!hands.is_empty());
        assert_eq!(hands[0].0, "clip");
    }

    #[test]
    fn bundled_hands_count() {
        let hands = bundled_hands();
        assert_eq!(hands.len(), 8);
    }

    #[test]
    fn parse_clip_hand() {
        let hands = bundled_hands();
        let (id, toml_content, skill_content) = hands[0];
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "clip");
        assert_eq!(def.name, "Clip Hand");
        assert_eq!(def.category, crate::HandCategory::Content);
        assert!(def.skill_content.is_some());
        assert!(!def.requires.is_empty());
        assert!(!def.tools.is_empty());
        assert!(!def.agent.system_prompt.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
    }

    #[test]
    fn parse_lead_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "lead")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "lead");
        assert_eq!(def.name, "Lead Hand");
        assert_eq!(def.category, crate::HandCategory::Data);
        assert!(def.skill_content.is_some());
        assert!(def.requires.is_empty());
        assert!(!def.tools.is_empty());
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert!(def.agent.temperature < 0.5);
    }

    #[test]
    fn parse_collector_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "collector")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "collector");
        assert_eq!(def.name, "Collector Hand");
        assert_eq!(def.category, crate::HandCategory::Data);
        assert!(def.skill_content.is_some());
        assert!(def.requires.is_empty());
        assert!(def.tools.contains(&"event_publish".to_string()));
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
    }

    #[test]
    fn parse_predictor_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "predictor")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "predictor");
        assert_eq!(def.name, "Predictor Hand");
        assert_eq!(def.category, crate::HandCategory::Data);
        assert!(def.skill_content.is_some());
        assert!(def.requires.is_empty());
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert!((def.agent.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_researcher_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "researcher")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "researcher");
        assert_eq!(def.name, "Researcher Hand");
        assert_eq!(def.category, crate::HandCategory::Productivity);
        assert!(def.skill_content.is_some());
        assert!(def.requires.is_empty());
        assert!(def.tools.contains(&"event_publish".to_string()));
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert_eq!(def.agent.max_iterations, Some(80));
    }

    #[test]
    fn parse_twitter_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "twitter")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "twitter");
        assert_eq!(def.name, "Twitter Hand");
        assert_eq!(def.category, crate::HandCategory::Communication);
        assert!(def.skill_content.is_some());
        assert!(!def.requires.is_empty()); // requires TWITTER_BEARER_TOKEN
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert!((def.agent.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_browser_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "browser")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "browser");
        assert_eq!(def.name, "Browser Hand");
        assert_eq!(def.category, crate::HandCategory::Productivity);
        assert!(def.skill_content.is_some());
        assert!(!def.requires.is_empty()); // requires python3 + chromium
        assert_eq!(def.requires.len(), 2);
        assert!(def.tools.contains(&"browser_navigate".to_string()));
        assert!(def.tools.contains(&"browser_click".to_string()));
        assert!(def.tools.contains(&"browser_type".to_string()));
        assert!(def.tools.contains(&"browser_screenshot".to_string()));
        assert!(def.tools.contains(&"browser_read_page".to_string()));
        assert!(def.tools.contains(&"browser_close".to_string()));
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert!((def.agent.temperature - 0.3).abs() < f32::EPSILON);
        assert_eq!(def.agent.max_iterations, Some(60));
    }

    #[test]
    fn parse_trader_hand() {
        let (id, toml_content, skill_content) = bundled_hands()
            .into_iter()
            .find(|(id, _, _)| *id == "trader")
            .unwrap();
        let def = parse_bundled(id, toml_content, skill_content).unwrap();
        assert_eq!(def.id, "trader");
        assert_eq!(def.name, "Trading Hand");
        assert_eq!(def.category, crate::HandCategory::Data);
        assert!(def.skill_content.is_some());
        assert!(def.requires.is_empty()); // no hard requirements
        assert!(!def.tools.is_empty());
        assert!(def.tools.contains(&"event_publish".to_string()));
        assert!(!def.settings.is_empty());
        assert!(!def.dashboard.metrics.is_empty());
        assert!((def.agent.temperature - 0.3).abs() < f32::EPSILON);
        assert_eq!(def.agent.max_iterations, Some(80));
    }

    #[test]
    fn all_bundled_hands_parse() {
        for (id, toml_content, skill_content) in bundled_hands() {
            let def = parse_bundled(id, toml_content, skill_content)
                .unwrap_or_else(|e| panic!("Failed to parse hand '{}': {}", id, e));
            assert_eq!(def.id, id);
            assert!(!def.name.is_empty());
            assert!(!def.tools.is_empty());
            assert!(!def.agent.system_prompt.is_empty());
            assert!(def.skill_content.is_some());
        }
    }

    #[test]
    fn all_einstein_hands_have_schedules() {
        let einstein_ids = ["lead", "collector", "predictor", "researcher", "twitter", "trader"];
        for (id, toml_content, skill_content) in bundled_hands() {
            if einstein_ids.contains(&id) {
                let def = parse_bundled(id, toml_content, skill_content).unwrap();
                assert!(
                    def.tools.contains(&"schedule_create".to_string()),
                    "Einstein hand '{}' must have schedule_create tool",
                    id
                );
                assert!(
                    def.tools.contains(&"schedule_list".to_string()),
                    "Einstein hand '{}' must have schedule_list tool",
                    id
                );
                assert!(
                    def.tools.contains(&"schedule_delete".to_string()),
                    "Einstein hand '{}' must have schedule_delete tool",
                    id
                );
            }
        }
    }

    #[test]
    fn all_einstein_hands_have_memory() {
        let einstein_ids = ["lead", "collector", "predictor", "researcher", "twitter", "trader"];
        for (id, toml_content, skill_content) in bundled_hands() {
            if einstein_ids.contains(&id) {
                let def = parse_bundled(id, toml_content, skill_content).unwrap();
                assert!(
                    def.tools.contains(&"memory_store".to_string()),
                    "Einstein hand '{}' must have memory_store tool",
                    id
                );
                assert!(
                    def.tools.contains(&"memory_recall".to_string()),
                    "Einstein hand '{}' must have memory_recall tool",
                    id
                );
            }
        }
    }

    #[test]
    fn all_einstein_hands_have_knowledge_graph() {
        let einstein_ids = ["lead", "collector", "predictor", "researcher", "twitter", "trader"];
        for (id, toml_content, skill_content) in bundled_hands() {
            if einstein_ids.contains(&id) {
                let def = parse_bundled(id, toml_content, skill_content).unwrap();
                assert!(
                    def.tools.contains(&"knowledge_add_entity".to_string()),
                    "Einstein hand '{}' must have knowledge_add_entity tool",
                    id
                );
                assert!(
                    def.tools.contains(&"knowledge_add_relation".to_string()),
                    "Einstein hand '{}' must have knowledge_add_relation tool",
                    id
                );
                assert!(
                    def.tools.contains(&"knowledge_query".to_string()),
                    "Einstein hand '{}' must have knowledge_query tool",
                    id
                );
            }
        }
    }
}
