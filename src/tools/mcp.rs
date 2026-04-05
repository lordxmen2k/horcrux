//! MCP Tool Integration
//!
//! This module provides integration between external MCP servers and the agent's tool system.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::mcp::McpClient;

use super::{Tool, ToolResult};

/// A tool that wraps an MCP tool from an external server
pub struct McpToolWrapper {
    name: String,
    description: String,
    parameters_schema: Value,
    client: Arc<McpClient>,
}

impl McpToolWrapper {
    pub fn new(name: String, description: String, parameters_schema: Value, client: Arc<McpClient>) -> Self {
        Self {
            name,
            description,
            parameters_schema,
            client,
        }
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        match self.client.call_tool(&self.name, args).await {
            Ok(result) => {
                if result.is_error {
                    Ok(ToolResult::error(result.content))
                } else {
                    Ok(ToolResult::success(result.content))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("MCP tool call failed: {}", e))),
        }
    }
}

/// Manager for MCP tools that can be registered with the tool registry
pub struct McpToolManager {
    client: Arc<McpClient>,
}

impl McpToolManager {
    pub fn new(client: Arc<McpClient>) -> Self {
        Self { client }
    }

    /// Get all available MCP tools as Tool trait objects
    pub fn get_tools(&self) -> Vec<McpToolWrapper> {
        self.client
            .get_all_tools()
            .iter()
            .map(|tool| McpToolWrapper::new(
                tool.name.clone(),
                tool.description.clone(),
                tool.input_schema.clone(),
                self.client.clone(),
            ))
            .collect()
    }

    pub fn client(&self) -> Arc<McpClient> {
        self.client.clone()
    }
}
