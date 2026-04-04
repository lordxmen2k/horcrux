//! HTTP Tool - Make HTTP requests

use super::{Tool, ToolResult};
use async_trait::async_trait;
use reqwest::Method;
use serde_json::Value;

pub struct HttpTool {
    client: reqwest::Client,
}

impl HttpTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");
        Self { client }
    }

    async fn make_request(
        &self,
        method: Method,
        url: &str,
        headers: Option<Value>,
        body: Option<Value>,
    ) -> anyhow::Result<ToolResult> {
        let mut request = self.client.request(method.clone(), url);

        // Add headers if provided
        if let Some(headers_val) = headers {
            if let Some(headers_obj) = headers_val.as_object() {
                for (key, value) in headers_obj {
                    if let Some(val_str) = value.as_str() {
                        request = request.header(key, val_str);
                    }
                }
            }
        }

        // Add body if provided
        if let Some(body_val) = body {
            request = request.json(&body_val);
        }

        // Execute request
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "HTTP request failed: {}",
                    e
                )));
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        
        // Try to parse as JSON first, fall back to text
        let body_text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read response body: {}",
                    e
                )));
            }
        };

        // Format response
        let mut output = format!("Status: {}\n", status);
        
        output.push_str("Headers:\n");
        for (name, value) in headers.iter() {
            output.push_str(&format!("  {}: {}\n", name, value.to_str().unwrap_or("(binary)")));
        }
        
        output.push_str(&format!("\nBody:\n{}", body_text));

        if status.is_success() {
            Ok(ToolResult::success(output))
        } else {
            Ok(ToolResult::error(format!(
                "HTTP {}\n{}",
                status, output
            )))
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to fetch data from APIs. YOU MUST PARSE JSON RESPONSES!\n\
         When you get JSON like [123, 456, 789], these are IDs - fetch each one!\n\
         When you get {\"title\": \"...\", \"url\": \"...\"}, extract those fields!\n\
         NEVER show raw JSON arrays or objects to users - ALWAYS extract meaningful data.\n\
         Example: Hacker News returns story IDs [12345, 67890] - you must fetch /item/12345.json etc."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "description": "HTTP method",
                    "default": "GET"
                },
                "url": {
                    "type": "string",
                    "description": "URL to request"
                },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers as key-value pairs (optional)",
                    "optional": true
                },
                "body": {
                    "type": "object",
                    "description": "JSON body for POST/PUT/PATCH (optional)",
                    "optional": true
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let method_str = args["method"].as_str().unwrap_or("GET");
        let url = args["url"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: url"))?;
        
        let headers = args.get("headers").cloned();
        let body = args.get("body").cloned();

        let method = match method_str.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            _ => Method::GET,
        };

        self.make_request(method, url, headers, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_tool_creation() {
        let tool = HttpTool::new();
        assert_eq!(tool.name(), "http");
    }
}
