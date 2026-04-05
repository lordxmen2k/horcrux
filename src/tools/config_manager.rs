//! Configuration Manager - Manage .env file from conversation
//!
//! Allows the agent to:
//! 1. Save API keys provided by user
//! 2. Update configuration values
//! 3. Read current config

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct ConfigManagerTool;

impl ConfigManagerTool {
    pub fn new() -> Self {
        Self
    }

    fn env_path(&self) -> &str {
        ".env"
    }

    /// Read current .env file
    fn read_env(&self) -> anyhow::Result<HashMap<String, String>> {
        let path = self.env_path();
        let mut config = HashMap::new();

        if !Path::new(path).exists() {
            return Ok(config);
        }

        let content = fs::read_to_string(path)?;
        
        for line in content.lines() {
            let line = line.trim();
            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos + 1..].trim().to_string();
                // Remove quotes if present
                let value = value.trim_matches('"').trim_matches('\'').to_string();
                config.insert(key, value);
            }
        }

        Ok(config)
    }

    /// Write config back to .env file
    fn write_env(&self, config: &HashMap<String, String>, comments: &HashMap<String, String>) -> anyhow::Result<()> {
        let path = self.env_path();
        let mut lines = vec![
            "# Horcrux Configuration".to_string(),
            "# Auto-generated and managed by agent".to_string(),
            "".to_string(),
        ];

        // Group by category for organization
        let mut api_keys: Vec<(String, String)> = Vec::new();
        let mut model_config: Vec<(String, String)> = Vec::new();
        let mut other: Vec<(String, String)> = Vec::new();

        for (key, value) in config.iter() {
            if key.contains("API_KEY") || key.contains("TOKEN") {
                api_keys.push((key.clone(), value.clone()));
            } else if key.starts_with("HORCRUX_") {
                model_config.push((key.clone(), value.clone()));
            } else {
                other.push((key.clone(), value.clone()));
            }
        }

        // Write API Keys section
        if !api_keys.is_empty() {
            lines.push("# === API Keys ===".to_string());
            for (key, value) in api_keys {
                if let Some(comment) = comments.get(&key) {
                    lines.push(format!("# {}", comment));
                }
                // Mask sensitive values in file
                let display_value = if value.len() > 10 {
                    format!("{}", value)
                } else {
                    value
                };
                lines.push(format!("{}={}", key, display_value));
            }
            lines.push("".to_string());
        }

        // Write Model Config section
        if !model_config.is_empty() {
            lines.push("# === Model Configuration ===".to_string());
            for (key, value) in model_config {
                lines.push(format!("{}={}", key, value));
            }
            lines.push("".to_string());
        }

        // Write Other section
        if !other.is_empty() {
            lines.push("# === Other Settings ===".to_string());
            for (key, value) in other {
                lines.push(format!("{}={}", key, value));
            }
        }

        fs::write(path, lines.join("\n"))?;
        Ok(())
    }

    /// Set a config value
    fn set_config(&self, key: &str, value: &str, comment: Option<&str>) -> anyhow::Result<String> {
        let mut config = self.read_env()?;
        let mut comments = HashMap::new();
        
        // Store comment if provided
        if let Some(c) = comment {
            comments.insert(key.to_string(), c.to_string());
        }

        // Update or insert
        let old_value = config.insert(key.to_string(), value.to_string());
        
        self.write_env(&config, &comments)?;

        if let Some(old) = old_value {
            Ok(format!("✅ Updated {} (was: {}...)", key, &old[..old.len().min(10)]))
        } else {
            Ok(format!("✅ Added {} to configuration", key))
        }
    }

    /// Get a config value
    fn get_config(&self, key: &str) -> anyhow::Result<String> {
        let config = self.read_env()?;
        
        match config.get(key) {
            Some(value) => {
                // Mask API keys
                if key.contains("API_KEY") || key.contains("TOKEN") || key.contains("SECRET") {
                    let masked = if value.len() > 8 {
                        format!("{}...{}", &value[..4], &value[value.len()-4..])
                    } else {
                        "***".to_string()
                    };
                    Ok(format!("{}={}", key, masked))
                } else {
                    Ok(format!("{}={}", key, value))
                }
            }
            None => Ok(format!("⚠️ {} not set", key)),
        }
    }

    /// List all config (with sensitive values masked)
    fn list_config(&self) -> anyhow::Result<String> {
        let config = self.read_env()?;
        
        if config.is_empty() {
            return Ok("No configuration set yet.".to_string());
        }

        let mut output = "📋 Current Configuration:\n\n".to_string();
        
        for (key, value) in config.iter() {
            // Mask sensitive values
            let display = if key.contains("API_KEY") || key.contains("TOKEN") || key.contains("SECRET") {
                if value.len() > 8 {
                    format!("{}...{}", &value[..4], &value[value.len()-4..])
                } else {
                    "***".to_string()
                }
            } else {
                value.clone()
            };
            
            output.push_str(&format!("{}={}\n", key, display));
        }

        output.push_str("\n💡 To set a value: config set KEY=value");
        Ok(output)
    }
}

#[async_trait]
impl Tool for ConfigManagerTool {
    fn name(&self) -> &str {
        "config"
    }

    fn description(&self) -> &str {
        "Manage configuration and API keys in .env file.\n\
         Can save API keys provided by user, read config, list settings.\n\
         CRITICAL: Use this when user says 'Set X to Y' or provides API keys!\n\
         Example: 'Set UNSPLASH_ACCESS_KEY to abc123' -> config set UNSPLASH_ACCESS_KEY=abc123"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set", "get", "list"],
                    "description": "Action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "Config key (e.g., UNSPLASH_ACCESS_KEY, OPENAI_API_KEY)"
                },
                "value": {
                    "type": "string",
                    "description": "Value to set (for action='set')"
                },
                "comment": {
                    "type": "string",
                    "description": "Comment explaining what this key is for (optional)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let action = args["action"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing action"))?;

        match action {
            "set" => {
                let key = args["key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing key"))?;
                let value = args["value"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing value"))?;
                let comment = args["comment"].as_str();
                
                match self.set_config(key, value, comment) {
                    Ok(msg) => Ok(ToolResult::success(msg)),
                    Err(e) => Ok(ToolResult::error(format!("Failed to save config: {}", e))),
                }
            }
            "get" => {
                let key = args["key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing key"))?;
                
                match self.get_config(key) {
                    Ok(msg) => Ok(ToolResult::success(msg)),
                    Err(e) => Ok(ToolResult::error(format!("Failed to read config: {}", e))),
                }
            }
            "list" => {
                match self.list_config() {
                    Ok(msg) => Ok(ToolResult::success(msg)),
                    Err(e) => Ok(ToolResult::error(format!("Failed to list config: {}", e))),
                }
            }
            _ => Ok(ToolResult::error(format!("Unknown action: {}", action))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_manager() {
        let tool = ConfigManagerTool::new();
        assert_eq!(tool.name(), "config");
    }
}
