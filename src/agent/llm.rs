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
                .unwrap_or(0.7),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
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
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
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

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            tools: tools_value,
            tool_choice: if tools.is_some() { Some(serde_json::json!("auto")) } else { None },
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("LLM request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
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

        Ok(LlmResponse {
            content: message.content.clone().unwrap_or_default(),
            tool_calls: message.tool_calls.clone().unwrap_or_default(),
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
        let response = self.chat(&messages, None).await?;
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
