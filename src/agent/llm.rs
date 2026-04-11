//! LLM Client for Agent - Supports OpenAI-compatible APIs and Ollama

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// LLM configuration from environment variables
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub timeout_secs: u64,
    pub max_tokens: Option<i32>,
    pub temperature: f32,
}

impl LlmConfig {
    pub fn from_env() -> Self {
        Self {
            model: std::env::var("HORCRUX_LLM_MODEL")
                .or_else(|_| std::env::var("OPENAI_MODEL"))
                .unwrap_or_else(|_| "gpt-4o-mini".into()),
            base_url: std::env::var("HORCRUX_LLM_URL")
                .or_else(|_| std::env::var("OPENAI_BASE_URL"))
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            api_key: std::env::var("HORCRUX_LLM_API_KEY")
                .or_else(|_| std::env::var("OPENAI_API_KEY"))
                .unwrap_or_else(|_| "ollama".into()),
            timeout_secs: std::env::var("HORCRUX_LLM_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            max_tokens: std::env::var("HORCRUX_LLM_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse().ok()),
            temperature: std::env::var("HORCRUX_LLM_TEMPERATURE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.6), // Moonshot K2 requires temperature=0.6
        }
    }

    /// Check if this is likely an Ollama endpoint
    pub fn is_ollama(&self) -> bool {
        self.base_url.contains("11434") || self.base_url.contains("ollama")
    }
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "should_skip_tool_call_id")]
    pub tool_call_id: Option<String>,
}

/// Skip tool_call_id if None OR empty string
fn should_skip_tool_call_id(value: &Option<String>) -> bool {
    match value {
        None => true,
        Some(s) => s.is_empty(),
    }
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let id: String = tool_call_id.into();
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: None,
            // Only set tool_call_id if it's not empty - prevents API errors
            tool_call_id: if id.is_empty() { None } else { Some(id) },
        }
    }
}

/// A tool definition for function calling
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// A tool call from the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

/// LLM response
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

// OpenAI API types
#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    temperature: f32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>, // Disable thinking to avoid reasoning_content issues
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<TokenUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

/// LLM Client for chat completions
pub struct LlmClient {
    config: LlmConfig,
    client: Client,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }

    pub fn from_env() -> Self {
        Self::new(LlmConfig::from_env())
    }

    /// Send a chat completion request
    /// 
    /// `force_tools`: When Some("required"), forces the model to call at least one tool
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
        force_tools: Option<&str>,
    ) -> Result<LlmResponse> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        // Convert tools to OpenAI format
        let tools_value = tools.map(|ts| {
            ts.iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect::<Vec<_>>()
        });

        // Determine tool_choice: "auto" (default), "none", or "required"
        let tool_choice = if force_tools == Some("required") && tools.is_some() {
            Some(serde_json::json!("required"))
        } else if tools.is_some() {
            Some(serde_json::json!("auto"))
        } else {
            None
        };

        // AGGRESSIVE FILTER: Remove any tool messages with empty/missing tool_call_id
        // This prevents API errors like "tool_call_id  is not found"
        let filtered_messages: Vec<ChatMessage> = messages.iter().filter(|msg| {
            if msg.role == "tool" {
                let has_valid_id = msg.tool_call_id.as_ref()
                    .map(|id| !id.is_empty())
                    .unwrap_or(false);
                if !has_valid_id {
                    eprintln!("⚠️ FILTERED OUT tool message with empty tool_call_id before API call");
                }
                has_valid_id
            } else {
                true
            }
        }).cloned().collect();
        
        // DEBUG: Verify no empty tool_call_id remains
        for (i, msg) in filtered_messages.iter().enumerate() {
            if msg.role == "tool" {
                let id_preview = msg.tool_call_id.as_ref()
                    .map(|s| if s.len() > 20 { format!("{}...", &s[..20]) } else { s.clone() })
                    .unwrap_or_else(|| "<NONE>".to_string());
                eprintln!("  [API] Message {}: role=tool, tool_call_id={}", i, id_preview);
            }
        }

        // DEBUG: Check message count before moving
        let msg_count = filtered_messages.len();
        
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: filtered_messages,
            tools: tools_value,
            tool_choice: tool_choice.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            stream: false,
            thinking: Some(ThinkingConfig { kind: "disabled".to_string() }), // Disable thinking
        };
        
        // DEBUG: Print JSON when we have many messages (error condition)
        if msg_count > 10 {
            match serde_json::to_string(&request) {
                Ok(json) => {
                    // Check for empty tool_call_id in the actual JSON
                    let empty_count = json.matches("\"tool_call_id\":\"\"").count();
                    let null_count = json.matches("\"tool_call_id\":null").count();
                    if empty_count > 0 || null_count > 0 {
                        eprintln!("🐛 CRITICAL: Found {} empty and {} null tool_call_id in JSON!", empty_count, null_count);
                        eprintln!("🐛 JSON (first 4000 chars): {}", &json[..json.len().min(4000)]);
                    }
                }
                Err(e) => eprintln!("🐛 Failed to serialize: {}", e),
            }
        }
        
        // Make request with retry logic for 429 rate limiting
        let mut retries = 0;
        let max_retries = 3;
        let response = loop {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.config.api_key)
                .json(&request)
                .send()
                .await
                .map_err(|e| anyhow!("LLM request failed: {}", e))?;
            
            if resp.status() == 429 && retries < max_retries {
                retries += 1;
                let delay = std::time::Duration::from_secs(retries as u64 * 2); // 2s, 4s, 6s
                eprintln!("⚠️ Rate limited (429), retrying in {}s... (attempt {}/{})", 
                    delay.as_secs(), retries, max_retries);
                tokio::time::sleep(delay).await;
                continue;
            }
            
            break resp;
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status == 429 {
                return Err(anyhow!("Rate limit exceeded. Please wait a moment and try again."));
            }
            return Err(anyhow!("LLM API error {}: {}", status, body));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse LLM response: {}", e))?;

        if chat_response.choices.is_empty() {
            return Err(anyhow!("No response from LLM"));
        }

        let choice = &chat_response.choices[0];
        let message = &choice.message;
        
        let tool_calls = message.tool_calls.clone().unwrap_or_default();
        if force_tools == Some("required") {
            eprintln!("🔧 Forced tools: got {} tool_calls, content='{}'", 
                tool_calls.len(), 
                message.content.clone().unwrap_or_default().chars().take(50).collect::<String>());
        }

        Ok(LlmResponse {
            content: message.content.clone().unwrap_or_default(),
            tool_calls,
            finish_reason: choice.finish_reason.clone(),
            usage: chat_response.usage,
        })
    }

    /// Simple non-tool chat for quick responses
    pub async fn chat_simple(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(user_message),
        ];
        let response = self.chat(&messages, None, None).await?;
        Ok(response.content)
    }

    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    pub fn is_available(&self) -> bool {
        // Check if API key is set (for cloud) or if it's Ollama
        !self.config.api_key.is_empty() || self.config.is_ollama()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let system = ChatMessage::system("You are a helpful assistant");
        assert_eq!(system.role, "system");
        
        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, "user");
        
        let assistant = ChatMessage::assistant("Hi there");
        assert_eq!(assistant.role, "assistant");
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::from_env();
        assert!(!config.model.is_empty());
        assert!(!config.base_url.is_empty());
        assert!(config.temperature > 0.0);
    }

    #[test]
    fn test_ollama_detection() {
        let mut config = LlmConfig::from_env();
        config.base_url = "http://localhost:11434/v1".into();
        assert!(config.is_ollama());
        
        config.base_url = "https://api.openai.com/v1".into();
        assert!(!config.is_ollama());
    }
}
