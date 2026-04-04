//! MCP Server implementation for hoard

use super::protocol::*;
use crate::db::Db;
use crate::embed::{EmbedClient, EmbedConfig};
use crate::search::run_search;

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use tracing::{debug, error, info, warn};

pub struct McpServer {
    db_path: PathBuf,
    state: ServerState,
    embed_client: Option<EmbedClient>,
    embed_model: String,
}

#[derive(Clone)]
struct ServerState {
    initialized: bool,
}

impl McpServer {
    pub fn new(db_path: PathBuf) -> Self {
        let config = EmbedConfig::from_env();
        let embed_client = if std::env::var("HORCRUX_EMBED_URL").is_ok()
            || std::env::var("HOARD_EMBED_URL").is_ok() // backward compat
            || std::env::var("OLLAMA_HOST").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
        {
            Some(EmbedClient::new(config.clone()))
        } else {
            None
        };

        Self {
            db_path,
            state: ServerState { initialized: false },
            embed_client,
            embed_model: config.model,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("🎛️  hoard MCP server starting...");
        
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = stdin.lock();

        for line in reader.lines() {
            let line = line?;
            debug!("Received: {}", line);

            match self.handle_message(&line).await {
                Ok(Some(response)) => {
                    let response_json = serde_json::to_string(&response)?;
                    debug!("Sending: {}", response_json);
                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                }
                Ok(None) => {
                    // Notification, no response needed
                }
                Err(e) => {
                    error!("Error handling message: {}", e);
                    let error_response = JsonRpcResponse::error(
                        RequestId::Null,
                        INTERNAL_ERROR,
                        format!("Internal error: {}", e),
                    );
                    let response_json = serde_json::to_string(&error_response)?;
                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, line: &str) -> Result<Option<JsonRpcResponse>> {
        // Try to parse as request first
        if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(line) {
            return self.handle_request(request).await;
        }

        // Try notification (no id)
        if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(line) {
            self.handle_notification(notification).await?;
            return Ok(None);
        }

        // Invalid JSON-RPC
        let response = JsonRpcResponse::error(
            RequestId::Null,
            PARSE_ERROR,
            "Invalid JSON-RPC message".into(),
        );
        Ok(Some(response))
    }

    async fn handle_request(&mut self, request: JsonRpcRequest) -> Result<Option<JsonRpcResponse>> {
        debug!("Handling method: {}", request.method);

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await?,
            "tools/list" => self.handle_tools_list().await?,
            "tools/call" => self.handle_tool_call(request.params).await?,
            "resources/list" => self.handle_resources_list().await?,
            "resources/read" => self.handle_resource_read(request.params).await?,
            "prompts/list" => self.handle_prompts_list().await?,
            "prompts/get" => self.handle_prompt_get(request.params).await?,
            _ => {
                return Ok(Some(JsonRpcResponse::error(
                    request.id,
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                )));
            }
        };

        Ok(Some(JsonRpcResponse::success(request.id, result)))
    }

    async fn handle_notification(&mut self, notification: JsonRpcNotification) -> Result<()> {
        match notification.method.as_str() {
            "initialized" => {
                info!("Client initialized");
                self.state.initialized = true;
            }
            "notifications/cancelled" => {
                debug!("Request cancelled");
            }
            _ => {
                warn!("Unknown notification: {}", notification.method);
            }
        }
        Ok(())
    }

    async fn handle_initialize(&self, _params: Option<Value>) -> Result<Value> {
        info!("Initialize request received");

        let result = InitializeResult {
            protocolVersion: PROTOCOL_VERSION.into(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { listChanged: false }),
                resources: Some(ResourcesCapability {
                    subscribe: false,
                    listChanged: false,
                }),
                prompts: Some(PromptsCapability { listChanged: false }),
                ..Default::default()
            },
            serverInfo: Implementation {
                name: "horcrux".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        };

        Ok(serde_json::to_value(result)?)
    }

    async fn handle_tools_list(&self) -> Result<Value> {
        let tools = vec![
            Tool {
                name: "search".into(),
                description: "Search your knowledge base for relevant documents. \
                    Use this when you need to find information from the user's notes, \
                    documents, or previously indexed content.".into(),
                inputSchema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results (default: 5)",
                            "default": 5
                        },
                        "collection": {
                            "type": "string",
                            "description": "Optional collection to search within",
                            "optional": true
                        }
                    },
                    "required": ["query"]
                })
                .as_object()
                .unwrap()
                .clone(),
            },
            Tool {
                name: "get_document".into(),
                description: "Retrieve a full document by its ID or path. \
                    Use this when you need to read the complete content of a specific document.".into(),
                inputSchema: json!({
                    "type": "object",
                    "properties": {
                        "docid": {
                            "type": "string",
                            "description": "Document ID (e.g., #a1b2c3) or path"
                        }
                    },
                    "required": ["docid"]
                })
                .as_object()
                .unwrap()
                .clone(),
            },
            Tool {
                name: "status".into(),
                description: "Get the status of the hoard index including \
                    document counts, collections, and embedding status.".into(),
                inputSchema: json!({
                    "type": "object",
                    "properties": {}
                })
                .as_object()
                .unwrap()
                .clone(),
            },
            Tool {
                name: "list_collections".into(),
                description: "List all collections in the hoard.".into(),
                inputSchema: json!({
                    "type": "object",
                    "properties": {}
                })
                .as_object()
                .unwrap()
                .clone(),
            },
        ];

        Ok(json!({ "tools": tools }))
    }

    async fn handle_tool_call(&self, params: Option<Value>) -> Result<Value> {
        let params: CallToolParams = serde_json::from_value(
            params.ok_or_else(|| anyhow::anyhow!("Missing params"))?
        )?;

        let result = match params.name.as_str() {
            "search" => self.tool_search(params.arguments).await?,
            "get_document" => self.tool_get_document(params.arguments).await?,
            "status" => self.tool_status().await?,
            "list_collections" => self.tool_list_collections().await?,
            _ => {
                return Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Unknown tool: {}", params.name)
                    }],
                    "isError": true
                }));
            }
        };

        Ok(result)
    }

    async fn tool_search(&self, args: Option<Value>) -> Result<Value> {
        let args = args.ok_or_else(|| anyhow::anyhow!("Missing arguments"))?;
        let query = args["query"].as_str().ok_or_else(|| anyhow::anyhow!("Missing query"))?;
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;
        let collection = args["collection"].as_str().map(|s| s.to_string());

        let db = Db::open(&self.db_path)?;
        
        let results = run_search(
            &db,
            query,
            "query", // hybrid mode
            limit,
            0.0,
            collection.as_deref(),
            self.embed_client.as_ref(),
            &self.embed_model,
        )?;

        if results.is_empty() {
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("No results found for query: '{}'", query)
                }]
            }));
        }

        let mut text = format!("Found {} results for '{}':\n\n", results.len(), query);
        
        for (i, result) in results.iter().enumerate() {
            text.push_str(&format!(
                "{}. {} (score: {:.2})\n   Path: {}\n   Snippet: {}\n\n",
                i + 1,
                result.title,
                result.score,
                result.path,
                if result.snippet.len() > 200 {
                    format!("{}...", &result.snippet[..200])
                } else {
                    result.snippet.clone()
                }
            ));
        }

        Ok(json!({
            "content": [{"type": "text", "text": text}]
        }))
    }

    async fn tool_get_document(&self, args: Option<Value>) -> Result<Value> {
        let args = args.ok_or_else(|| anyhow::anyhow!("Missing arguments"))?;
        let docid = args["docid"].as_str().ok_or_else(|| anyhow::anyhow!("Missing docid"))?;

        let db = Db::open(&self.db_path)?;
        
        let doc = if docid.starts_with('#') {
            let id = docid.trim_start_matches('#');
            db.get_document(id)?
        } else {
            db.find_document_by_path(docid)?
        };

        match doc {
            Some(d) => {
                let text = format!(
                    "# {}\n\nPath: {}\nCollection: {}\n\n{}",
                    d.title, d.path, d.collection, d.body
                );
                Ok(json!({
                    "content": [{"type": "text", "text": text}]
                }))
            }
            None => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Document not found: {}", docid)
                }],
                "isError": true
            })),
        }
    }

    async fn tool_status(&self) -> Result<Value> {
        let db = Db::open(&self.db_path)?;
        
        let collections = db.list_collections()?;
        let doc_count = db.document_count()?;
        let chunk_count = db.chunk_count()?;
        let embedded_count = db.embedded_chunk_count()?;

        let embed_status = if self.embed_client.is_some() {
            "available"
        } else {
            "not configured (set HOARD_EMBED_URL for semantic search)"
        };

        let text = format!(
            "📊 hoard Status\n\n\
            Collections: {}\n\
            Documents: {}\n\
            Chunks: {}\n\
            Embedded: {}\n\
            Semantic Search: {}\n\n\
            Collections:\n{}",
            collections.len(),
            doc_count,
            chunk_count,
            embedded_count,
            embed_status,
            collections.iter()
                .map(|c| format!("  - {}: {}", c.name, c.path))
                .collect::<Vec<_>>()
                .join("\n")
        );

        Ok(json!({
            "content": [{"type": "text", "text": text}]
        }))
    }

    async fn tool_list_collections(&self) -> Result<Value> {
        let db = Db::open(&self.db_path)?;
        let collections = db.list_collections()?;

        if collections.is_empty() {
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": "No collections found. Add one with: hoard collection add <path>"
                }]
            }));
        }

        let mut text = "Collections:\n\n".to_string();
        for c in collections {
            text.push_str(&format!("📁 {}\n   Path: {}\n   Pattern: {}\n\n", c.name, c.path, c.pattern));
        }

        Ok(json!({
            "content": [{"type": "text", "text": text}]
        }))
    }

    async fn handle_resources_list(&self) -> Result<Value> {
        // Resources could be individual documents
        // For now, return empty list
        Ok(json!({ "resources": [] }))
    }

    async fn handle_resource_read(&self, _params: Option<Value>) -> Result<Value> {
        Ok(json!({ "contents": [] }))
    }

    async fn handle_prompts_list(&self) -> Result<Value> {
        let prompts = vec![Prompt {
            name: "search_hoard".into(),
            description: Some("Search the user's knowledge hoard for context".into()),
            arguments: Some(vec![
                PromptArgument {
                    name: "query".into(),
                    description: Some("What to search for".into()),
                    required: true,
                }
            ]),
        }];

        Ok(json!({ "prompts": prompts }))
    }

    async fn handle_prompt_get(&self, params: Option<Value>) -> Result<Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params["name"].as_str().ok_or_else(|| anyhow::anyhow!("Missing name"))?;

        if name == "search_hoard" {
            let query = params["arguments"]["query"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing query argument"))?;

            Ok(json!({
                "description": "Search results from hoard",
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!("Search my hoard for: {}", query)
                        }
                    }
                ]
            }))
        } else {
            Err(anyhow::anyhow!("Prompt not found: {}", name))
        }
    }
}
