//! File System Tool - Read, write, and list files
//!
//! Cross-platform filesystem operations with automatic path expansion

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct FileSystemTool;

impl FileSystemTool {
    pub fn new() -> Self {
        Self
    }
    
    /// Expand paths like ~/Documents to absolute paths
    fn expand_path(&self, path: &str) -> PathBuf {
        let path = if path.starts_with("~/") {
            // Expand ~ to home directory
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..])
            } else {
                PathBuf::from(path)
            }
        } else {
            PathBuf::from(path)
        };
        
        // Normalize the path
        path.canonicalize().unwrap_or(path)
    }
    
    /// Get helpful error message based on platform
    fn get_error_context(&self, path: &str, err: &std::io::Error) -> String {
        let platform = if cfg!(windows) { "Windows" } else { "Unix" };
        let mut msg = format!("Error accessing '{}': {}", path, err);
        
        if cfg!(windows) {
            if path.contains('/') && !path.contains("\\") {
                msg.push_str("\n\n💡 Hint: You're using Unix-style paths (/) on Windows.");
                msg.push_str("\n   Try using backslashes (\\) instead, e.g.: C:\\Users\\name\\Documents");
            }
            if path.starts_with("~/") {
                if let Some(home) = dirs::home_dir() {
                    msg.push_str(&format!("\n\n💡 Hint: ~ expanded to: {}", home.display()));
                }
            }
        }
        
        if err.kind() == std::io::ErrorKind::NotFound {
            msg.push_str(&format!("\n\n💡 Hint: Make sure the path exists and you have permissions."));
            if let Some(parent) = Path::new(path).parent() {
                msg.push_str(&format!("\n   Parent directory would be: {}", parent.display()));
            }
        }
        
        msg
    }

    fn read_file(&self, path: &str) -> anyhow::Result<String> {
        let expanded = self.expand_path(path);
        match std::fs::read_to_string(&expanded) {
            Ok(content) => Ok(content),
            Err(e) => Err(anyhow::anyhow!("{}", self.get_error_context(path, &e)))
        }
    }

    fn write_file(&self, path: &str, content: &str, append: bool) -> anyhow::Result<String> {
        let expanded = self.expand_path(path);
        let path_obj = Path::new(&expanded);
        
        // Create parent directories if needed
        if let Some(parent) = path_obj.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("Failed to create parent directories: {}", e)
            })?;
        }

        if append {
            let existing = std::fs::read_to_string(&expanded).unwrap_or_default();
            let new_content = format!("{}{}", existing, content);
            std::fs::write(&expanded, new_content).map_err(|e| {
                anyhow::anyhow!("{}", self.get_error_context(path, &e))
            })?;
            Ok(format!("Appended {} bytes to {}", content.len(), path))
        } else {
            std::fs::write(&expanded, content).map_err(|e| {
                anyhow::anyhow!("{}", self.get_error_context(path, &e))
            })?;
            Ok(format!("Wrote {} bytes to {}", content.len(), path))
        }
    }

    fn list_directory(&self, path: &str, recursive: bool) -> anyhow::Result<String> {
        let expanded = self.expand_path(path);
        let mut output = String::new();
        
        // Show what path we're actually using
        output.push_str(&format!("📂 Listing: {}\n", expanded.display()));
        output.push_str("─".repeat(40).as_str());
        output.push('\n');
        
        if recursive {
            for entry in walkdir::WalkDir::new(&expanded).max_depth(10) {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        return Err(anyhow::anyhow!("{}", self.get_error_context(path, &e.into())));
                    }
                };
                let indent = "  ".repeat(entry.depth());
                let name = entry.file_name().to_string_lossy();
                let file_type = if entry.file_type().is_dir() {
                    "📁"
                } else {
                    "📄"
                };
                output.push_str(&format!("{}{} {}\n", indent, file_type, name));
            }
        } else {
            let entries = match std::fs::read_dir(&expanded) {
                Ok(e) => e,
                Err(e) => {
                    return Err(anyhow::anyhow!("{}", self.get_error_context(path, &e)));
                }
            };
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                let file_type = if entry.file_type()?.is_dir() {
                    "📁"
                } else {
                    "📄"
                };
                let size = if entry.file_type()?.is_file() {
                    match entry.metadata() {
                        Ok(m) => format!(" ({} bytes)", m.len()),
                        Err(_) => String::new(),
                    }
                } else {
                    String::new()
                };
                output.push_str(&format!("{} {}{}\n", file_type, name, size));
            }
        }

        if output.trim().lines().count() <= 2 {
            output.push_str("(empty directory)\n");
        }

        Ok(output)
    }

    fn get_file_info(&self, path: &str) -> anyhow::Result<String> {
        let expanded = self.expand_path(path);
        let metadata = std::fs::metadata(&expanded).map_err(|e| {
            anyhow::anyhow!("{}", self.get_error_context(path, &e))
        })?;
        
        let mut info = format!("Path: {} (expanded: {})\n", path, expanded.display());
        info.push_str(&format!("Type: {}\n", 
            if metadata.is_dir() { "Directory" } 
            else if metadata.is_file() { "File" } 
            else { "Other" }
        ));
        info.push_str(&format!("Size: {} bytes\n", metadata.len()));
        
        if let Ok(modified) = metadata.modified() {
            let datetime: chrono::DateTime<chrono::Local> = modified.into();
            info.push_str(&format!("Modified: {}\n", datetime.format("%Y-%m-%d %H:%M:%S")));
        }
        
        if let Ok(created) = metadata.created() {
            let datetime: chrono::DateTime<chrono::Local> = created.into();
            info.push_str(&format!("Created: {}\n", datetime.format("%Y-%m-%d %H:%M:%S")));
        }

        // If it's a file, count lines and words
        if metadata.is_file() {
            if let Ok(content) = std::fs::read_to_string(&expanded) {
                let lines = content.lines().count();
                let words = content.split_whitespace().count();
                info.push_str(&format!("Lines: {}\n", lines));
                info.push_str(&format!("Words: {}\n", words));
            }
        }

        Ok(info)
    }
}

#[async_trait]
impl Tool for FileSystemTool {
    fn name(&self) -> &str {
        "filesystem"
    }

    fn description(&self) -> &str {
        "Read, write, and manage files. Supports ~ home directory expansion. \
        Use this when you need to:\n\
        - Read file contents to understand code, configs, or documents\n\
        - Write or edit files to save results, configs, or scripts\n\
        - List directories to explore project structure\n\
        Operations: read, write, append, list_dir, file_info\n\
        WINDOWS USERS: Use backslashes (C:\\Users\\name\\Documents) or forward slashes (C:/Users/name/Documents)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["read", "write", "append", "list_dir", "file_info"],
                    "description": "The filesystem operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write/append operations)",
                    "optional": true
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List directories recursively (for list_dir operation)",
                    "default": false,
                    "optional": true
                }
            },
            "required": ["operation", "path"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let operation = args["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: operation"))?;
        let path = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

        let result = match operation {
            "read" => match self.read_file(path) {
                Ok(content) => ToolResult::success(content),
                Err(e) => ToolResult::error(format!("Failed to read file '{}': {}", path, e)),
            },
            "write" => {
                let content = args["content"].as_str().unwrap_or("");
                match self.write_file(path, content, false) {
                    Ok(msg) => ToolResult::success(msg),
                    Err(e) => ToolResult::error(format!("Failed to write file '{}': {}", path, e)),
                }
            }
            "append" => {
                let content = args["content"].as_str().unwrap_or("");
                match self.write_file(path, content, true) {
                    Ok(msg) => ToolResult::success(msg),
                    Err(e) => ToolResult::error(format!("Failed to append to file '{}': {}", path, e)),
                }
            }
            "list_dir" => {
                let recursive = args["recursive"].as_bool().unwrap_or(false);
                match self.list_directory(path, recursive) {
                    Ok(listing) => ToolResult::success(listing),
                    Err(e) => ToolResult::error(format!("Failed to list directory '{}': {}", path, e)),
                }
            }
            "file_info" => match self.get_file_info(path) {
                Ok(info) => ToolResult::success(info),
                Err(e) => ToolResult::error(format!("Failed to get file info '{}': {}", path, e)),
            },
            _ => ToolResult::error(format!("Unknown operation: {}", operation)),
        };

        Ok(result)
    }
}
