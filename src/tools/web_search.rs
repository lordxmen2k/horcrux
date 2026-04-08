//! Web Search Tool - Search the internet using API-based providers
//!
//! Supports: Tavily (primary), Serper.dev, Brave Search
//! API key loaded from config.toml [web_search] section
//! Self-healing: Reloads config on each call, provides diagnostic info

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, RwLock};

pub struct WebSearchTool {
    client: reqwest::Client,
    config: Arc<RwLock<Option<WebSearchProviderConfig>>>,
}

#[derive(Debug, Clone)]
struct WebSearchProviderConfig {
    provider: Provider,
    api_key: String,
}

#[derive(Debug, Clone)]
enum Provider {
    Tavily,
    Serper,
    Brave,
}

impl WebSearchProviderConfig {
    fn provider_name(&self) -> &'static str {
        match self.provider {
            Provider::Tavily => "Tavily",
            Provider::Serper => "Serper.dev",
            Provider::Brave => "Brave Search",
        }
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        
        // Initial config load
        let config = Self::load_config();
        
        if config.is_some() {
            tracing::info!("✅ WebSearchTool initialized with API key");
        } else {
            tracing::warn!("⚠️ WebSearchTool: No API key configured");
        }
            
        Self { 
            client, 
            config: Arc::new(RwLock::new(config)),
        }
    }
    
    /// Load or reload config from file
    fn load_config() -> Option<WebSearchProviderConfig> {
        // Try to load from the main config file
        match crate::config::Config::load() {
            Ok(global_config) => {
                if global_config.web_search.is_configured() {
                    let provider = match global_config.web_search.provider().to_lowercase().as_str() {
                        "tavily" => Provider::Tavily,
                        "serper" => Provider::Serper,
                        "brave" => Provider::Brave,
                        _ => Provider::Tavily,
                    };
                    tracing::info!("Web search config loaded: {}", global_config.web_search.provider());
                    return Some(WebSearchProviderConfig {
                        provider,
                        api_key: global_config.web_search.api_key.unwrap(),
                    });
                } else {
                    tracing::debug!("Web search not configured: provider={:?}, key_exists={}", 
                        global_config.web_search.provider,
                        global_config.web_search.api_key.is_some());
                }
            }
            Err(e) => {
                tracing::error!("Failed to load config: {}", e);
            }
        }
        
        // Fallback: Try environment variables
        if let Ok(api_key) = std::env::var("TAVILY_API_KEY") {
            tracing::info!("Web search config loaded from TAVILY_API_KEY env var");
            return Some(WebSearchProviderConfig {
                provider: Provider::Tavily,
                api_key,
            });
        }
        if let Ok(api_key) = std::env::var("SERPER_API_KEY") {
            tracing::info!("Web search config loaded from SERPER_API_KEY env var");
            return Some(WebSearchProviderConfig {
                provider: Provider::Serper,
                api_key,
            });
        }
        if let Ok(api_key) = std::env::var("BRAVE_API_KEY") {
            tracing::info!("Web search config loaded from BRAVE_API_KEY env var");
            return Some(WebSearchProviderConfig {
                provider: Provider::Brave,
                api_key,
            });
        }
        
        None
    }
    
    /// Reload config (call this before each search to pick up changes)
    fn reload_config(&self) {
        let new_config = Self::load_config();
        if let Ok(mut config) = self.config.write() {
            *config = new_config;
        }
    }
    
    /// Get diagnostic information about why config might be failing
    fn get_diagnostic_info() -> String {
        let mut info = String::from("Web Search Diagnostic Info:\n\n");
        
        // Check config file
        let config_path = crate::config::Config::config_path();
        info.push_str(&format!("Config file path: {}\n", config_path.display()));
        info.push_str(&format!("Config file exists: {}\n", config_path.exists()));
        
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    info.push_str("Config file content length: ");
                    info.push_str(&content.len().to_string());
                    info.push_str(" bytes\n");
                    
                    // Check for web_search section
                    if content.contains("[web_search]") {
                        info.push_str("✓ Found [web_search] section\n");
                        
                        // Check for api_key
                        if content.contains("api_key") {
                            info.push_str("✓ Found api_key field\n");
                            
                            // Check if it has a value
                            if content.contains(r#"api_key = ""#) || content.contains(r#"api_key=""#) {
                                info.push_str("✗ api_key appears to be empty\n");
                            } else if content.contains("tvly-") || content.contains("sk-") {
                                info.push_str("✓ api_key appears to have a value\n");
                            }
                        } else {
                            info.push_str("✗ No api_key field found\n");
                        }
                    } else {
                        info.push_str("✗ No [web_search] section found\n");
                    }
                }
                Err(e) => {
                    info.push_str(&format!("✗ Failed to read config: {}\n", e));
                }
            }
        }
        
        // Check environment variables
        info.push_str("\nEnvironment variables:\n");
        info.push_str(&format!("  TAVILY_API_KEY: {}\n", 
            if std::env::var("TAVILY_API_KEY").is_ok() { "✓ Set" } else { "✗ Not set" }));
        info.push_str(&format!("  SERPER_API_KEY: {}\n",
            if std::env::var("SERPER_API_KEY").is_ok() { "✓ Set" } else { "✗ Not set" }));
        info.push_str(&format!("  BRAVE_API_KEY: {}\n",
            if std::env::var("BRAVE_API_KEY").is_ok() { "✓ Set" } else { "✗ Not set" }));
        
        info.push_str("\nTo fix:\n");
        info.push_str("1. Add to ~/.horcrux/config.toml:\n");
        info.push_str("   [web_search]\n");
        info.push_str("   provider = \"tavily\"\n");
        info.push_str("   api_key = \"tvly-your-key-here\"\n\n");
        info.push_str("2. Or set environment variable:\n");
        info.push_str("   export TAVILY_API_KEY=tvly-your-key\n\n");
        info.push_str("Get free key at https://tavily.com (1000 searches/month)");
        
        info
    }
    
    /// Search using configured provider
    async fn search(&self, query: &str, max_results: usize) -> anyhow::Result<Vec<WebResult>> {
        // Reload config to pick up any changes
        self.reload_config();
        
        let config = self.config.read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
            .clone()
            .ok_or_else(|| anyhow::anyhow!(
                "No web search API key configured.\n\n{}\n\nTo fix:\n{}",
                Self::get_diagnostic_info(),
                "Run 'horcrux setup' or edit ~/.horcrux/config.toml"
            ))?;
        
        match config.provider {
            Provider::Tavily => self.search_tavily(query, max_results, &config.api_key).await,
            Provider::Serper => self.search_serper(query, max_results, &config.api_key).await,
            Provider::Brave => self.search_brave(query, max_results, &config.api_key).await,
        }
    }
    
    /// Search using Tavily API
    async fn search_tavily(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let request_body = serde_json::json!({
            "query": query,
            "max_results": max_results,
            "api_key": api_key,
            "search_depth": "advanced",
            "include_answer": true,
        });
        
        let response = self.client
            .post("https://api.tavily.com/search")
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            
            // Check for specific error types
            if status == 401 || status == 403 {
                return Err(anyhow::anyhow!(
                    "Authentication failed. Your Tavily API key may be invalid or expired.\n\
                    Status: {}\nError: {}\n\n\
                    Please check your API key at https://tavily.com",
                    status, error_text
                ));
            }
            if status == 429 {
                return Err(anyhow::anyhow!(
                    "Rate limit exceeded. You've used your quota of 1000 free searches/month.\n\
                    Please wait or upgrade your Tavily plan."
                ));
            }
            
            return Err(anyhow::anyhow!(
                "Tavily API error: {} - {}", status, error_text
            ));
        }
        
        let result: TavilyResponse = response.json().await?;
        
        let mut results = Vec::new();
        
        // Add the AI answer if present
        if let Some(answer) = result.answer {
            results.push(WebResult {
                title: "AI Summary".to_string(),
                url: "https://tavily.com".to_string(),
                snippet: answer,
                source: "tavily-ai".to_string(),
            });
        }
        
        // Add regular results
        for r in result.results {
            results.push(WebResult {
                title: r.title,
                url: r.url,
                snippet: r.content,
                source: r.source.unwrap_or_else(|| "web".to_string()),
            });
        }
        
        Ok(results)
    }
    
    /// Search using Serper.dev API
    async fn search_serper(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let request_body = serde_json::json!({
            "q": query,
            "num": max_results,
        });
        
        let response = self.client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", api_key)
            .json(&request_body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Serper API error: {} - {}", status, error_text
            ));
        }
        
        let result: SerperResponse = response.json().await?;
        
        let results: Vec<WebResult> = result.organic
            .into_iter()
            .map(|r| WebResult {
                title: r.title,
                url: r.link,
                snippet: r.snippet,
                source: r.source.unwrap_or_else(|| "google".to_string()),
            })
            .collect();
        
        Ok(results)
    }
    
    /// Search using Brave Search API
    async fn search_brave(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let response = self.client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &max_results.to_string())])
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Brave Search API error: {} - {}", status, error_text
            ));
        }
        
        let result: BraveResponse = response.json().await?;
        
        let results: Vec<WebResult> = result.web.results
            .into_iter()
            .map(|r| WebResult {
                title: r.title,
                url: r.url,
                snippet: r.description,
                source: "brave".to_string(),
            })
            .collect();
        
        Ok(results)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    
    fn description(&self) -> &str {
        "Search the web for current information. \
         Use this when you need up-to-date facts, news, or information not in your knowledge base. \
         Returns search results with titles, URLs, and snippets. \
         Requires web search API key to be configured."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let max_results = args["max_results"].as_u64().unwrap_or(5) as usize;
        
        match self.search(query, max_results).await {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(ToolResult::success("No results found for the query."));
                }
                
                let mut output = format!("Web search results for '{}':\n\n", query);
                
                for (i, result) in results.iter().enumerate() {
                    output.push_str(&format!(
                        "[{}] {}\n{}
{}
\n",
                        i + 1,
                        result.title,
                        result.url,
                        result.snippet
                    ));
                }
                
                Ok(ToolResult::success(output))
            }
            Err(e) => {
                // Return diagnostic info with the error
                Ok(ToolResult::error(format!(
                    "Web search failed: {}\n\n{}",
                    e,
                    Self::get_diagnostic_info()
                )))
            }
        }
    }
}

/// A single web search result
#[derive(Debug)]
struct WebResult {
    title: String,
    url: String,
    snippet: String,
    source: String,
}

// Tavily API response structures
#[derive(Debug, serde::Deserialize)]
struct TavilyResponse {
    answer: Option<String>,
    results: Vec<TavilyResult>,
}

#[derive(Debug, serde::Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
    #[serde(default)]
    source: Option<String>,
}

// Serper API response structures
#[derive(Debug, serde::Deserialize)]
struct SerperResponse {
    organic: Vec<SerperResult>,
}

#[derive(Debug, serde::Deserialize)]
struct SerperResult {
    title: String,
    link: String,
    snippet: String,
    #[serde(default)]
    source: Option<String>,
}

// Brave Search API response structures
#[derive(Debug, serde::Deserialize)]
struct BraveResponse {
    web: BraveWebResults,
}

#[derive(Debug, serde::Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Debug, serde::Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    description: String,
}
