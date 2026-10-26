use crate::models::captain::CaptainCustomTool;
use serde_json::{json, Value};

/// Converts a CaptainCustomTool record into standard OpenAI/Gemini Function Calling tool format
pub fn to_llm_tool_definition(tool: &CaptainCustomTool) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required_params = Vec::new();

    if let Some(params) = tool.param_schema.as_ref().and_then(|p| p.as_array()) {
        for param in params {
            if let Some(name) = param.get("name").and_then(|n| n.as_str()) {
                let p_type = param.get("type").and_then(|t| t.as_str()).unwrap_or("string");
                let description = param.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let is_required = param.get("required").and_then(|r| r.as_bool()).unwrap_or(false);

                properties.insert(
                    name.to_string(),
                    json!({
                        "type": p_type,
                        "description": description
                    }),
                );

                if is_required {
                    required_params.push(json!(name));
                }
            }
        }
    }

    let fn_name = if tool.slug.starts_with("custom_") {
        tool.slug.clone()
    } else {
        format!("custom_{}", tool.slug)
    };

    let desc = tool
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(&tool.title);

    json!({
        "type": "function",
        "function": {
            "name": fn_name,
            "description": desc,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required_params
            }
        }
    })
}
