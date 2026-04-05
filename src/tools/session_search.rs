//! Session Search Tool - FTS5 search across conversation history

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::Db;

use super::{Tool, ToolResult};

/// Search conversation history using FTS5
pub struct SessionSearchTool {
    db_path: PathBuf,
}

impl SessionSearchTool {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

#[async_trait]
impl Tool for SessionSearchTool {
    fn name(&self) -> &str {
        "session_search"
    }

    fn description(&self) -> &str {
        "Search conversation history across all sessions using full-text search (FTS5). \
         Use this to recall information from previous conversations, find context about \
         past tasks, or retrieve forgotten details. Searches through user messages, \
         assistant responses, and tool outputs."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query using FTS5 syntax. Supports AND/OR operators, phrases in quotes, and prefix matching with *."
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional: restrict search to a specific session ID"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10, max: 50)",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 50
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required 'query' parameter"))?;
        
        let session_id = args["session_id"].as_str();
        let limit = args["limit"].as_u64().unwrap_or(10).min(50) as usize;

        // Open database connection
        let db = match Db::open(&self.db_path) {
            Ok(db) => Arc::new(db),
            Err(e) => return Ok(ToolResult::error(format!("Failed to open database: {}", e))),
        };

        // Perform search
        let results = match db.search_conversations(query, session_id, limit) {
            Ok(results) => results,
            Err(e) => return Ok(ToolResult::error(format!("Search failed: {}", e))),
        };

        if results.is_empty() {
            return Ok(ToolResult::success("No matching conversations found."));
        }

        // Format results
        let mut output = format!("Found {} matching conversation(s):\n\n", results.len());
        
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "[{}] Session: {} | Role: {} | Time: {} | Rank: {:.4}\n",
                i + 1,
                &result.session_id[..result.session_id.len().min(16)],
                result.role,
                result.created_at,
                result.rank
            ));
            
            // Truncate content if too long
            let content = if result.content.len() > 500 {
                format!("{}...", &result.content[..500])
            } else {
                result.content.clone()
            };
            output.push_str(&format!("Content: {}\n\n", content));
        }

        Ok(ToolResult::success(output))
    }
}
