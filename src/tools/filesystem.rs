//! File System Tool - Read, write, and list files

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

pub struct FileSystemTool;

impl FileSystemTool {
    pub fn new() -> Self {
        Self
    }

    fn read_file(&self, path: &str) -> anyhow::Result<String> {
        let content = std::fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_file(&self, path: &str, content: &str, append: bool) -> anyhow::Result<String> {
        let path_obj = Path::new(path);
        
        // Create parent directories if needed
        if let Some(parent) = path_obj.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if append {
            let existing = std::fs::read_to_string(path).unwrap_or_default();
            let new_content = format!("{}{}", existing, content);
            std::fs::write(path, new_content)?;
            Ok(format!("Appended {} bytes to {}", content.len(), path))
        } else {
            std::fs::write(path, content)?;
            Ok(format!("Wrote {} bytes to {}", content.len(), path))
        }
    }

    fn list_directory(&self, path: &str, recursive: bool) -> anyhow::Result<String> {
        let mut output = String::new();
        
        if recursive {
            for entry in walkdir::WalkDir::new(path).max_depth(10) {
                let entry = entry?;
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
            let entries = std::fs::read_dir(path)?;
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

        if output.is_empty() {
            output = "(empty directory)".to_string();
        }

        Ok(output)
    }

    fn get_file_info(&self, path: &str) -> anyhow::Result<String> {
        let metadata = std::fs::metadata(path)?;
        let path_obj = Path::new(path);
        
        let mut info = format!("Path: {}\n", path);
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
            if let Ok(content) = std::fs::read_to_string(path) {
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
        "Read, write, and manage files. Use this when you need to:\n\
        - Read file contents to understand code, configs, or documents\n\
        - Write or edit files to save results, configs, or scripts\n\
        - List directories to explore project structure\n\
        Operations: read, write, append, list_dir, file_info"
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
