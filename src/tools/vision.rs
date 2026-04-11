//! Vision Tool - Analyze images using AI vision models
//!
//! Supports: OpenAI GPT-4 Vision, Anthropic Claude Vision, Ollama local models

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::path::Path;

pub struct VisionTool {
    config: VisionConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
struct VisionConfig {
    provider: VisionProvider,
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Debug, Clone)]
enum VisionProvider {
    OpenAI,
    Anthropic,
    Ollama,
}

impl VisionTool {
    pub fn new() -> Self {
        let config = Self::load_config();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");
        
        Self { config, client }
    }
    
    fn load_config() -> VisionConfig {
        // Try to load from config.toml first
        if let Ok(global_config) = crate::config::Config::load() {
            let vision_cfg = global_config.vision;
            let provider_str = vision_cfg.provider().to_lowercase();
            let provider = match provider_str.as_str() {
                "anthropic" | "claude" => VisionProvider::Anthropic,
                "ollama" | "local" => VisionProvider::Ollama,
                _ => VisionProvider::OpenAI,
            };
            
            // Use config values, fall back to defaults
            let api_key = vision_cfg.api_key.unwrap_or_default();
            let base_url = vision_cfg.base_url.unwrap_or_else(|| Self::default_base_url(&provider));
            let model = vision_cfg.model.unwrap_or_else(|| Self::default_model(&provider));
            
            // If we have a provider set in config, use it
            if !vision_cfg.provider.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
                return VisionConfig {
                    provider,
                    api_key,
                    base_url,
                    model,
                };
            }
        }
        
        // Fall back to environment variables
        let provider = match std::env::var("VISION_PROVIDER").unwrap_or_default().to_lowercase().as_str() {
            "anthropic" | "claude" => VisionProvider::Anthropic,
            "ollama" | "local" => VisionProvider::Ollama,
            _ => VisionProvider::OpenAI,
        };
        
        let api_key = std::env::var("VISION_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_default();
        
        VisionConfig {
            provider: provider.clone(),
            api_key,
            base_url: std::env::var("VISION_BASE_URL")
                .unwrap_or_else(|_| Self::default_base_url(&provider)),
            model: std::env::var("VISION_MODEL")
                .unwrap_or_else(|_| Self::default_model(&provider)),
        }
    }
    
    fn default_base_url(provider: &VisionProvider) -> String {
        match provider {
            VisionProvider::OpenAI => "https://api.openai.com/v1".to_string(),
            VisionProvider::Anthropic => "https://api.anthropic.com/v1".to_string(),
            VisionProvider::Ollama => "http://localhost:11434".to_string(),
        }
    }
    
    fn default_model(provider: &VisionProvider) -> String {
        match provider {
            VisionProvider::OpenAI => "gpt-4o".to_string(),
            VisionProvider::Anthropic => "claude-3-opus-20240229".to_string(),
            VisionProvider::Ollama => "llava".to_string(),
        }
    }
    
    fn is_configured(&self) -> bool {
        !self.config.api_key.is_empty() || matches!(self.config.provider, VisionProvider::Ollama)
    }
    
    async fn analyze_image_file(&self, file_path: &str, prompt: &str) -> anyhow::Result<String> {
        let image_data = tokio::fs::read(file_path).await?;
        let base64_image = BASE64.encode(&image_data);
        
        let mime_type = match Path::new(file_path).extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        
        match self.config.provider {
            VisionProvider::OpenAI => self.call_openai(&base64_image, mime_type, prompt).await,
            VisionProvider::Anthropic => self.call_anthropic(&base64_image, mime_type, prompt).await,
            VisionProvider::Ollama => self.call_ollama(&base64_image, prompt).await,
        }
    }
    
    async fn analyze_image_url(&self, image_url: &str, prompt: &str) -> anyhow::Result<String> {
        match self.config.provider {
            VisionProvider::OpenAI => self.call_openai_url(image_url, prompt).await,
            VisionProvider::Anthropic => {
                let image_data = self.client.get(image_url).send().await?.bytes().await?;
                let base64_image = BASE64.encode(&image_data);
                self.call_anthropic(&base64_image, "image/jpeg", prompt).await
            }
            VisionProvider::Ollama => {
                let image_data = self.client.get(image_url).send().await?.bytes().await?;
                let base64_image = BASE64.encode(&image_data);
                self.call_ollama(&base64_image, prompt).await
            }
        }
    }
    
    async fn call_openai(&self, base64_image: &str, mime_type: &str, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let data_url = format!("data:{};base64,{}"     , mime_type, base64_image);
        
        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": data_url}}
                ]
            }],
            "max_tokens": 1000
        });
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", response.text().await?));
        }
        
        let result: serde_json::Value = response.json().await?;
        Ok(result["choices"][0]["message"]["content"].as_str().unwrap_or("No response").to_string())
    }
    
    async fn call_openai_url(&self, image_url: &str, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url);
        
        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": image_url}}
                ]
            }],
            "max_tokens": 1000
        });
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", response.text().await?));
        }
        
        let result: serde_json::Value = response.json().await?;
        Ok(result["choices"][0]["message"]["content"].as_str().unwrap_or("No response").to_string())
    }
    
    async fn call_anthropic(&self, base64_image: &str, mime_type: &str, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/messages", self.config.base_url);
        
        let request_body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": 1000,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": mime_type, "data": base64_image}},
                    {"type": "text", "text": prompt}
                ]
            }]
        });
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", response.text().await?));
        }
        
        let result: serde_json::Value = response.json().await?;
        Ok(result["content"][0]["text"].as_str().unwrap_or("No response").to_string())
    }
    
    async fn call_ollama(&self, base64_image: &str, prompt: &str) -> anyhow::Result<String> {
        let url = format!("{}/api/generate", self.config.base_url);
        
        let request_body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "images": [base64_image],
            "stream": false
        });
        
        let response = self.client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API error: {}", response.text().await?));
        }
        
        let result: serde_json::Value = response.json().await?;
        Ok(result["response"].as_str().unwrap_or("No response").to_string())
    }
}

#[async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "vision"
    }
    
    fn description(&self) -> &str {
        "Analyze images using AI vision models. Can describe images, read text (OCR), \
         identify objects, answer questions about image content. \
         Supports: local image files and image URLs. \
         Requires VISION_API_KEY environment variable or config."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "image_path": {
                    "type": "string",
                    "description": "Path to local image file, or URL to image"
                },
                "prompt": {
                    "type": "string",
                    "description": "What to ask about the image",
                    "default": "Describe this image in detail"
                }
            },
            "required": ["image_path"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        if !self.is_configured() {
            return Ok(ToolResult::error(
                "Vision not configured.\n\n\
                Add to your config.toml (~/.horcrux/config.toml):\n\
                [vision]\n\
                provider = \"openai\"  # or \"anthropic\", \"ollama\"\n\
                api_key = \"your-api-key\"\n\
                # model = \"gpt-4o\"  # optional\n\n\
                Or set environment variable:\n\
                VISION_API_KEY=your-key".to_string()
            ));
        }
        
        let image_path = args["image_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing image_path"))?;
        let prompt = args["prompt"].as_str().unwrap_or("Describe this image");
        
        let result = if image_path.starts_with("http") {
            self.analyze_image_url(image_path, prompt).await
        } else {
            if !Path::new(image_path).exists() {
                return Ok(ToolResult::error(format!("File not found: {}", image_path)));
            }
            self.analyze_image_file(image_path, prompt).await
        };
        
        match result {
            Ok(analysis) => Ok(ToolResult::success(analysis)),
            Err(e) => Ok(ToolResult::error(format!("Vision failed: {}", e)))
        }
    }
}
