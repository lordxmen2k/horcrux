//! Agent System - ReAct-based AI Agent
//!
//! This module provides a fully-featured agent that can:
//! - Reason about tasks using an LLM
//! - Call tools to interact with the system
//! - Maintain conversation history
//! - Search knowledge bases
//! - Execute shell commands
//! - Read/write files
//! - Make HTTP requests
//! - Create custom skills dynamically
//! - Compact conversations to manage token usage

pub mod compaction;
pub mod config_cli;
pub mod llm;
pub mod memory;
pub mod personality;
pub mod react;
pub mod subagent;

pub use compaction::{CompactionConfig, CompactionManager};
pub use config_cli::{ConfigWizard, show_current_config};
pub use llm::{ChatMessage, LlmClient, LlmConfig, LlmResponse, ToolDefinition, ToolCall};
pub use memory::ConversationMemory;
pub use react::ReActAgent;
pub use subagent::{SubagentExecutor, SubagentTask, SubagentResult, DelegateTaskTool, DelegateParallelTool};
use crate::tools::ToolRegistry;

// ToolRegistry re-exported below
use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub db_path: PathBuf,
    pub session_id: String,
    pub llm_config: LlmConfig,
}

impl AgentConfig {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            session_id: generate_session_id(),
            llm_config: LlmConfig::from_env(),
        }
    }

    pub fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_llm_config(mut self, llm_config: LlmConfig) -> Self {
        self.llm_config = llm_config;
        self
    }
}

/// Main Agent handle
pub struct Agent {
    react_agent: ReActAgent,
    config: AgentConfig,
}

impl Agent {
    /// Create a new agent with default tools
    pub fn new(config: AgentConfig) -> Result<Self> {
        let llm = LlmClient::new(config.llm_config.clone());
        
        if !llm.is_available() {
            anyhow::bail!(
                "LLM not configured. Set one of:\n\
                - HORCRUX_LLM_URL + HORCRUX_LLM_MODEL (for Ollama)\n\
                - OPENAI_API_KEY (for OpenAI)\n\
                Example:\n\
                export HORCRUX_LLM_URL=http://localhost:11434/v1\n\
                export HORCRUX_LLM_MODEL=qwen2.5:7b"
            );
        }

        let tools = ToolRegistry::default_with_db(config.db_path.clone());
        let memory = ConversationMemory::new(config.db_path.clone(), config.session_id.clone());
        let react_agent = ReActAgent::new(llm, tools, memory);

        info!("Agent initialized with session ID: {}", config.session_id);

        Ok(Self {
            react_agent,
            config,
        })
    }

    /// Run the agent with a single user input
    pub async fn run(&mut self, input: &str) -> Result<String> {
        self.react_agent.run(input).await
    }

    /// Run in interactive mode (with status updates)
    pub async fn run_interactive(&mut self, input: &str) -> Result<String> {
        self.react_agent.run_interactive(input).await
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Clear conversation history
    pub async fn clear_history(&mut self) -> Result<()> {
        self.react_agent.clear_history().await
    }
}

/// Generate a unique session ID
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random: u32 = rand::random();
    format!("{}-{}", timestamp, random)
}

// Re-export commonly used types
pub use crate::tools::{Tool, ToolResult};
