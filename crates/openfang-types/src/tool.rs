//! Tool definition and result types.

use serde::{Deserialize, Serialize};

/// Definition of a tool that an agent can use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique tool identifier.
    pub name: String,
    /// Human-readable description for the LLM.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool use instance.
    pub id: String,
    /// Which tool to call.
    pub name: String,
    /// The input parameters.
    pub input: serde_json::Value,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The tool_use ID this result corresponds to.
    pub tool_use_id: String,
    /// The output content.
    pub content: String,
    /// Whether the tool execution resulted in an error.
    pub is_error: bool,
}

/// Normalize a JSON Schema for cross-provider compatibility.
///
/// Some providers (Gemini, Groq) reject `anyOf` in tool schemas.
/// This function:
/// - Converts `anyOf` arrays of simple types to flat `enum` arrays
/// - Strips `$schema` keys (not accepted by most providers)
/// - Recursively walks `properties` and `items`
pub fn normalize_schema_for_provider(
    schema: &serde_json::Value,
    provider: &str,
) -> serde_json::Value {
    // Anthropic handles anyOf natively — no normalization needed
    if provider == "anthropic" {
        return schema.clone();
    }
    normalize_schema_recursive(schema)
}

fn normalize_schema_recursive(schema: &serde_json::Value) -> serde_json::Value {
    let obj = match schema.as_object() {
        Some(o) => o,
        None => {
            // If the schema is a JSON string, try to parse it as a JSON object.
            // Some MCP servers / skill definitions serialize schemas as strings.
            if let Some(s) = schema.as_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    if parsed.is_object() {
                        return normalize_schema_recursive(&parsed);
                    }
                }
            }
            // Non-object schema (null, number, bool, unparseable string, array) —
            // return a valid empty object schema so providers don't reject it.
            return serde_json::json!({"type": "object", "properties": {}});
        }
    };

    // Resolve $ref references before processing.
    // If the schema has $defs and $ref, inline the referenced definition.
    let resolved = resolve_refs(obj);
    let obj = resolved.as_object().unwrap_or(obj);

    let mut result = serde_json::Map::new();

    for (key, value) in obj {
        // Strip fields unsupported by Gemini and most non-Anthropic providers
        if matches!(
            key.as_str(),
            "$schema"
                | "$defs"
                | "$ref"
                | "additionalProperties"
                | "default"
                | "$id"
                | "$comment"
                | "examples"
                | "title"
                | "const"
                | "format"
        ) {
            continue;
        }

        // Convert anyOf/oneOf to flat type + enum when possible
        if key == "anyOf" || key == "oneOf" {
            if let Some(converted) = try_flatten_any_of(value) {
                for (k, v) in converted {
                    result.insert(k, v);
                }
                continue;
            }
            // Can't flatten — strip entirely rather than leave unsupported keyword
            continue;
        }

        // Flatten type arrays like ["string", "null"] to single type + nullable
        if key == "type" {
            if let Some(arr) = value.as_array() {
                let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                let has_null = types.contains(&"null");
                let non_null: Vec<&&str> =
                    types.iter().filter(|&&t| t != "null").collect();
                if has_null && non_null.len() == 1 {
                    // ["string", "null"] → type: "string", nullable: true
                    result.insert(
                        "type".to_string(),
                        serde_json::Value::String(non_null[0].to_string()),
                    );
                    result.insert("nullable".to_string(), serde_json::Value::Bool(true));
                    continue;
                } else if non_null.len() == 1 {
                    // ["string"] → type: "string"
                    result.insert(
                        "type".to_string(),
                        serde_json::Value::String(non_null[0].to_string()),
                    );
                    continue;
                } else if !non_null.is_empty() {
                    // Multiple non-null types — pick first (best effort)
                    result.insert(
                        "type".to_string(),
                        serde_json::Value::String(non_null[0].to_string()),
                    );
                    if has_null {
                        result.insert(
                            "nullable".to_string(),
                            serde_json::Value::Bool(true),
                        );
                    }
                    continue;
                }
            }
            // Scalar type string — pass through
            result.insert(key.clone(), value.clone());
            continue;
        }

        // Recurse into properties
        if key == "properties" {
            if let Some(props) = value.as_object() {
                let mut new_props = serde_json::Map::new();
                for (prop_name, prop_schema) in props {
                    new_props.insert(prop_name.clone(), normalize_schema_recursive(prop_schema));
                }
                result.insert(key.clone(), serde_json::Value::Object(new_props));
                continue;
            }
        }

        // Recurse into items
        if key == "items" {
            result.insert(key.clone(), normalize_schema_recursive(value));
            continue;
        }

        result.insert(key.clone(), value.clone());
    }

    serde_json::Value::Object(result)
}

/// Resolve `$ref` references by inlining definitions from `$defs`.
///
/// If the schema has `$defs` and any property uses `$ref: "#/$defs/Foo"`,
/// replace the `$ref` with the actual definition. This is needed because
/// Gemini and most providers don't support `$ref`/`$defs`.
fn resolve_refs(obj: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let defs = match obj.get("$defs").and_then(|d| d.as_object()) {
        Some(d) => d.clone(),
        None => return serde_json::Value::Object(obj.clone()),
    };

    let mut result = obj.clone();
    result.remove("$defs");

    // Recursively replace $ref in the schema
    fn inline_refs(
        val: &mut serde_json::Value,
        defs: &serde_json::Map<String, serde_json::Value>,
    ) {
        match val {
            serde_json::Value::Object(map) => {
                // If this object is a $ref, replace it with the definition
                if let Some(ref_val) = map.get("$ref").and_then(|r| r.as_str()) {
                    let ref_name = ref_val
                        .strip_prefix("#/$defs/")
                        .or_else(|| ref_val.strip_prefix("#/definitions/"));
                    if let Some(name) = ref_name {
                        if let Some(def) = defs.get(name) {
                            *val = def.clone();
                            // Recurse into the inlined definition
                            inline_refs(val, defs);
                            return;
                        }
                    }
                }
                // Recurse into all values
                for v in map.values_mut() {
                    inline_refs(v, defs);
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    inline_refs(item, defs);
                }
            }
            _ => {}
        }
    }

    let mut resolved = serde_json::Value::Object(result);
    inline_refs(&mut resolved, &defs);
    resolved
}

/// Try to flatten an `anyOf` array into a simple type + enum.
///
/// Works when all variants are simple types (string, number, etc.) or
/// when it's a nullable pattern like `anyOf: [{type: "string"}, {type: "null"}]`.
fn try_flatten_any_of(any_of: &serde_json::Value) -> Option<Vec<(String, serde_json::Value)>> {
    let items = any_of.as_array()?;
    if items.is_empty() {
        return None;
    }

    // Check if this is a simple type union (all items have just "type")
    let mut types = Vec::new();
    let mut has_null = false;
    let mut non_null_type = None;

    for item in items {
        let obj = item.as_object()?;
        let type_val = obj.get("type")?.as_str()?;

        if type_val == "null" {
            has_null = true;
        } else {
            types.push(type_val.to_string());
            non_null_type = Some(type_val.to_string());
        }
    }

    // If it's a nullable pattern (type + null), emit the non-null type
    if has_null && types.len() == 1 {
        let mut result = vec![(
            "type".to_string(),
            serde_json::Value::String(non_null_type.unwrap()),
        )];
        // Mark as nullable via description hint (since JSON Schema nullable isn't universal)
        result.push(("nullable".to_string(), serde_json::Value::Bool(true)));
        return Some(result);
    }

    // If all items are simple types, pick the first non-null type (best effort).
    // Gemini rejects type arrays, so we can't emit ["string", "number"].
    if types.len() == items.len() && types.len() > 1 {
        let mut result = vec![(
            "type".to_string(),
            serde_json::Value::String(types[0].clone()),
        )];
        if has_null {
            result.push(("nullable".to_string(), serde_json::Value::Bool(true)));
        }
        return Some(result);
    }

    // Can't flatten — caller will strip the key entirely
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("web_search"));
    }

    #[test]
    fn test_normalize_schema_strips_dollar_schema() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result.get("$schema").is_none());
        assert_eq!(result["type"], "object");
    }

    #[test]
    fn test_normalize_schema_flattens_anyof_nullable() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let value_prop = &result["properties"]["value"];
        assert_eq!(value_prop["type"], "string");
        assert_eq!(value_prop["nullable"], true);
        assert!(value_prop.get("anyOf").is_none());
    }

    #[test]
    fn test_normalize_schema_flattens_anyof_multi_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": {
                    "anyOf": [
                        { "type": "string" },
                        { "type": "number" }
                    ]
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "groq");
        let value_prop = &result["properties"]["value"];
        // Gemini rejects type arrays — should flatten to first type
        assert_eq!(value_prop["type"], "string");
        assert!(value_prop.get("anyOf").is_none());
    }

    #[test]
    fn test_normalize_schema_anthropic_passthrough() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "anyOf": [{"type": "string"}]
        });
        let result = normalize_schema_for_provider(&schema, "anthropic");
        // Anthropic should get the original schema unchanged
        assert!(result.get("$schema").is_some());
    }

    #[test]
    fn test_normalize_schema_nested_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "outer": {
                    "type": "object",
                    "properties": {
                        "inner": {
                            "$schema": "strip_me",
                            "type": "string"
                        }
                    }
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result["properties"]["outer"]["properties"]["inner"]
            .get("$schema")
            .is_none());
    }

    #[test]
    fn test_normalize_schema_string_parsed_to_object() {
        // MCP servers may return inputSchema as a JSON string
        let schema = serde_json::Value::String(
            r#"{"type":"object","properties":{"query":{"type":"string"}}}"#.to_string(),
        );
        let result = normalize_schema_for_provider(&schema, "openai");
        assert!(result.is_object());
        assert_eq!(result["type"], "object");
        assert!(result["properties"]["query"].is_object());
    }

    #[test]
    fn test_normalize_schema_null_becomes_empty_object() {
        let schema = serde_json::Value::Null;
        let result = normalize_schema_for_provider(&schema, "openai");
        assert!(result.is_object());
        assert_eq!(result["type"], "object");
    }

    #[test]
    fn test_normalize_schema_unparseable_string_becomes_empty_object() {
        let schema = serde_json::Value::String("not valid json".to_string());
        let result = normalize_schema_for_provider(&schema, "openai");
        assert!(result.is_object());
        assert_eq!(result["type"], "object");
    }

    #[test]
    fn test_normalize_schema_number_becomes_empty_object() {
        let schema = serde_json::json!(42);
        let result = normalize_schema_for_provider(&schema, "openai");
        assert!(result.is_object());
        assert_eq!(result["type"], "object");
    }

    #[test]
    fn test_normalize_schema_string_with_dollar_schema_stripped() {
        // String schema that contains $schema — should be parsed AND normalized
        let schema = serde_json::Value::String(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","properties":{}}"#.to_string(),
        );
        let result = normalize_schema_for_provider(&schema, "openai");
        assert!(result.is_object());
        assert_eq!(result["type"], "object");
        assert!(result.get("$schema").is_none());
    }

    #[test]
    fn test_normalize_strips_additional_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": { "type": "string", "default": "hello", "title": "Name" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result.get("additionalProperties").is_none());
        assert!(result["properties"]["name"].get("default").is_none());
        assert!(result["properties"]["name"].get("title").is_none());
        assert_eq!(result["properties"]["name"]["type"], "string");
    }

    #[test]
    fn test_normalize_resolves_refs() {
        let schema = serde_json::json!({
            "type": "object",
            "$defs": {
                "Color": {
                    "type": "string",
                    "enum": ["red", "green", "blue"]
                }
            },
            "properties": {
                "color": { "$ref": "#/$defs/Color" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result.get("$defs").is_none());
        assert_eq!(result["properties"]["color"]["type"], "string");
        assert!(result["properties"]["color"]["enum"].is_array());
    }

    #[test]
    fn test_normalize_strips_defs_without_refs() {
        let schema = serde_json::json!({
            "type": "object",
            "$defs": { "Unused": { "type": "number" } },
            "properties": {
                "x": { "type": "string" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result.get("$defs").is_none());
        assert_eq!(result["properties"]["x"]["type"], "string");
    }

    // --- Issue #488 tests ---

    #[test]
    fn test_normalize_strips_const() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "version": { "type": "string", "const": "v1" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result["properties"]["version"].get("const").is_none());
        assert_eq!(result["properties"]["version"]["type"], "string");
    }

    #[test]
    fn test_normalize_strips_format() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "created_at": { "type": "string", "format": "date-time" },
                "email": { "type": "string", "format": "email" }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert!(result["properties"]["created_at"].get("format").is_none());
        assert!(result["properties"]["email"].get("format").is_none());
        assert_eq!(result["properties"]["created_at"]["type"], "string");
        assert_eq!(result["properties"]["email"]["type"], "string");
    }

    #[test]
    fn test_normalize_flattens_oneof_nullable() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let value_prop = &result["properties"]["value"];
        assert_eq!(value_prop["type"], "string");
        assert_eq!(value_prop["nullable"], true);
        assert!(value_prop.get("oneOf").is_none());
    }

    #[test]
    fn test_normalize_strips_oneof_complex() {
        // Complex oneOf that can't be flattened — should be stripped entirely
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "data": {
                    "oneOf": [
                        { "type": "object", "properties": { "a": { "type": "string" } } },
                        { "type": "object", "properties": { "b": { "type": "number" } } }
                    ]
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let data_prop = &result["properties"]["data"];
        assert!(data_prop.get("oneOf").is_none());
    }

    #[test]
    fn test_normalize_flattens_type_array_nullable() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": ["string", "null"] }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let name_prop = &result["properties"]["name"];
        assert_eq!(name_prop["type"], "string");
        assert_eq!(name_prop["nullable"], true);
    }

    #[test]
    fn test_normalize_flattens_type_array_multi() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": ["string", "number", "null"] }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let value_prop = &result["properties"]["value"];
        // Should pick first non-null type
        assert_eq!(value_prop["type"], "string");
        assert_eq!(value_prop["nullable"], true);
    }

    #[test]
    fn test_normalize_flattens_type_array_single() {
        // Single-element type array
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": ["integer"] }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        assert_eq!(result["properties"]["x"]["type"], "integer");
        assert!(result["properties"]["x"].get("nullable").is_none());
    }

    #[test]
    fn test_normalize_strips_anyof_complex() {
        // Complex anyOf that can't be flattened — should be stripped entirely
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "payload": {
                    "anyOf": [
                        { "type": "object", "properties": { "url": { "type": "string" } } },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        let payload_prop = &result["properties"]["payload"];
        assert!(payload_prop.get("anyOf").is_none());
    }

    #[test]
    fn test_normalize_combined_issue_488() {
        // Real-world schema combining multiple #488 issues
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "api_version": { "type": "string", "const": "v2", "format": "semver" },
                "timestamp": { "type": "string", "format": "date-time" },
                "label": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "null" }
                    ]
                },
                "tags": { "type": ["string", "null"] }
            }
        });
        let result = normalize_schema_for_provider(&schema, "gemini");
        // const and format stripped
        assert!(result["properties"]["api_version"].get("const").is_none());
        assert!(result["properties"]["api_version"].get("format").is_none());
        assert!(result["properties"]["timestamp"].get("format").is_none());
        // oneOf flattened
        assert_eq!(result["properties"]["label"]["type"], "string");
        assert_eq!(result["properties"]["label"]["nullable"], true);
        assert!(result["properties"]["label"].get("oneOf").is_none());
        // type array flattened
        assert_eq!(result["properties"]["tags"]["type"], "string");
        assert_eq!(result["properties"]["tags"]["nullable"], true);
    }
}
