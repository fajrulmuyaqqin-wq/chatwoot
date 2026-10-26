use crate::models::captain::CaptainCustomTool;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::info;

fn base64_encode(input: &str) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        out.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(CHARSET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

/// Executes an HTTP request to an external tool with given parameter arguments
pub async fn execute_custom_tool(
    tool: &CaptainCustomTool,
    arguments: &Value,
) -> Result<Value, String> {
    info!(
        "Executing custom tool '{}' ({}) with args: {}",
        tool.title, tool.slug, arguments
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    // 1. Process URL & Placeholder Replacement
    let mut target_url = tool.endpoint_url.clone();
    let mut unused_args = serde_json::Map::new();

    if let Some(arg_map) = arguments.as_object() {
        for (k, v) in arg_map {
            let val_str = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => v.to_string(),
            };

            let placeholder = format!("{{{{{}}}}}", k);
            if target_url.contains(&placeholder) {
                target_url = target_url.replace(&placeholder, &val_str);
            } else {
                unused_args.insert(k.clone(), v.clone());
            }
        }
    }

    // 2. Build Headers
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static("Chatwoot-Captain-Agent/1.0"),
    );

    if let Some(custom_headers) = tool.headers.as_ref().and_then(|h| h.as_object()) {
        for (hk, hv) in custom_headers {
            let mut val_str = hv.as_str().unwrap_or("").to_string();
            // Replace placeholders in headers if any
            if let Some(arg_map) = arguments.as_object() {
                for (k, v) in arg_map {
                    let placeholder = format!("{{{{{}}}}}", k);
                    if val_str.contains(&placeholder) {
                        let rep = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                        val_str = val_str.replace(&placeholder, &rep);
                    }
                }
            }

            if let (Ok(h_name), Ok(h_val)) = (
                HeaderName::from_bytes(hk.as_bytes()),
                HeaderValue::from_str(&val_str),
            ) {
                headers.insert(h_name, h_val);
            }
        }
    }

    // 3. Process Authentication
    let auth_type = tool.auth_type.as_deref().unwrap_or("none");
    let auth_config = tool.auth_config.as_ref().unwrap_or(&Value::Null);

    match auth_type {
        "bearer" => {
            if let Some(token) = auth_config
                .get("token")
                .or_else(|| auth_config.get("api_key"))
                .and_then(|v| v.as_str())
            {
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
                    headers.insert(reqwest::header::AUTHORIZATION, val);
                }
            }
        }
        "basic" => {
            let username = auth_config.get("username").and_then(|v| v.as_str()).unwrap_or("");
            let password = auth_config.get("password").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = base64_encode(&format!("{}:{}", username, password));
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", encoded)) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }
        "api_key" => {
            let key_name = auth_config.get("key").and_then(|v| v.as_str()).unwrap_or("X-API-Key");
            let key_val = auth_config.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let key_in = auth_config.get("in").and_then(|v| v.as_str()).unwrap_or("header");

            if key_in == "query" {
                let separator = if target_url.contains('?') { "&" } else { "?" };
                target_url = format!("{}{}{}={}", target_url, separator, key_name, key_val);
            } else if let (Ok(h_name), Ok(h_val)) = (
                HeaderName::from_bytes(key_name.as_bytes()),
                HeaderValue::from_str(key_val),
            ) {
                headers.insert(h_name, h_val);
            }
        }
        _ => {}
    }

    // 4. Build and Send Request according to HTTP Method
    let method = tool.http_method.to_uppercase();
    let mut req_builder = match method.as_str() {
        "POST" => client.post(&target_url),
        "PUT" => client.put(&target_url),
        "PATCH" => client.patch(&target_url),
        "DELETE" => client.delete(&target_url),
        _ => {
            let mut req = client.get(&target_url);
            for (k, v) in unused_args {
                let v_str = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                req = req.query(&[(k, v_str)]);
            }
            req
        }
    };

    req_builder = req_builder.headers(headers);

    // If method has body (POST/PUT/PATCH)
    if ["POST", "PUT", "PATCH"].contains(&method.as_str()) {
        if let Some(tpl) = tool.request_template.as_deref().filter(|t| !t.trim().is_empty()) {
            let mut final_body = tpl.to_string();
            if let Some(arg_map) = arguments.as_object() {
                for (k, v) in arg_map {
                    let placeholder = format!("{{{{{}}}}}", k);
                    let rep = match v {
                        Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    };
                    final_body = final_body.replace(&placeholder, &rep);
                }
            }
            req_builder = req_builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(final_body);
        } else if !arguments.is_null() && arguments != &json!({}) {
            req_builder = req_builder.json(arguments);
        }
    }

    // 5. Execute Request
    let resp = req_builder.send().await.map_err(|e| format!("HTTP request failed: {e}"))?;
    let status_code = resp.status().as_u16();

    let text_content = resp.text().await.unwrap_or_default();
    let json_val = serde_json::from_str::<Value>(&text_content).unwrap_or_else(|_| {
        json!({
            "status": status_code,
            "raw_response": text_content
        })
    });

    info!(
        "Custom tool '{}' executed successfully with status {}",
        tool.title, status_code
    );

    Ok(json!({
        "status": status_code,
        "data": json_val
    }))
}

/// Helper function to test tool configuration directly from UI form
pub async fn test_tool_execution(payload: &Value) -> Result<Value, String> {
    let endpoint_url = payload
        .get("endpoint_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing endpoint_url".to_string())?;

    let http_method = payload
        .get("http_method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static("Chatwoot-Captain-Tester/1.0"),
    );

    if let Some(custom_headers) = payload.get("headers").and_then(|h| h.as_object()) {
        for (k, v) in custom_headers {
            let val_str = v.as_str().unwrap_or("");
            if let (Ok(h_name), Ok(h_val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(val_str),
            ) {
                headers.insert(h_name, h_val);
            }
        }
    }

    // Process Auth
    let auth_type = payload.get("auth_type").and_then(|v| v.as_str()).unwrap_or("none");
    let auth_config = payload.get("auth_config").unwrap_or(&Value::Null);

    let mut final_url = endpoint_url.to_string();

    match auth_type {
        "bearer" => {
            if let Some(token) = auth_config
                .get("token")
                .or_else(|| auth_config.get("api_key"))
                .and_then(|v| v.as_str())
            {
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
                    headers.insert(reqwest::header::AUTHORIZATION, val);
                }
            }
        }
        "basic" => {
            let username = auth_config.get("username").and_then(|v| v.as_str()).unwrap_or("");
            let password = auth_config.get("password").and_then(|v| v.as_str()).unwrap_or("");
            let encoded = base64_encode(&format!("{}:{}", username, password));
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", encoded)) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }
        "api_key" => {
            let key_name = auth_config.get("key").and_then(|v| v.as_str()).unwrap_or("X-API-Key");
            let key_val = auth_config.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let key_in = auth_config.get("in").and_then(|v| v.as_str()).unwrap_or("header");

            if key_in == "query" {
                let separator = if final_url.contains('?') { "&" } else { "?" };
                final_url = format!("{}{}{}={}", final_url, separator, key_name, key_val);
            } else if let (Ok(h_name), Ok(h_val)) = (
                HeaderName::from_bytes(key_name.as_bytes()),
                HeaderValue::from_str(key_val),
            ) {
                headers.insert(h_name, h_val);
            }
        }
        _ => {}
    }

    let mut req_builder = match http_method.to_uppercase().as_str() {
        "POST" => client.post(&final_url),
        "PUT" => client.put(&final_url),
        "PATCH" => client.patch(&final_url),
        "DELETE" => client.delete(&final_url),
        _ => client.get(&final_url),
    };

    req_builder = req_builder.headers(headers);

    let resp = req_builder.send().await.map_err(|e| format!("Request failed: {e}"))?;
    let status = resp.status().as_u16();
    let body_text = resp.text().await.unwrap_or_default();
    let body_json = serde_json::from_str::<Value>(&body_text).unwrap_or_else(|_| json!(body_text));

    Ok(json!({
        "status": status,
        "response": body_json
    }))
}
