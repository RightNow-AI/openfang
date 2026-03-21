//! Python SDK generator for Programmatic Tool Calling.
//!
//! Converts `ToolDefinition` schemas into:
//! 1. A Python SDK preamble with sync function stubs (injected before user code)
//! 2. Compact one-line function signatures (shown in the `execute_code` tool description)
//!
//! The generated SDK uses `urllib.request` for zero-dependency HTTP calls to the
//! localhost IPC server. All functions are synchronous — no async/await confusion.

use openfang_types::tool::ToolDefinition;

/// Python reserved words that need a trailing underscore.
const PYTHON_RESERVED: &[&str] = &[
    "type", "class", "import", "from", "return", "pass", "in", "is", "not", "and", "or", "for",
    "while", "if", "else", "elif", "try", "except", "finally", "with", "as", "def", "del",
    "global", "nonlocal", "lambda", "yield", "assert", "break", "continue", "raise", "True",
    "False", "None",
];

/// Python builtin names that must not be shadowed by tool function names.
/// If a tool name collides with one of these, it gets a trailing underscore.
const PYTHON_BUILTINS: &[&str] = &[
    "print",
    "input",
    "open",
    "id",
    "list",
    "dict",
    "set",
    "map",
    "filter",
    "hash",
    "format",
    "range",
    "len",
    "str",
    "int",
    "float",
    "bool",
    "bytes",
    "tuple",
    "abs",
    "all",
    "any",
    "bin",
    "chr",
    "dir",
    "eval",
    "exec",
    "exit",
    "getattr",
    "globals",
    "hasattr",
    "help",
    "hex",
    "isinstance",
    "issubclass",
    "iter",
    "locals",
    "max",
    "min",
    "next",
    "object",
    "oct",
    "ord",
    "pow",
    "property",
    "repr",
    "reversed",
    "round",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "sum",
    "super",
    "vars",
    "zip",
];

/// A parameter extracted from a JSON Schema.
#[derive(Debug)]
struct SchemaParam {
    /// Python-safe name (e.g. `type_` for `type`).
    name: String,
    /// Original schema key (used in the IPC args dict).
    original_name: String,
    /// Python type hint string.
    python_type: String,
    /// Whether this parameter is required.
    required: bool,
    /// Default value as Python literal, if optional.
    default: Option<String>,
    /// Human-readable description (reserved for future docstring expansion).
    #[allow(dead_code)]
    description: Option<String>,
}

/// Convert a JSON Schema type to a Python type hint.
fn json_type_to_python(schema: &serde_json::Value) -> String {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => "str".to_string(),
        Some("integer") => "int".to_string(),
        Some("number") => "float".to_string(),
        Some("boolean") => "bool".to_string(),
        Some("array") => "list".to_string(),
        Some("object") => "dict".to_string(),
        _ => {
            // Handle anyOf/oneOf (nullable patterns, union types)
            if let Some(any_of) = schema.get("anyOf").or(schema.get("oneOf")) {
                if let Some(arr) = any_of.as_array() {
                    let non_null: Vec<&serde_json::Value> = arr
                        .iter()
                        .filter(|v| v.get("type").and_then(|t| t.as_str()) != Some("null"))
                        .collect();
                    if non_null.len() == 1 {
                        return json_type_to_python(non_null[0]);
                    }
                }
            }
            // Handle enum values
            if schema.get("enum").is_some() {
                return "str".to_string();
            }
            "str".to_string()
        }
    }
}

/// Convert a JSON Schema type to a display type (includes enum values).
fn json_type_to_display(schema: &serde_json::Value) -> String {
    // Show enum values inline
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        let vals: Vec<String> = enum_vals
            .iter()
            .filter_map(|v| v.as_str().map(|s| format!("\"{}\"", s)))
            .collect();
        if !vals.is_empty() {
            return vals.join("|");
        }
    }
    json_type_to_python(schema)
}

/// Convert camelCase to snake_case.
fn camel_to_snake(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            // Don't add underscore if previous char is already uppercase (acronym)
            let prev = name.chars().nth(i - 1).unwrap_or('a');
            if prev.is_lowercase() || prev.is_numeric() {
                result.push('_');
            }
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Sanitize a name for Python: camelCase→snake_case, replace hyphens, avoid reserved words/builtins.
fn sanitize_python_name(name: &str) -> String {
    let mut result = camel_to_snake(name).replace('-', "_");
    if PYTHON_RESERVED.contains(&result.as_str()) || PYTHON_BUILTINS.contains(&result.as_str()) {
        result.push('_');
    }
    result
}

/// Convert a Rust/JSON default value to a Python literal.
fn python_default(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        serde_json::Value::String(s) => format!("{:?}", s),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(_) => "[]".to_string(),
        serde_json::Value::Object(_) => "{}".to_string(),
    }
}

/// Extract parameters from a JSON Schema object.
fn extract_params(schema: &serde_json::Value) -> Vec<SchemaParam> {
    let properties = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let required_set: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut params: Vec<SchemaParam> = properties
        .iter()
        .map(|(name, prop_schema)| {
            let is_required = required_set.contains(name.as_str());
            let python_type = json_type_to_python(prop_schema);
            let description = prop_schema
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let default = if !is_required {
                Some(
                    prop_schema
                        .get("default")
                        .map(python_default)
                        .unwrap_or_else(|| "None".to_string()),
                )
            } else {
                None
            };

            SchemaParam {
                name: sanitize_python_name(name),
                original_name: name.clone(),
                python_type,
                required: is_required,
                default,
                description,
            }
        })
        .collect();

    // Sort: required params first, then optional
    params.sort_by(|a, b| b.required.cmp(&a.required));
    params
}

/// Generate a compact one-line function signature for the execute_code description.
fn generate_function_signature(tool: &ToolDefinition) -> String {
    let fn_name = sanitize_python_name(&tool.name);
    let params = extract_params(&tool.input_schema);

    let param_parts: Vec<String> = params
        .iter()
        .map(|p| {
            let display_type = json_type_to_display(
                tool.input_schema
                    .get("properties")
                    .and_then(|props| props.get(&p.original_name))
                    .unwrap_or(&serde_json::Value::Null),
            );
            if let Some(ref default) = p.default {
                format!("{}: {} = {}", p.name, display_type, default)
            } else {
                format!("{}: {}", p.name, display_type)
            }
        })
        .collect();

    format!("{}({})", fn_name, param_parts.join(", "))
}

/// Generate a synchronous Python function stub for a single tool.
fn generate_function(tool: &ToolDefinition) -> String {
    let fn_name = sanitize_python_name(&tool.name);
    let params = extract_params(&tool.input_schema);

    let param_parts: Vec<String> = params
        .iter()
        .map(|p| {
            if let Some(ref default) = p.default {
                format!("{}: {} = {}", p.name, p.python_type, default)
            } else {
                format!("{}: {}", p.name, p.python_type)
            }
        })
        .collect();

    // Build the args dict, mapping Python names back to original schema keys
    let has_optional = params.iter().any(|p| p.default.is_some());
    let entries: String = params
        .iter()
        .map(|p| format!("\"{}\": {}", p.original_name, p.name))
        .collect::<Vec<_>>()
        .join(", ");

    let args_expr = if has_optional {
        format!(
            "{{k: v for k, v in {{{}}}.items() if v is not None}}",
            entries
        )
    } else {
        format!("{{{}}}", entries)
    };

    // Sanitize description for a single-line docstring
    let raw_desc = tool
        .description
        .replace('\\', "\\\\")
        .replace("\"\"\"", "\"\"\\\"")
        .replace('\n', " ")
        .replace('\r', "")
        .chars()
        .take(120)
        .collect::<String>()
        .trim()
        .to_string();

    format!(
        "def {}({}) -> str:\n    \"\"\"{}\"\"\"    \n    return _ptc_call(\"{}\", {})",
        fn_name,
        param_parts.join(", "),
        raw_desc,
        tool.name,
        args_expr
    )
}

/// Generate the full Python SDK preamble for all PTC tools.
///
/// The preamble includes:
/// - HTTP client setup (urllib.request, zero dependencies)
/// - The `_ptc_call()` bridge function
/// - One synchronous function per tool
pub fn generate_python_sdk(tools: &[ToolDefinition], ipc_port: u16) -> String {
    let functions: String = tools
        .iter()
        .map(generate_function)
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"# ── PTC SDK (auto-generated) ────────────────────────────────────────────
import json
import urllib.request

_PTC_PORT = {ipc_port}

def _ptc_call(name: str, args: dict) -> str:
    """Call a tool via the IPC bridge. Returns tool result as string."""
    data = json.dumps(args).encode("utf-8")
    req = urllib.request.Request(
        f"http://127.0.0.1:{{_PTC_PORT}}/tool/{{name}}",
        data=data,
        headers={{"Content-Type": "application/json"}},
    )
    try:
        with urllib.request.urlopen(req, timeout=300) as resp:
            return resp.read().decode("utf-8")
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        return f"Error calling {{name}}: {{e.code}} - {{body}}"
    except Exception as e:
        return f"Error calling {{name}}: {{e}}"

{functions}

# ── User code runs below ────────────────────────────────────────────────
"#
    )
}

/// Wrap user code with the SDK preamble.
pub fn wrap_user_code(sdk_preamble: &str, user_code: &str) -> String {
    format!("{}{}\n", sdk_preamble, user_code)
}

/// Generate compact one-line function signatures for the execute_code tool description.
///
/// This is what the LLM sees in its prompt — a compact reference of available functions.
pub fn generate_compact_reference(tools: &[ToolDefinition]) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Group tools: built-in vs MCP
    let mut builtin: Vec<&ToolDefinition> = Vec::new();
    let mut mcp: Vec<&ToolDefinition> = Vec::new();

    for tool in tools {
        if tool.name.starts_with("mcp_") {
            mcp.push(tool);
        } else {
            builtin.push(tool);
        }
    }

    for tool in &builtin {
        let sig = generate_function_signature(tool);
        let desc: String = tool
            .description
            .replace('\n', " ")
            .chars()
            .take(60)
            .collect();
        lines.push(format!("  {} -> str  # {}", sig, desc));
    }

    if !mcp.is_empty() {
        lines.push(String::new());
        lines.push("  # MCP tools:".to_string());
        for tool in &mcp {
            let sig = generate_function_signature(tool);
            let desc: String = tool
                .description
                .replace('\n', " ")
                .chars()
                .take(60)
                .collect();
            lines.push(format!("  {} -> str  # {}", sig, desc));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(name: &str, desc: &str, schema: serde_json::Value) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: schema,
        }
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("tabId"), "tab_id");
        assert_eq!(camel_to_snake("pressEnter"), "press_enter");
        assert_eq!(camel_to_snake("domainSuffix"), "domain_suffix");
        assert_eq!(camel_to_snake("file_read"), "file_read");
        assert_eq!(camel_to_snake("HTMLParser"), "htmlparser"); // Acronyms merge — acceptable for SDK names
    }

    #[test]
    fn test_sanitize_python_name() {
        assert_eq!(sanitize_python_name("type"), "type_");
        assert_eq!(sanitize_python_name("class"), "class_");
        assert_eq!(sanitize_python_name("from"), "from_");
        assert_eq!(sanitize_python_name("file-read"), "file_read");
        assert_eq!(sanitize_python_name("maxResults"), "max_results");
        // Builtins must be suffixed to avoid shadowing
        assert_eq!(sanitize_python_name("print"), "print_");
        assert_eq!(sanitize_python_name("input"), "input_");
        assert_eq!(sanitize_python_name("list"), "list_");
        assert_eq!(sanitize_python_name("id"), "id_");
        // Non-conflicting names are unchanged
        assert_eq!(sanitize_python_name("file_read"), "file_read");
        assert_eq!(sanitize_python_name("web_search"), "web_search");
    }

    #[test]
    fn test_generate_function_simple() {
        let tool = make_tool(
            "file_read",
            "Read a file",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" }
                },
                "required": ["path"]
            }),
        );

        let code = generate_function(&tool);
        assert!(code.contains("def file_read(path: str) -> str:"));
        assert!(code.contains("_ptc_call(\"file_read\""));
    }

    #[test]
    fn test_generate_function_with_optional() {
        let tool = make_tool(
            "web_search",
            "Search the web",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "default": 5 }
                },
                "required": ["query"]
            }),
        );

        let code = generate_function(&tool);
        assert!(code.contains("query: str"));
        assert!(code.contains("max_results: int = 5"));
        assert!(code.contains("if v is not None"));
    }

    #[test]
    fn test_generate_compact_reference() {
        let tools = vec![
            make_tool(
                "file_read",
                "Read file contents",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            ),
            make_tool(
                "mcp_github_list",
                "List GitHub repos",
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
        ];

        let ref_text = generate_compact_reference(&tools);
        assert!(ref_text.contains("file_read(path: str) -> str"));
        assert!(ref_text.contains("# MCP tools:"));
        assert!(ref_text.contains("mcp_github_list"));
    }

    #[test]
    fn test_generate_python_sdk() {
        let tools = vec![make_tool(
            "file_read",
            "Read a file",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        )];

        let sdk = generate_python_sdk(&tools, 12345);
        assert!(sdk.contains("_PTC_PORT = 12345"));
        assert!(sdk.contains("def _ptc_call(name: str, args: dict) -> str:"));
        assert!(sdk.contains("def file_read(path: str) -> str:"));
        assert!(sdk.contains("urllib.request"));
    }
}
