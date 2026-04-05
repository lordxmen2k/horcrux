//! MCP Client for connecting to external MCP servers
//!
//! This module provides a client that can connect to MCP servers via stdio,
//! list their available tools, and execute tool calls through the MCP protocol.

use super::protocol::*;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Configuration for an MCP server connection
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub allowed_tools: Option<Vec<String>>, // If None, allow all tools
}

/// An MCP tool from an external server
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String,
}

/// Active connection to an MCP server
pub struct McpServerConnection {
    config: McpServerConfig,
    child: Child,
    stdin: Arc<Mutex<BufWriter<tokio::process::ChildStdin>>>,
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    tools: Vec<McpTool>,
    request_id: Arc<Mutex<i64>>,
}

impl McpServerConnection {
    /// Start a new MCP server connection
    pub async fn connect(config: McpServerConfig) -> Result<Self> {
        info!("Connecting to MCP server: {}", config.name);
        
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let stderr go to parent's stderr

        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to spawn MCP server: {}", config.command))?;

        let stdin = BufWriter::new(child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?);
        let stdout = BufReader::new(child.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?);

        let mut conn = Self {
            config: config.clone(),
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
            tools: Vec::new(),
            request_id: Arc::new(Mutex::new(1)),
        };

        // Initialize the session
        conn.initialize().await?;

        // List available tools
        conn.tools = conn.list_tools().await?;
        
        // Filter tools if allowed_tools is specified
        if let Some(allowed) = &config.allowed_tools {
            conn.tools.retain(|t| allowed.contains(&t.name));
            info!(
                "MCP server '{}' connected with {} tools (filtered from {})",
                config.name,
                conn.tools.len(),
                allowed.len()
            );
        } else {
            info!(
                "MCP server '{}' connected with {} tools",
                config.name,
                conn.tools.len()
            );
        }

        Ok(conn)
    }

    /// Send a request and wait for response
    async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = {
            let mut id = self.request_id.lock().await;
            let current = *id;
            *id += 1;
            current
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let request_str = serde_json::to_string(&request)?;
        debug!("MCP -> {}: {}", self.config.name, request_str);

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(request_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Read response
        let mut line = String::new();
        {
            let mut stdout = self.stdout.lock().await;
            stdout.read_line(&mut line).await?;
        }

        debug!("MCP <- {}: {}", self.config.name, line.trim());

        let response: Value = serde_json::from_str(&line)
            .with_context(|| format!("Invalid JSON response: {}", line))?;

        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("MCP error {}: {}", code, message));
        }

        response.get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing result in response"))
    }

    /// Initialize the MCP session
    async fn initialize(&self) -> Result<()> {
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "experimental": null,
                "sampling": null
            },
            "clientInfo": {
                "name": "horcrux",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        self.send_request("initialize", Some(params)).await?;
        
        // Send initialized notification
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        
        let notification_str = serde_json::to_string(&notification)?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(notification_str.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        
        Ok(())
    }

    /// List available tools from this server
    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let result = self.send_request("tools/list", None).await?;
        
        let tools_array = result.get("tools")
            .and_then(|t| t.as_array())
            .unwrap_or(&Vec::new())
            .clone();

        let mut tools = Vec::new();
        for tool_value in tools_array {
            let name = tool_value.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            
            let description = tool_value.get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            
            let input_schema = tool_value.get("inputSchema")
                .cloned()
                .unwrap_or(json!({"type": "object"}));

            tools.push(McpTool {
                name,
                description,
                input_schema,
                server_name: self.config.name.clone(),
            });
        }

        Ok(tools)
    }

    /// Call a tool on this server
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<ToolResult> {
        let params = json!({
            "name": tool_name,
            "arguments": arguments
        });

        let result = self.send_request("tools/call", Some(params)).await?;

        // Parse the result
        let content = result.get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let is_error = result.get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);

        // Concatenate all text content
        let mut text_parts = Vec::new();
        for item in content {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                text_parts.push(text.to_string());
            } else if let Some(data) = item.get("data").and_then(|d| d.as_str()) {
                // For image or blob data
                text_parts.push(format!("[data: {}]", data.chars().take(100).collect::<String>()));
            }
        }

        Ok(ToolResult {
            content: text_parts.join("\n"),
            is_error,
        })
    }

    pub fn get_tools(&self) -> &[McpTool] {
        &self.tools
    }

    pub fn get_name(&self) -> &str {
        &self.config.name
    }
}

/// Result from calling an MCP tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

/// MCP Client managing multiple server connections
pub struct McpClient {
    servers: HashMap<String, Arc<McpServerConnection>>,
    tool_to_server: HashMap<String, String>, // tool_name -> server_name
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tool_to_server: HashMap::new(),
        }
    }

    /// Connect to an MCP server
    pub async fn connect_server(&mut self, config: McpServerConfig) -> Result<()> {
        let server = McpServerConnection::connect(config).await?;
        let server_name = server.get_name().to_string();
        let server_arc = Arc::new(server);

        // Map tools to this server
        for tool in server_arc.get_tools() {
            if self.tool_to_server.contains_key(&tool.name) {
                warn!(
                    "Tool '{}' already defined by another server, skipping",
                    tool.name
                );
                continue;
            }
            self.tool_to_server.insert(tool.name.clone(), server_name.clone());
        }

        self.servers.insert(server_name, server_arc);
        Ok(())
    }

    /// Get all available tools from all connected servers
    pub fn get_all_tools(&self) -> Vec<&McpTool> {
        self.servers
            .values()
            .flat_map(|s| s.get_tools())
            .collect()
    }

    /// Call a tool by name
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<ToolResult> {
        let server_name = self.tool_to_server
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_name))?;

        let server = self.servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;

        server.call_tool(tool_name, arguments).await
    }

    /// Check if a tool is available
    pub fn has_tool(&self, tool_name: &str) -> bool {
        self.tool_to_server.contains_key(tool_name)
    }

    /// Get number of connected servers
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Disconnect all servers
    pub async fn disconnect_all(&mut self) {
        for (name, server) in &self.servers {
            info!("Disconnecting MCP server: {}", name);
            // The server process will be killed when dropped
        }
        self.servers.clear();
        self.tool_to_server.clear();
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Load MCP server configurations from config file
pub fn load_mcp_configs() -> Vec<McpServerConfig> {
    let mut configs = Vec::new();

    // Try to load from config file
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("horcrux").join("mcp.toml");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(toml_value) = content.parse::<toml::Value>() {
                    if let Some(servers) = toml_value.get("servers").and_then(|s| s.as_table()) {
                        for (name, server_config) in servers {
                            if let Some(command) = server_config.get("command").and_then(|c| c.as_str()) {
                                let args = server_config.get("args")
                                    .and_then(|a| a.as_array())
                                    .map(|arr| arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect())
                                    .unwrap_or_default();

                                let env = server_config.get("env")
                                    .and_then(|e| e.as_table())
                                    .map(|table| table.iter()
                                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                        .collect())
                                    .unwrap_or_default();

                                let allowed_tools = server_config.get("allowed_tools")
                                    .and_then(|a| a.as_array())
                                    .map(|arr| arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect());

                                configs.push(McpServerConfig {
                                    name: name.clone(),
                                    command: command.to_string(),
                                    args,
                                    env,
                                    allowed_tools,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    configs
}
