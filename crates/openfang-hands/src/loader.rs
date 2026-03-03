//! Load hand definitions from local directories at runtime.

use crate::{HandDefinition, HandError};
use std::path::Path;

/// Load a hand definition from a directory containing HAND.toml and optional SKILL.md.
pub fn load_from_dir(dir: &Path) -> Result<HandDefinition, HandError> {
    let hand_toml = dir.join("HAND.toml");
    if !hand_toml.exists() {
        return Err(HandError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("No HAND.toml found in {}", dir.display()),
        )));
    }

    let toml_content = std::fs::read_to_string(&hand_toml)?;
    let mut def: HandDefinition =
        toml::from_str(&toml_content).map_err(|e| HandError::TomlParse(e.to_string()))?;

    let skill_path = dir.join("SKILL.md");
    if skill_path.exists() {
        let skill_content = std::fs::read_to_string(&skill_path)?;
        if !skill_content.is_empty() {
            def.skill_content = Some(skill_content);
        }
    }

    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn minimal_hand_toml() -> &'static str {
        r#"
id = "myhand"
name = "My Hand"
description = "A hand for testing"
category = "content"
tools = []

[agent]
name = "my-hand"
description = "Test agent"
system_prompt = "You are a test agent."
"#
    }

    #[test]
    fn load_missing_hand_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_from_dir(dir.path()).unwrap_err();
        assert!(matches!(err, HandError::Io(_)));
        assert!(err.to_string().contains("HAND.toml"));
    }

    #[test]
    fn load_invalid_toml_returns_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("HAND.toml"), "not = valid {{ toml").unwrap();
        let err = load_from_dir(dir.path()).unwrap_err();
        assert!(matches!(err, HandError::TomlParse(_)));
    }

    #[test]
    fn load_hand_toml_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("HAND.toml"), minimal_hand_toml()).unwrap();

        let def = load_from_dir(dir.path()).unwrap();
        assert_eq!(def.id, "myhand");
        assert_eq!(def.name, "My Hand");
        assert!(def.skill_content.is_none());
    }

    #[test]
    fn load_hand_toml_with_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("HAND.toml"), minimal_hand_toml()).unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# My Skill\nDo things.").unwrap();

        let def = load_from_dir(dir.path()).unwrap();
        assert_eq!(def.id, "myhand");
        assert_eq!(def.skill_content.as_deref(), Some("# My Skill\nDo things."));
    }

    #[test]
    fn load_empty_skill_md_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("HAND.toml"), minimal_hand_toml()).unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "").unwrap();

        let def = load_from_dir(dir.path()).unwrap();
        assert!(def.skill_content.is_none());
    }
}
