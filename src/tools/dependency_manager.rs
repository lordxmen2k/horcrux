//! Dependency Manager Tool - Install languages and tools on demand
//!
//! Allows the agent to self-install required dependencies with user consent

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;

/// Dependency manager for installing languages and tools
pub struct DependencyManagerTool;

impl DependencyManagerTool {
    pub fn new() -> Self {
        Self
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
    
    /// Install Python
    async fn install_python(&self, confirmation: &str) -> anyhow::Result<String> {
        if confirmation != "I consent to installing Python" {
            return Err(anyhow::anyhow!(
                "User consent required. Provide confirmation: 'I consent to installing Python'"
            ));
        }
        
        if cfg!(windows) {
            // Try winget first, then fallback to Microsoft Store
            let result = Command::new("winget")
                .args(["install", "Python.Python.3.11", "--silent", "--accept-package-agreements", "--accept-source-agreements"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Python installed successfully via winget".to_string())
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                Err(anyhow::anyhow!("Failed to install Python: {}", stderr))
            }
        } else if cfg!(target_os = "macos") {
            let result = Command::new("brew")
                .args(["install", "python@3.11"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Python installed successfully via Homebrew".to_string())
            } else {
                Err(anyhow::anyhow!("Failed to install Python. Try: brew install python@3.11"))
            }
        } else {
            // Linux
            let result = Command::new("apt-get")
                .args(["update"])
                .output()
                .await?;
            
            let result = Command::new("apt-get")
                .args(["install", "-y", "python3", "python3-pip"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Python installed successfully via apt".to_string())
            } else {
                Err(anyhow::anyhow!("Failed to install Python. Try: sudo apt-get install python3 python3-pip"))
            }
        }
    }
    
    /// Install Node.js
    async fn install_nodejs(&self, confirmation: &str) -> anyhow::Result<String> {
        if confirmation != "I consent to installing Node.js" {
            return Err(anyhow::anyhow!(
                "User consent required. Provide confirmation: 'I consent to installing Node.js'"
            ));
        }
        
        if cfg!(windows) {
            let result = Command::new("winget")
                .args(["install", "OpenJS.NodeJS", "--silent", "--accept-package-agreements"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Node.js installed successfully via winget".to_string())
            } else {
                Err(anyhow::anyhow!("Failed to install Node.js"))
            }
        } else if cfg!(target_os = "macos") {
            let result = Command::new("brew")
                .args(["install", "node"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Node.js installed successfully via Homebrew".to_string())
            } else {
                Err(anyhow::anyhow!("Failed to install Node.js. Try: brew install node"))
            }
        } else {
            let result = Command::new("apt-get")
                .args(["update"])
                .output()
                .await?;
            
            let result = Command::new("apt-get")
                .args(["install", "-y", "nodejs", "npm"])
                .output()
                .await?;
            
            if result.status.success() {
                Ok("✅ Node.js installed successfully via apt".to_string())
            } else {
                Err(anyhow::anyhow!("Failed to install Node.js"))
            }
        }
    }
    
    /// Install Rust
    async fn install_rust(&self, confirmation: &str) -> anyhow::Result<String> {
        if confirmation != "I consent to installing Rust" {
            return Err(anyhow::anyhow!(
                "User consent required. Provide confirmation: 'I consent to installing Rust'"
            ));
        }
        
        // Rustup works on all platforms
        let result = Command::new("sh")
            .arg("-c")
            .arg("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y")
            .output()
            .await?;
        
        if result.status.success() {
            Ok("✅ Rust installed successfully via rustup".to_string())
        } else {
            Err(anyhow::anyhow!("Failed to install Rust. Visit: https://rustup.rs"))
        }
    }
    
    /// Install a Python package
    async fn install_python_package(&self, package: &str, confirmation: &str) -> anyhow::Result<String> {
        if confirmation != "I consent to installing Python packages" {
            return Err(anyhow::anyhow!(
                "User consent required. Provide confirmation: 'I consent to installing Python packages'"
            ));
        }
        
        // First check if pip is available
        if !self.is_available("pip").await && !self.is_available("pip3").await {
            return Err(anyhow::anyhow!(
                "pip not found. Install Python first using: dependency_manager install python"
            ));
        }
        
        let pip_cmd = if self.is_available("pip3").await { "pip3" } else { "pip" };
        
        let result = Command::new(pip_cmd)
            .args(["install", package])
            .output()
            .await?;
        
        if result.status.success() {
            Ok(format!("✅ Python package '{}' installed successfully", package))
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            Err(anyhow::anyhow!("Failed to install {}: {}", package, stderr))
        }
    }
    
    /// Install a Node.js package globally
    async fn install_node_package(&self, package: &str, confirmation: &str) -> anyhow::Result<String> {
        if confirmation != "I consent to installing Node.js packages" {
            return Err(anyhow::anyhow!(
                "User consent required. Provide confirmation: 'I consent to installing Node.js packages'"
            ));
        }
        
        if !self.is_available("npm").await {
            return Err(anyhow::anyhow!(
                "npm not found. Install Node.js first using: dependency_manager install nodejs"
            ));
        }
        
        let result = Command::new("npm")
            .args(["install", "-g", package])
            .output()
            .await?;
        
        if result.status.success() {
            Ok(format!("✅ Node.js package '{}' installed globally", package))
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            Err(anyhow::anyhow!("Failed to install {}: {}", package, stderr))
        }
    }
    
    /// List available package managers
    async fn list_package_managers(&self) -> String {
        let mut output = String::from("📦 Package Manager Status:\n\n");
        
        let managers = vec![
            ("winget", "Windows Package Manager"),
            ("brew", "Homebrew (macOS)"),
            ("apt-get", "APT (Debian/Ubuntu)"),
            ("yum", "YUM (RHEL/CentOS)"),
            ("pacman", "Pacman (Arch)"),
        ];
        
        for (cmd, name) in managers {
            let status = if self.is_available(cmd).await { "✅" } else { "❌" };
            output.push_str(&format!("{} {} ({}): {}\n", status, name, cmd, if self.is_available(cmd).await { "available" } else { "not found" }));
        }
        
        output.push_str("\n🛠️ Language Runtimes:\n");
        let languages = vec![
            ("python", "Python"),
            ("python3", "Python 3"),
            ("node", "Node.js"),
            ("cargo", "Rust"),
            ("go", "Go"),
        ];
        
        for (cmd, name) in languages {
            let status = if self.is_available(cmd).await { "✅" } else { "❌" };
            output.push_str(&format!("{} {} ({}): {}\n", status, name, cmd, if self.is_available(cmd).await { "available" } else { "not found" }));
        }
        
        output
    }
}

#[async_trait]
impl Tool for DependencyManagerTool {
    fn name(&self) -> &str {
        "dependency_manager"
    }
    
    fn description(&self) -> &str {
        "Check for and install programming languages, tools, and packages. \
         Use this when you need Python, Node.js, or other tools that aren't available. \
         Always asks for user consent before installing anything. \
         Can install: python, nodejs, rust, and specific packages."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["check", "install", "install_package"],
                    "description": "Action to perform: check availability, install language, or install package"
                },
                "tool": {
                    "type": "string",
                    "description": "Tool/language to install (for action=install): python, nodejs, rust"
                },
                "package": {
                    "type": "string",
                    "description": "Package name to install (for action=install_package)"
                },
                "package_manager": {
                    "type": "string",
                    "enum": ["pip", "npm"],
                    "description": "Package manager to use (pip for Python, npm for Node.js)"
                },
                "confirmation": {
                    "type": "string",
                    "description": "User consent for installation (required format: 'I consent to installing X')"
                }
            },
            "required": ["action"]
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let action = args["action"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' parameter"))?;
        
        match action {
            "check" => {
                let output = self.list_package_managers().await;
                Ok(ToolResult::success(output))
            }
            "install" => {
                let tool = args["tool"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'tool' parameter for install"))?;
                let confirmation = args["confirmation"].as_str().unwrap_or("");
                
                let result = match tool {
                    "python" | "python3" => self.install_python(confirmation).await,
                    "nodejs" | "node" | "npm" => self.install_nodejs(confirmation).await,
                    "rust" | "cargo" => self.install_rust(confirmation).await,
                    _ => Err(anyhow::anyhow!("Unknown tool: {}. Available: python, nodejs, rust", tool)),
                };
                
                match result {
                    Ok(msg) => Ok(ToolResult::success(msg)),
                    Err(e) => Ok(ToolResult::error(format!("{}", e))),
                }
            }
            "install_package" => {
                let package = args["package"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'package' parameter"))?;
                let package_manager = args["package_manager"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'package_manager' parameter (pip or npm)"))?;
                let confirmation = args["confirmation"].as_str().unwrap_or("");
                
                let result = match package_manager {
                    "pip" => self.install_python_package(package, confirmation).await,
                    "npm" => self.install_node_package(package, confirmation).await,
                    _ => Err(anyhow::anyhow!("Unknown package manager: {}. Use 'pip' or 'npm'", package_manager)),
                };
                
                match result {
                    Ok(msg) => Ok(ToolResult::success(msg)),
                    Err(e) => Ok(ToolResult::error(format!("{}", e))),
                }
            }
            _ => Ok(ToolResult::error(format!("Unknown action: {}. Use check, install, or install_package", action))),
        }
    }
}
