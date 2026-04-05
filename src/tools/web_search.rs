//! Web Search Tool - Search the internet using API-based providers
//!
//! Supports: Tavily (primary), Serper.dev, Brave Search
//! API key loaded from config.toml [web_search] section

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;

pub struct WebSearchTool {
    client: reqwest::Client,
    config: Option<WebSearchProviderConfig>,
}

#[derive(Debug, Clone)]
struct WebSearchProviderConfig {
    provider: Provider,
    api_key: String,
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

#[derive(Debug, Clone)]
enum Provider {
    Tavily,
    Serper,
    Brave,
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        
        // Load from config file
        let config = Self::load_config_from_file();
        
        if let Some(ref cfg) = config {
            println!("✅ WebSearchTool: Using {} API", cfg.provider_name());
        } else {
            println!("⚠️ WebSearchTool: No API key configured (add to ~/.horcrux/config.toml)");
        }
            
        Self { client, config }
    }
    
    /// Load config from the main config file
    fn load_config_from_file() -> Option<WebSearchProviderConfig> {
        if let Ok(global_config) = crate::config::Config::load() {
            if global_config.web_search.is_configured() {
                let provider = match global_config.web_search.provider().to_lowercase().as_str() {
                    "tavily" => Provider::Tavily,
                    "serper" => Provider::Serper,
                    "brave" => Provider::Brave,
                    _ => Provider::Tavily,
                };
                return Some(WebSearchProviderConfig {
                    provider,
                    api_key: global_config.web_search.api_key.unwrap(),
                });
            }
        }
        None
    }
    
    fn provider_name(&self) -> &'static str {
        match self.config.as_ref().map(|c| &c.provider) {
            Some(Provider::Tavily) => "Tavily",
            Some(Provider::Serper) => "Serper.dev",
            Some(Provider::Brave) => "Brave Search",
            None => "None",
        }
    }
    
    /// Search using configured provider
    async fn search(&self, query: &str, max_results: usize) -> anyhow::Result<Vec<WebResult>> {
        let config = self.config.as_ref()
            .ok_or_else(|| anyhow::anyhow!(
                "No web search API key configured.\n\n\
                Add to ~/.horcrux/config.toml:\n\n\
                [web_search]\n\
                provider = \"tavily\"\n\
                api_key = \"your-api-key\"\n\n\
                Get free key at https://tavily.com (1000 searches/month)"
            ))?;
        
        match config.provider {
            Provider::Tavily => self.search_tavily(query, max_results, &config.api_key).await,
            Provider::Serper => self.search_serper(query, max_results, &config.api_key).await,
            Provider::Brave => self.search_brave(query, max_results, &config.api_key).await,
        }
    }
    
    /// Tavily search - AI-optimized results
    async fn search_tavily(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let payload = serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": "basic",
            "include_answer": false,
            "include_images": false,
            "include_raw_content": false
        });
        
        let response = self.client
            .post("https://api.tavily.com/search")
            .json(&payload)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Tavily API error {}: {}", status, text));
        }
        
        let json: Value = response.json().await?;
        let mut results = Vec::new();
        
        if let Some(results_arr) = json.get("results").and_then(|r| r.as_array()) {
            for r in results_arr {
                if let Some(title) = r.get("title").and_then(|t| t.as_str()) {
                    let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    let content = r.get("content")
                        .or_else(|| r.get("snippet"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("");
                        
                    results.push(WebResult {
                        title: title.to_string(),
                        url: url.to_string(),
                        snippet: content.to_string(),
                    });
                }
            }
        }
        
        Ok(results)
    }
    
    /// Serper.dev search - Google Search API
    async fn search_serper(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let payload = serde_json::json!({
            "q": query,
            "num": max_results
        });
        
        let response = self.client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", api_key)
            .json(&payload)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Serper API error {}: {}", status, text));
        }
        
        let json: Value = response.json().await?;
        let mut results = Vec::new();
        
        // Parse organic results
        if let Some(organic) = json.get("organic").and_then(|o| o.as_array()) {
            for r in organic {
                if let Some(title) = r.get("title").and_then(|t| t.as_str()) {
                    let url = r.get("link").and_then(|l| l.as_str()).unwrap_or("");
                    let snippet = r.get("snippet").and_then(|s| s.as_str()).unwrap_or("");
                    
                    results.push(WebResult {
                        title: title.to_string(),
                        url: url.to_string(),
                        snippet: snippet.to_string(),
                    });
                }
            }
        }
        
        Ok(results)
    }
    
    /// Brave Search API
    async fn search_brave(&self, query: &str, max_results: usize, api_key: &str) -> anyhow::Result<Vec<WebResult>> {
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}&offset=0",
            urlencoding::encode(query),
            max_results
        );
        
        let response = self.client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", api_key)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Brave API error {}: {}", status, text));
        }
        
        let json: Value = response.json().await?;
        let mut results = Vec::new();
        
        // Parse web results
        if let Some(web_results) = json.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
            for r in web_results {
                if let Some(title) = r.get("title").and_then(|t| t.as_str()) {
                    let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    let description = r.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    
                    results.push(WebResult {
                        title: title.to_string(),
                        url: url.to_string(),
                        snippet: description.to_string(),
                    });
                }
            }
        }
        
        Ok(results)
    }
}

#[derive(Debug, Clone)]
struct WebResult {
    title: String,
    url: String,
    snippet: String,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the internet using AI-optimized search APIs (Tavily, Serper, or Brave). \
         Requires API key in config.toml [web_search] section."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of results (1-10, default: 5)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query parameter"))?;
        
        let count = args["count"].as_i64().unwrap_or(5) as usize;
        let count = count.min(10).max(1);
        
        // Check if configured
        if self.config.is_none() {
            return Ok(ToolResult::error(
                "Web search is not configured.\n\n\
                Add to ~/.horcrux/config.toml:\n\n\
                [web_search]\n\
                provider = \"tavily\"\n\
                api_key = \"your-api-key\"\n\n\
                Get free key at https://tavily.com (1000 searches/month)".to_string()
            ));
        }
        
        match self.search(query, count).await {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(ToolResult::success(
                        format!("No results found for '{}'.", query)
                    ));
                }
                
                let provider = self.provider_name();
                let mut output = format!("Found {} results for '{}' (via {}):\n\n", 
                    results.len(), query, provider);
                
                for (i, result) in results.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {}\n   URL: {}\n   {}\n\n",
                        i + 1,
                        result.title,
                        result.url,
                        if result.snippet.len() > 200 {
                            format!("{}...", &result.snippet[..200])
                        } else {
                            result.snippet.clone()
                        }
                    ));
                }
                
                Ok(ToolResult::success(output))
            }
            Err(e) => {
                Ok(ToolResult::error(format!(
                    "Web search failed: {}", e
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_creation() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
    }
}
