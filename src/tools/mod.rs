//! Tool Registry and Tool Definitions for the Agent

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod filesystem;
pub mod http;
pub mod search;
pub mod shell;
pub mod skills;
pub mod skills_library;
pub mod telegram;

pub use filesystem::FileSystemTool;
pub use http::HttpTool;
pub use search::SearchTool;
pub use shell::ShellTool;
pub use skills::{CreateSkillTool, ListSkillsTool, Skill, SkillImplementation, SkillManager, SkillTool};
pub use skills_library::{find_similar_skill, get_builtin_skills};
pub use telegram::{TelegramTool, TelegramAgentBot};

/// A tool that can be called by the agent
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (must be unique)
    fn name(&self) -> &str;

    /// Tool description for the LLM
    fn description(&self) -> &str;

    /// JSON schema for the tool's parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with the given arguments
    async fn execute(&self, args: Value) -> Result<ToolResult>;
}

/// Result of a tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
        }
    }

    pub fn to_string(&self) -> String {
        if self.success {
            self.output.clone()
        } else {
            format!("Error: {}", self.error.as_deref().unwrap_or("Unknown error"))
        }
    }
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<&Arc<dyn Tool>> {
        self.tools.values().collect()
    }

    pub fn list_definitions(&self) -> Vec<crate::agent::llm::ToolDefinition> {
        self.tools
            .values()
            .map(|t| crate::agent::llm::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    /// Create a default registry with all built-in tools
    pub fn default_with_db(db_path: std::path::PathBuf) -> Self {
        let mut registry = Self::new();
        
        // Register search tool
        registry.register(Arc::new(SearchTool::new(db_path.clone())));
        
        // Register filesystem tool
        registry.register(Arc::new(FileSystemTool::new()));
        
        // Register shell tool
        registry.register(Arc::new(ShellTool::new()));
        
        // Register HTTP tool
        registry.register(Arc::new(HttpTool::new()));
        
        // Register Telegram tool
        registry.register(Arc::new(TelegramTool::new()));
        
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
