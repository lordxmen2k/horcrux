//! Code Executor Tool - Execute code locally without API calls
//!
//! Allows the agent to run Python, Node.js, and other code locally
//! when APIs fail or when local execution is preferred.

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::info;

/// Code executor for running Python, Node.js, and other languages locally
pub struct CodeExecutorTool {
    default_timeout_secs: u64,
}

impl CodeExecutorTool {
    pub fn new() -> Self {
        Self {
            default_timeout_secs: 60,
        }
    }
    
    /// Check if a command is available
    async fn is_available(&self, cmd: &str) -> bool {
        let which_cmd = if cfg!(windows) { "where" } else { "which" };
        
        match Command::new(which_cmd)
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }
    
    /// Execute Python code
    async fn execute_python(&self, code: &str, timeout_secs: u64) -> anyhow::Result<ToolResult> {
        // Check for Python
        let python_cmd = if self.is_available("python3").await {
            "python3"
        } else if self.is_available("python").await {
            "python"
        } else {
            return Ok(ToolResult::error(
                "Python is not installed. Use the dependency_manager tool to install it:\n\
                 dependency_manager install python"
            ));
        };
        
        // Create a temporary file for the code
        let temp_dir = std::env::temp_dir();
        let file_name = format!("horcrux_code_{}.py", std::process::id());
        let file_path = temp_dir.join(&file_name);
        
        // Write code to temp file
        tokio::fs::write(&file_path, code).await?;
        
        // Execute the Python code
        let mut cmd = Command::new(python_cmd)
            .arg(&file_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let result = timeout(Duration::from_secs(timeout_secs), cmd.wait()).await;
        
        // Clean up temp file
        let _ = tokio::fs::remove_file(&file_path).await;
        
        match result {
            Ok(Ok(status)) => {
                let stdout = if let Some(mut output) = cmd.stdout.take() {
                    let mut buf = String::new();
                    output.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                let stderr = if let Some(mut err) = cmd.stderr.take() {
                    let mut buf = String::new();
                    err.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                if status.success() {
                    let output = if stderr.is_empty() {
                        stdout
                    } else {
                        format!("{stdout}\n\n[stderr]:\n{stderr}")
                    };
                    Ok(ToolResult::success(output))
                } else {
                    let exit_code = status.code().unwrap_or(-1);
                    Ok(ToolResult::error(format!(
                        "Python script exited with code {}\n\n[stdout]:\n{}\n\n[stderr]:\n{}",
                        exit_code, stdout, stderr
                    )))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("Failed to run Python: {}", e)))
            }
            Err(_) => {
                cmd.kill().await.ok();
                Ok(ToolResult::error(format!(
                    "Python script timed out after {} seconds", timeout_secs
                )))
            }
        }
    }
    
    /// Execute Node.js code
    async fn execute_node(&self, code: &str, timeout_secs: u64) -> anyhow::Result<ToolResult> {
        // Check for Node.js
        let node_cmd = if self.is_available("node").await {
            "node"
        } else {
            return Ok(ToolResult::error(
                "Node.js is not installed. Use the dependency_manager tool to install it:\n\
                 dependency_manager install nodejs"
            ));
        };
        
        // Create a temporary file for the code
        let temp_dir = std::env::temp_dir();
        let file_name = format!("horcrux_code_{}.js", std::process::id());
        let file_path = temp_dir.join(&file_name);
        
        // Write code to temp file
        tokio::fs::write(&file_path, code).await?;
        
        // Execute the Node.js code
        let mut cmd = Command::new(node_cmd)
            .arg(&file_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let result = timeout(Duration::from_secs(timeout_secs), cmd.wait()).await;
        
        // Clean up temp file
        let _ = tokio::fs::remove_file(&file_path).await;
        
        match result {
            Ok(Ok(status)) => {
                let stdout = if let Some(mut output) = cmd.stdout.take() {
                    let mut buf = String::new();
                    output.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                let stderr = if let Some(mut err) = cmd.stderr.take() {
                    let mut buf = String::new();
                    err.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                if status.success() {
                    let output = if stderr.is_empty() {
                        stdout
                    } else {
                        format!("{stdout}\n\n[stderr]:\n{stderr}")
                    };
                    Ok(ToolResult::success(output))
                } else {
                    let exit_code = status.code().unwrap_or(-1);
                    Ok(ToolResult::error(format!(
                        "Node.js script exited with code {}\n\n[stdout]:\n{}\n\n[stderr]:\n{}",
                        exit_code, stdout, stderr
                    )))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("Failed to run Node.js: {}", e)))
            }
            Err(_) => {
                cmd.kill().await.ok();
                Ok(ToolResult::error(format!(
                    "Node.js script timed out after {} seconds", timeout_secs
                )))
            }
        }
    }
    
    /// Execute shell script
    async fn execute_shell(&self, code: &str, timeout_secs: u64) -> anyhow::Result<ToolResult> {
        let shell = if cfg!(windows) { "cmd" } else { "bash" };
        let flag = if cfg!(windows) { "/c" } else { "-c" };
        
        // Execute the shell script
        let mut cmd = Command::new(shell)
            .arg(flag)
            .arg(code)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let result = timeout(Duration::from_secs(timeout_secs), cmd.wait()).await;
        
        match result {
            Ok(Ok(status)) => {
                let stdout = if let Some(mut output) = cmd.stdout.take() {
                    let mut buf = String::new();
                    output.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                let stderr = if let Some(mut err) = cmd.stderr.take() {
                    let mut buf = String::new();
                    err.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                if status.success() {
                    let output = if stderr.is_empty() {
                        stdout
                    } else {
                        format!("{stdout}\n\n[stderr]:\n{stderr}")
                    };
                    Ok(ToolResult::success(output))
                } else {
                    let exit_code = status.code().unwrap_or(-1);
                    Ok(ToolResult::error(format!(
                        "Shell script exited with code {}\n\n[stdout]:\n{}\n\n[stderr]:\n{}",
                        exit_code, stdout, stderr
                    )))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("Failed to run shell: {}", e)))
            }
            Err(_) => {
                cmd.kill().await.ok();
                Ok(ToolResult::error(format!(
                    "Shell script timed out after {} seconds", timeout_secs
                )))
            }
        }
    }
    
    /// Execute Rust code (requires cargo)
    async fn execute_rust(&self, code: &str, timeout_secs: u64) -> anyhow::Result<ToolResult> {
        // Check for Rust
        if !self.is_available("rustc").await {
            return Ok(ToolResult::error(
                "Rust is not installed. Use the dependency_manager tool to install it:\n\
                 dependency_manager install rust"
            ));
        }
        
        // For simple Rust scripts, we can use `cargo script` if available
        // Otherwise compile and run a temporary project
        let temp_dir = std::env::temp_dir().join(format!("horcrust_rust_{}", std::process::id()));
        tokio::fs::create_dir_all(&temp_dir).await?;
        
        // Create a minimal Cargo project
        let cargo_toml = r#"[package]
name = "temp_script"
version = "0.1.0"
edition = "2021"
"#;
        
        tokio::fs::write(temp_dir.join("Cargo.toml"), cargo_toml).await?;
        tokio::fs::create_dir_all(temp_dir.join("src")).await?;
        
        // Create main.rs with the provided code
        let full_code = if code.contains("fn main") {
            code.to_string()
        } else {
            format!("fn main() {{\n{}\n}}", code)
        };
        
        tokio::fs::write(temp_dir.join("src/main.rs"), full_code).await?;
        
        // Compile and run
        let mut cmd = Command::new("cargo")
            .args(["run", "--quiet"])
            .current_dir(&temp_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let result = timeout(Duration::from_secs(timeout_secs), cmd.wait()).await;
        
        // Clean up temp directory
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        
        match result {
            Ok(Ok(status)) => {
                let stdout = if let Some(mut output) = cmd.stdout.take() {
                    let mut buf = String::new();
                    output.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                let stderr = if let Some(mut err) = cmd.stderr.take() {
                    let mut buf = String::new();
                    err.read_to_string(&mut buf).await?;
                    buf
                } else {
                    String::new()
                };
                
                if status.success() {
                    Ok(ToolResult::success(stdout))
                } else {
                    let exit_code = status.code().unwrap_or(-1);
                    Ok(ToolResult::error(format!(
                        "Rust program exited with code {}\n\n[stdout]:\n{}\n\n[stderr]:\n{}",
                        exit_code, stdout, stderr
                    )))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("Failed to run Rust: {}", e)))
            }
            Err(_) => {
                cmd.kill().await.ok();
                Ok(ToolResult::error(format!(
                    "Rust program timed out after {} seconds", timeout_secs
                )))
            }
        }
    }
    
    /// List available languages
    async fn list_languages(&self) -> String {
        let mut output = String::from("🚀 Available Code Execution Environments:\n\n");
        
        let languages = vec![
            ("python3", "Python 3"),
            ("python", "Python"),
            ("node", "Node.js"),
            ("rustc", "Rust"),
        ];
        
        for (cmd, name) in languages {
            let status = if self.is_available(cmd).await { "✅" } else { "❌" };
            output.push_str(&format!("{} {} ({}): {}\n", 
                status, name, cmd, 
                if self.is_available(cmd).await { "available" } else { "not installed" }));
        }
        
        output.push_str("\n💡 Install missing languages with:\n");
        output.push_str("  - dependency_manager install python\n");
        output.push_str("  - dependency_manager install nodejs\n");
        output.push_str("  - dependency_manager install rust\n");
        
        output
    }
}

#[async_trait]
impl Tool for CodeExecutorTool {
    fn name(&self) -> &str {
        "code_executor"
    }
    
    fn description(&self) -> &str {
        "Execute code locally in Python, Node.js, Rust, or shell scripts. \
         Use this when you need to run code without API calls, \
         or when APIs are rate-limited or unavailable. \
         Automatically handles temporary files and cleanup. \
         Can also be used for data processing, calculations, or local automation."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "nodejs", "rust", "shell", "list"],
                    "description": "Programming language to execute (python, nodejs, rust, shell) or 'list' to see available languages"
                },
                "code": {
                    "type": "string",
                    "description": "Code to execute (required for all except 'list' action)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60, max: 300)",
                    "default": 60,
                    "minimum": 1,
                    "maximum": 300
                }
            },
            "required": ["language"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let language = args["language"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'language' parameter"))?;
        
        let timeout_secs = args["timeout"].as_u64()
            .map(|t| t.min(300).max(1))
            .unwrap_or(self.default_timeout_secs);
        
        if language == "list" {
            let output = self.list_languages().await;
            return Ok(ToolResult::success(output));
        }
        
        let code = args["code"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'code' parameter"))?;
        
        match language {
            "python" => self.execute_python(code, timeout_secs).await,
            "nodejs" | "javascript" | "js" => self.execute_node(code, timeout_secs).await,
            "rust" => self.execute_rust(code, timeout_secs).await,
            "shell" | "bash" | "sh" | "cmd" | "powershell" => self.execute_shell(code, timeout_secs).await,
            _ => Ok(ToolResult::error(format!(
                "Unsupported language: {}. Available: python, nodejs, rust, shell",
                language
            ))),
        }
    }
}
