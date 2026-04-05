//! Memory Tool - Active memory management for the agent
//!
//! Allows the agent to add, replace, and remove memories during conversations.
//! Memories are persisted to MEMORY.md and USER.md files.

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

/// Memory entry with metadata
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub content: String,
    pub timestamp: String,
    pub category: String, // "user" or "agent"
}

/// Memory manager for active memory operations
pub struct MemoryTool {
    memory_file: PathBuf,
    user_file: PathBuf,
    max_memory_chars: usize,
    max_user_chars: usize,
}

impl MemoryTool {
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("horcrux");
        
        Self {
            memory_file: config_dir.join("MEMORY.md"),
            user_file: config_dir.join("USER.md"),
            max_memory_chars: 2200, // ~800 tokens
            max_user_chars: 1375,   // ~500 tokens
        }
    }
    
    /// Read current memory content
    fn read_memory(&self, target: &str) -> anyhow::Result<String> {
        let path = if target == "user" {
            &self.user_file
        } else {
            &self.memory_file
        };
        
        if path.exists() {
            Ok(std::fs::read_to_string(path)?)
        } else {
            Ok(String::new())
        }
    }
    
    /// Write memory content
    fn write_memory(&self, target: &str, content: &str) -> anyhow::Result<()> {
        let path = if target == "user" {
            &self.user_file
        } else {
            &self.memory_file
        };
        
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(path, content)?;
        Ok(())
    }
    
    /// Extract entries from memory content (split by § delimiter)
    fn extract_entries(&self, content: &str) -> Vec<String> {
        content
            .split('§')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
    
    /// Join entries back into memory format
    fn join_entries(&self, entries: &[String]) -> String {
        if entries.is_empty() {
            return String::new();
        }
        
        let header = if entries[0].contains("AGENT NOTES") || entries[0].contains("USER PROFILE") {
            entries[0].clone()
        } else {
            String::new()
        };
        
        let body: Vec<String> = if header.is_empty() {
            entries.to_vec()
        } else {
            entries[1..].to_vec()
        };
        
        let joined = body.join("\n\n§\n\n");
        
        if header.is_empty() {
            joined
        } else {
            format!("{}\n\n§\n\n{}", header, joined)
        }
    }
    
    /// Check if content would exceed limits
    fn check_limits(&self, target: &str, new_content: &str) -> Result<(), String> {
        let limit = if target == "user" {
            self.max_user_chars
        } else {
            self.max_memory_chars
        };
        
        if new_content.len() > limit {
            return Err(format!(
                "Memory would exceed limit: {}/{} chars. Consider consolidating or removing entries first.",
                new_content.len(),
                limit
            ));
        }
        
        Ok(())
    }
    
    /// Add a new memory entry
    async fn add_memory(&self, target: &str, content: &str) -> ToolResult {
        let mut memory = self.read_memory(target).unwrap_or_default();
        
        // Check for exact duplicates
        if memory.contains(content) {
            return ToolResult::success("Memory entry already exists (duplicate).".to_string());
        }
        
        // Add delimiter and new content
        let new_entry = if memory.is_empty() {
            // Initialize with header
            let header = if target == "user" {
                "═══════════════════════════════════════════════════\nUSER PROFILE [Preferences, communication style, pet peeves]\n═══════════════════════════════════════════════════"
            } else {
                "═══════════════════════════════════════════════════\nAGENT NOTES [Personal observations about environment and workflows]\n═══════════════════════════════════════════════════"
            };
            format!("{}\n\n§\n\n{}", header, content)
        } else {
            format!("{}\n\n§\n\n{}", memory, content)
        };
        
        // Check limits
        if let Err(e) = self.check_limits(target, &new_entry) {
            // Return current entries to help user decide what to remove
            let entries = self.extract_entries(&memory);
            let current = entries.iter().enumerate()
                .map(|(i, e)| format!("[{}] {}", i, e.lines().next().unwrap_or("...").chars().take(50).collect::<String>()))
                .collect::<Vec<_>>()
                .join("\n");
            
            return ToolResult::error(format!(
                "{}\n\nCurrent entries:\n{}",
                e, current
            ));
        }
        
        if let Err(e) = self.write_memory(target, &new_entry) {
            return ToolResult::error(format!("Failed to save memory: {}", e));
        }
        
        ToolResult::success(format!(
            "✅ Added to {} memory ({} chars used).",
            target,
            new_entry.len()
        ))
    }
    
    /// Replace an existing memory entry
    async fn replace_memory(&self, target: &str, old_text: &str, new_content: &str) -> ToolResult {
        let memory = self.read_memory(target).unwrap_or_default();
        
        // Find entry containing old_text
        let entries = self.extract_entries(&memory);
        let mut found = false;
        let mut new_entries = Vec::new();
        let mut match_count = 0;
        
        for entry in &entries {
            if entry.contains(old_text) {
                match_count += 1;
                if match_count == 1 {
                    found = true;
                    new_entries.push(new_content.to_string());
                } else {
                    // Keep other matches as-is (they're different entries)
                    new_entries.push(entry.clone());
                }
            } else {
                new_entries.push(entry.clone());
            }
        }
        
        if !found {
            if match_count > 1 {
                return ToolResult::error(
                    format!("Multiple entries match '{}'. Please use a more specific substring.", old_text)
                );
            }
            return ToolResult::error(
                format!("No entry found matching '{}'. Use 'memory_search' to find the exact text.", old_text)
            );
        }
        
        let new_memory = self.join_entries(&new_entries);
        
        // Check limits
        if let Err(e) = self.check_limits(target, &new_memory) {
            return ToolResult::error(e);
        }
        
        if let Err(e) = self.write_memory(target, &new_memory) {
            return ToolResult::error(format!("Failed to update memory: {}", e));
        }
        
        ToolResult::success(format!(
            "✅ Replaced entry in {} memory ({} chars used).",
            target,
            new_memory.len()
        ))
    }
    
    /// Remove a memory entry
    async fn remove_memory(&self, target: &str, old_text: &str) -> ToolResult {
        let memory = self.read_memory(target).unwrap_or_default();
        
        let entries = self.extract_entries(&memory);
        let mut found = false;
        let mut new_entries = Vec::new();
        let mut match_count = 0;
        
        for entry in &entries {
            if entry.contains(old_text) {
                match_count += 1;
                if match_count == 1 {
                    found = true;
                    // Skip this entry (remove it)
                } else {
                    new_entries.push(entry.clone());
                }
            } else {
                new_entries.push(entry.clone());
            }
        }
        
        if !found {
            if match_count > 1 {
                return ToolResult::error(
                    format!("Multiple entries match '{}'. Please use a more specific substring.", old_text)
                );
            }
            return ToolResult::error(
                format!("No entry found matching '{}'.", old_text)
            );
        }
        
        let new_memory = self.join_entries(&new_entries);
        
        if let Err(e) = self.write_memory(target, &new_memory) {
            return ToolResult::error(format!("Failed to update memory: {}", e));
        }
        
        ToolResult::success(format!(
            "✅ Removed entry from {} memory ({} chars remaining).",
            target,
            new_memory.len()
        ))
    }
    
    /// Get memory status and current entries
    async fn get_status(&self) -> ToolResult {
        let memory_content = self.read_memory("memory").unwrap_or_default();
        let user_content = self.read_memory("user").unwrap_or_default();
        
        let memory_entries = self.extract_entries(&memory_content).len();
        let user_entries = self.extract_entries(&user_content).len();
        
        let output = format!(
            "Memory Status:\n\n\
            AGENT MEMORY: {}/{} chars ({} entries)\n\
            USER PROFILE: {}/{} chars ({} entries)\n\n\
            Files:\n\
            - {}\n\
            - {}",
            memory_content.len(), self.max_memory_chars, memory_entries.saturating_sub(1),
            user_content.len(), self.max_user_chars, user_entries.saturating_sub(1),
            self.memory_file.display(),
            self.user_file.display()
        );
        
        ToolResult::success(output)
    }
}

#[async_trait]
impl Tool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }
    
    fn description(&self) -> &str {
        "Active memory management - add, replace, or remove memories about the user and environment. \
         Memories persist across sessions and shape the agent's behavior. \
         Target 'memory' for agent notes (environment, workflows) or 'user' for user profile (preferences, style)."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "replace", "remove", "status"],
                    "description": "Action to perform: add (new entry), replace (update existing), remove (delete entry), status (show usage)"
                },
                "target": {
                    "type": "string",
                    "enum": ["memory", "user"],
                    "description": "Which memory store: 'memory' (agent's personal notes) or 'user' (user profile)"
                },
                "content": {
                    "type": "string",
                    "description": "For 'add': the new memory content. For 'replace': the replacement content."
                },
                "old_text": {
                    "type": "string",
                    "description": "For 'replace' and 'remove': unique substring of the entry to find. Must match exactly one entry."
                }
            },
            "required": ["action", "target"]
        })
    }
    
    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let action = args["action"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;
        let target = args["target"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: target"))?;
        
        match action {
            "add" => {
                let content = args["content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content for add action"))?;
                Ok(self.add_memory(target, content).await)
            }
            "replace" => {
                let old_text = args["old_text"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_text for replace action"))?;
                let content = args["content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content for replace action"))?;
                Ok(self.replace_memory(target, old_text, content).await)
            }
            "remove" => {
                let old_text = args["old_text"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_text for remove action"))?;
                Ok(self.remove_memory(target, old_text).await)
            }
            "status" => {
                Ok(self.get_status().await)
            }
            _ => Ok(ToolResult::error(format!("Unknown action: {}", action)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tool_creation() {
        let tool = MemoryTool::new();
        assert_eq!(tool.name(), "memory");
    }
}
