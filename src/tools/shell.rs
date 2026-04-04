//! Shell Tool - Execute system commands

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }

    async fn execute_command(&self, command: &str, working_dir: Option<&str>, timeout_secs: u64) -> anyhow::Result<ToolResult> {
        // Detect shell based on OS
        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut cmd = Command::new(shell);
        cmd.arg(shell_arg).arg(command);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Capture both stdout and stderr
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Execute with timeout
        let result = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut result_text = String::new();
                
                if !stdout.is_empty() {
                    result_text.push_str(&format!("STDOUT:\n{}\n", stdout));
                }
                
                if !stderr.is_empty() {
                    result_text.push_str(&format!("STDERR:\n{}\n", stderr));
                }

                result_text.push_str(&format!("Exit code: {}", exit_code));

                if output.status.success() {
                    Ok(ToolResult::success(result_text))
                } else {
                    Ok(ToolResult::error(format!(
                        "Command failed with exit code {}\n{}",
                        exit_code, result_text
                    )))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("Failed to execute command: {}", e)))
            }
            Err(_) => {
                Ok(ToolResult::error(format!(
                    "Command timed out after {} seconds",
                    timeout_secs
                )))
            }
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute system commands. Use this when you need to:\n\
        - Run installed programs or scripts\n\
        - Check system info (git status, docker ps, etc.)\n\
        - Use command-line tools not available as other tools\n\
        Note: Commands run with full permissions. On Windows uses cmd, on Unix uses sh."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command (optional)",
                    "optional": true
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60)",
                    "default": 60,
                    "minimum": 1,
                    "maximum": 600,
                    "optional": true
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let command = args["command"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;
        
        let working_dir = args["working_dir"].as_str();
        let timeout = args["timeout"].as_u64().unwrap_or(60);

        if command.trim().is_empty() {
            return Ok(ToolResult::error("Empty command"));
        }

        self.execute_command(command, working_dir, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_tool_creation() {
        let tool = ShellTool::new();
        assert_eq!(tool.name(), "shell");
    }
}
