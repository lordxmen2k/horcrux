//! Shell Command Tool - Execute system commands
//!
//! With safety controls and permission system

use super::{Tool, ToolResult};
use anyhow::Context;
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

/// Shell command tool with permission controls
pub struct ShellTool {
    /// Whether to require explicit confirmation for commands
    require_confirmation: bool,
    /// List of allowed commands (empty = allow all with confirmation)
    allowlist: Vec<String>,
    /// List of blocked commands/patterns
    blocklist: Vec<String>,
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            require_confirmation: true,
            allowlist: Vec::new(),
            blocklist: vec![
                "rm -rf /".to_string(),
                ":(){ :|:& };:".to_string(), // Fork bomb
                "mkfs".to_string(),
                "dd if=/dev/zero".to_string(),
            ],
        }
    }
    
    /// Check if command is in blocklist
    fn is_blocked(&self, command: &str) -> Option<&str> {
        for blocked in &self.blocklist {
            if command.contains(blocked) {
                return Some(blocked);
            }
        }
        None
    }
    
    /// Check if command requires confirmation
    fn requires_confirmation(&self, command: &str) -> bool {
        if !self.require_confirmation {
            return false;
        }
        
        // Check if in allowlist
        for allowed in &self.allowlist {
            if command.starts_with(allowed) {
                return false;
            }
        }
        
        true
    }
    
    /// Execute a shell command
    async fn execute_command(&self, command: &str, cwd: Option<&str>, timeout_secs: u64) -> anyhow::Result<CommandResult> {
        info!("Executing shell command: {}", command);
        
        // Parse command (handle shell syntax)
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let shell_arg = if cfg!(windows) { "/C" } else { "-c" };
        
        let mut cmd = Command::new(shell);
        cmd.arg(shell_arg)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Set working directory if specified
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        
        // Execute with timeout
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            cmd.output()
        ).await
            .map_err(|_| anyhow::anyhow!("Command timed out after {} seconds", timeout_secs))?
            .context("Failed to execute command")?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();
        let exit_code = output.status.code().unwrap_or(-1);
        
        Ok(CommandResult {
            success,
            exit_code,
            stdout,
            stderr,
        })
    }
}

#[derive(Debug)]
struct CommandResult {
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    
    fn description(&self) -> &str {
        "Execute system shell commands. \
         Use this to interact with the system, check status, install packages, etc. \
         Common uses: checking disk space, listing files, monitoring processes, system updates. \
         ⚠️ Potentially dangerous commands require user confirmation."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for the command (optional)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60)",
                    "default": 60,
                    "minimum": 1,
                    "maximum": 300
                },
                "confirmation": {
                    "type": "string",
                    "description": "User confirmation for dangerous commands (required for certain commands)"
                }
            },
            "required": ["command"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let command = args["command"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;
        let cwd = args["cwd"].as_str();
        let timeout = args["timeout"].as_u64().unwrap_or(60);
        let confirmation = args["confirmation"].as_str();
        
        // Check for blocked commands
        if let Some(blocked) = self.is_blocked(command) {
            return Ok(ToolResult::error(format!(
                "Command blocked for safety: contains '{}'. \
                This command could damage your system.",
                blocked
            )));
        }
        
        // Check if confirmation is required
        if self.requires_confirmation(command) {
            if confirmation.is_none() {
                return Ok(ToolResult::error(format!(
                    "⚠️ This command requires user confirmation:\n\n\
                    ```\n{}\n```\n\n\
                    To execute, provide confirmation parameter:\n\
                    ```\n\"confirmation\": \"I understand the risks and want to execute this command\"\n```",
                    command
                )));
            }
        }
        
        // Execute the command
        match self.execute_command(command, cwd, timeout).await {
            Ok(result) => {
                let mut output = String::new();
                
                if !result.success {
                    output.push_str(&format!("⚠️ Command exited with code {}\n\n", result.exit_code));
                }
                
                if !result.stdout.is_empty() {
                    output.push_str("STDOUT:\n");
                    output.push_str(&result.stdout);
                    output.push('\n');
                }
                
                if !result.stderr.is_empty() {
                    output.push_str("STDERR:\n");
                    output.push_str(&result.stderr);
                    output.push('\n');
                }
                
                if result.stdout.is_empty() && result.stderr.is_empty() {
                    output.push_str("(Command completed with no output)\n");
                }
                
                if result.success {
                    Ok(ToolResult::success(output))
                } else {
                    Ok(ToolResult::error(output))
                }
            }
            Err(e) => {
                Ok(ToolResult::error(format!(
                    "Failed to execute command: {}\n\n\
                    Make sure the command exists and you have permission to run it.",
                    e
                )))
            }
        }
    }
}

/// Tool for self-diagnosis and repair
pub struct SelfHealTool;

impl SelfHealTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SelfHealTool {
    fn name(&self) -> &str {
        "self_heal"
    }
    
    fn description(&self) -> &str {
        "Attempt to diagnose and fix common configuration issues automatically. \
         Checks API keys, config files, directories, and permissions. \
         Use this when the agent isn't working properly."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "component": {
                    "type": "string",
                    "description": "Specific component to heal (config, web_search, all). Default: all",
                    "enum": ["config", "web_search", "llm", "all"],
                    "default": "all"
                }
            }
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let component = args["component"].as_str().unwrap_or("all");
        
        let mut fixes = Vec::new();
        let mut errors = Vec::new();
        
        // Heal config directory
        let config_dir = crate::config::Config::config_dir();
        if !config_dir.exists() {
            match std::fs::create_dir_all(&config_dir) {
                Ok(_) => fixes.push(format!("✓ Created config directory: {:?}", config_dir)),
                Err(e) => errors.push(format!("✗ Failed to create config dir: {}", e)),
            }
        }
        
        // Heal config file
        if component == "all" || component == "config" {
            let config_path = crate::config::Config::config_path();
            if !config_path.exists() {
                let default_config = crate::config::Config::default();
                match default_config.save() {
                    Ok(_) => fixes.push(format!("✓ Created default config at {:?}", config_path)),
                    Err(e) => errors.push(format!("✗ Failed to create config: {}", e)),
                }
            }
        }
        
        // Check web search config
        if component == "all" || component == "web_search" {
            match crate::config::Config::load() {
                Ok(config) => {
                    if !config.web_search.is_configured() {
                        // Check env vars
                        if std::env::var("TAVILY_API_KEY").is_ok() {
                            fixes.push("✓ Found TAVILY_API_KEY in environment".to_string());
                        } else {
                            errors.push(
                                "⚠ Web search not configured. Add to ~/.horcrux/config.toml:\n\
                                [web_search]\n\
                                provider = \"tavily\"\n\
                                api_key = \"your-key\"".to_string()
                            );
                        }
                    }
                }
                Err(e) => errors.push(format!("✗ Cannot load config: {}", e)),
            }
        }
        
        // Build report
        let mut output = String::from("🔧 Self-Heal Report\n");
        output.push_str("===================\n\n");
        
        if !fixes.is_empty() {
            output.push_str("Fixes Applied:\n");
            for fix in &fixes {
                output.push_str(&format!("  {}\n", fix));
            }
            output.push('\n');
        }
        
        if !errors.is_empty() {
            output.push_str("Issues Requiring Manual Fix:\n");
            for error in &errors {
                output.push_str(&format!("  {}\n", error));
            }
        }
        
        if fixes.is_empty() && errors.is_empty() {
            output.push_str("✅ All components healthy!\n");
        }
        
        Ok(ToolResult::success(output))
    }
}
