use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliverableContract {
    pub id: String,
    pub name: String,
    pub description: String,
    pub artifact_kind: ArtifactKind,
    pub required_sections: Vec<String>,
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Markdown,
    Json,
    Text,
    CodeSnippet,
    Checklist,
    Report,
    ResponseDraft,
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverableTemplate {
    pub title: String,
    pub body_markdown: String,
}
