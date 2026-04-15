//! Tool Registry and Tool Definitions for the Agent

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod filesystem;
pub mod config_manager;
pub mod http;
pub mod image_search;
pub mod mcp;
pub mod search;
pub mod session_search;
pub mod shell;
pub mod skills;
pub mod voice;
pub mod web_search;
pub mod skills_library;
pub mod telegram;
pub mod dependency_manager;
pub mod code_executor;
pub mod vision;
pub mod file_search;

pub use filesystem::FileSystemTool;
pub use config_manager::ConfigManagerTool;
pub use http::HttpTool;
pub use image_search::ImageSearchTool;
pub use mcp::{McpToolWrapper, McpToolManager};
pub use search::SearchTool;
pub use session_search::SessionSearchTool;
pub use shell::{ShellTool, SelfHealTool};
pub use voice::VoiceTranscriptionTool;
pub use web_search::WebSearchTool;
pub use skills::{CreateSkillTool, ListSkillsTool, Skill, SkillImplementation, SkillManager, SkillTool};
pub use skills_library::{find_similar_skill, get_builtin_skills};
pub use telegram::{TelegramTool, TelegramAgentBot};
pub use dependency_manager::DependencyManagerTool;
pub use code_executor::CodeExecutorTool;
pub use vision::VisionTool;
pub use file_search::FileSearchTool;

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
        Self::default_with_db_and_mcp(db_path, None)
    }

    /// Create a registry with built-in tools and optional MCP client
    pub fn default_with_db_and_mcp(db_path: std::path::PathBuf, mcp_client: Option<Arc<crate::mcp::McpClient>>) -> Self {
        let mut registry = Self::new();
        
        // Register search tool
        registry.register(Arc::new(SearchTool::new(db_path.clone())));
        
        // Register session search tool (FTS5 conversation search)
        registry.register(Arc::new(SessionSearchTool::new(db_path.clone())));
        
        // Register filesystem tool
        registry.register(Arc::new(FileSystemTool::new()));
        
        // Register web search tool for current information
        registry.register(Arc::new(WebSearchTool::new()));
        
        // Register shell tool for system commands (with permission controls)
        registry.register(Arc::new(ShellTool::new()));
        
        // Register self-heal tool for diagnostics and repair
        registry.register(Arc::new(SelfHealTool::new()));
        
        // Register system health monitoring tool
        registry.register(Arc::new(crate::doctor::tool::SystemHealthTool::new()));
        
        // Register config manager (for setup/config, not general use)
        registry.register(Arc::new(ConfigManagerTool::new()));
        // Register image search tool (may fail if config can't be loaded, which is ok)
        if let Ok(img_tool) = ImageSearchTool::new() {
            registry.register(Arc::new(img_tool));
        }
        
        // Register Telegram tool
        registry.register(Arc::new(TelegramTool::new()));
        
        // Register voice transcription tool
        registry.register(Arc::new(VoiceTranscriptionTool::new()));
        
        // Register dependency manager tool for self-installing languages
        registry.register(Arc::new(DependencyManagerTool::new()));
        
        // Register code executor for local code execution
        registry.register(Arc::new(CodeExecutorTool::new()));
        
        // Register vision tool for image analysis
        registry.register(Arc::new(VisionTool::new()));
        
        // Register file search tool for searching inside documents
        registry.register(Arc::new(FileSearchTool::new()));
        
        // Register Skills tools (create_skill, list_skills, etc.)
        // Try project directory first, then fall back to data dir
        let skills_dir = if std::path::Path::new("skills").exists() {
            std::path::PathBuf::from("skills")
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap())
                .join("horcrux/skills")
        };
        registry.register(Arc::new(CreateSkillTool::new(skills_dir)));
        // ListSkillsTool removed - skills already injected into system prompt
        
        // Register MCP tools if client is provided
        if let Some(client) = mcp_client {
            let manager = McpToolManager::new(client);
            for tool in manager.get_tools() {
                registry.register(Arc::new(tool));
            }
        }
        
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
