//! Search Tool - Search the knowledge base

use super::{Tool, ToolResult};
use crate::db::Db;
use crate::search::run_search;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

pub struct SearchTool {
    db_path: PathBuf,
    embed_config: crate::embed::EmbedConfig,
}

impl SearchTool {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            embed_config: crate::embed::EmbedConfig::from_env(),
        }
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search_knowledge"
    }

    fn description(&self) -> &str {
        "Search your knowledge base for relevant documents, notes, and information. \
         Use this when you need to find information from the user's indexed documents. \
         Supports both keyword (BM25) and semantic search."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query - be specific and use keywords that would appear in the documents"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                },
                "collection": {
                    "type": "string",
                    "description": "Optional: specific collection to search within",
                    "optional": true
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;
        let collection = args["collection"].as_str().map(|s| s.to_string());

        // Open database
        let db = match Db::open(&self.db_path) {
            Ok(db) => db,
            Err(e) => return Ok(ToolResult::error(format!("Failed to open database: {}", e))),
        };

        // Create embed client if configured
        let embed_client = if std::env::var("HORCRUX_EMBED_URL").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("OLLAMA_HOST").is_ok()
        {
            Some(crate::embed::EmbedClient::new(self.embed_config.clone()))
        } else {
            None
        };

        // Run search
        let results = match run_search(
            &db,
            query,
            "query", // hybrid mode
            limit,
            0.0,
            collection.as_deref(),
            embed_client.as_ref(),
            &self.embed_config.model,
        ) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("Search failed: {}", e))),
        };

        if results.is_empty() {
            return Ok(ToolResult::success(format!(
                "No results found for query: '{}'\n\nTry:\n- Using different keywords\n- Checking if documents are indexed (run: horcrux update)\n- Broadening your search terms",
                query
            )));
        }

        let mut output = format!("Found {} results for '{}':\n\n", results.len(), query);
        
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {} (score: {:.2})\n   Path: {}\n   Snippet: {}\n\n",
                i + 1,
                result.title,
                result.score,
                result.path,
                if result.snippet.len() > 300 {
                    format!("{}...", &result.snippet[..300])
                } else {
                    result.snippet.clone()
                }
            ));
        }

        Ok(ToolResult::success(output))
    }
}
