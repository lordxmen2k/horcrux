//! Dynamic Skill System - Create and manage custom skills/tools
//!
//! Skills are user-created tools that can be generated on the fly and reused.
//! They can be:
//! - Shell scripts (cross-platform with bash/sh)
//! - Python scripts (if Python is available)
//! - Composite tools (combinations of existing tools)

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// A custom skill created by the agent or user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON schema
    pub implementation: SkillImplementation,
    pub created_at: String,
    pub usage_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SkillImplementation {
    #[serde(rename = "shell")]
    Shell {
        script: String,
        interpreter: String, // "bash", "sh", "python3", "python", "cmd", "powershell"
    },
    #[serde(rename = "composite")]
    Composite {
        steps: Vec<CompositeStep>,
    },
    #[serde(rename = "template")]
    Template {
        template: String, // Tera-like template for command generation
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeStep {
    pub tool: String,
    pub arguments: Value,
    pub output_var: Option<String>, // Store output in variable for later steps
}

/// Manager for dynamic skills
pub struct SkillManager {
    skills_dir: PathBuf,
    skills: HashMap<String, Skill>,
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf) -> Self {
        // Ensure skills directory exists
        if let Err(e) = std::fs::create_dir_all(&skills_dir) {
            eprintln!("⚠️ Failed to create skills directory {:?}: {}", skills_dir, e);
        } else {
            println!("📁 Skills directory: {:?}", skills_dir);
        }
        
        let mut manager = Self {
            skills_dir,
            skills: HashMap::new(),
        };
        
        // Load built-in skills first
        manager.load_builtin_skills();
        
        // Then load user skills (which can override built-ins)
        if let Err(e) = manager.load_skills() {
            eprintln!("⚠️ Failed to load skills: {}", e);
        } else {
            let count = manager.skills.len();
            if count > 0 {
                println!("📚 Loaded {} skills (built-in + custom)", count);
            }
        }
        manager
    }
    
    /// Load built-in skills that ship with horcrux
    fn load_builtin_skills(&mut self) {
        use crate::tools::skills_library::get_builtin_skills;
        
        for skill in get_builtin_skills() {
            self.skills.insert(skill.name.clone(), skill);
        }
    }

    /// Load all skills from disk
    fn load_skills(&mut self) -> anyhow::Result<()> {
        if !self.skills_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(skill) = serde_json::from_str::<Skill>(&content) {
                        self.skills.insert(skill.name.clone(), skill);
                    }
                }
            }
        }

        Ok(())
    }

    /// Save a skill to disk
    fn save_skill(&self, skill: &Skill) -> anyhow::Result<()> {
        let filename = format!("{}.json", sanitize_filename(&skill.name));
        let path = self.skills_dir.join(&filename);
        let content = serde_json::to_string_pretty(skill)?;
        std::fs::write(&path, &content)?;
        println!("💾 Skill saved: {:?}", path);
        Ok(())
    }

    /// Create a new skill
    pub fn create_skill(&mut self, skill: Skill) -> anyhow::Result<()> {
        self.save_skill(&skill)?;
        self.skills.insert(skill.name.clone(), skill);
        Ok(())
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }
    
    /// Find skills by keyword in name or description
    pub fn find_skills_by_keyword(&self, keyword: &str) -> Vec<&Skill> {
        let keyword_lower = keyword.to_lowercase();
        self.skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&keyword_lower) ||
                s.description.to_lowercase().contains(&keyword_lower)
            })
            .collect()
    }
    
    /// Find the best matching skill for a request type
    pub fn find_best_skill_for(&self, request_type: &str) -> Option<&Skill> {
        let request_lower = request_type.to_lowercase();
        let keywords: Vec<&str> = request_lower.split_whitespace().collect();
        
        // Find skills that match most keywords
        let mut best_match: Option<&Skill> = None;
        let mut best_score = 0;
        
        for skill in self.skills.values() {
            let skill_text = format!("{} {}", skill.name, skill.description).to_lowercase();
            let score = keywords.iter()
                .filter(|k| skill_text.contains(*k))
                .count();
            
            if score > best_score {
                best_score = score;
                best_match = Some(skill);
            }
        }
        
        // Only return if at least one keyword matched
        if best_score > 0 {
            best_match
        } else {
            None
        }
    }

    /// List all available skills
    pub fn list_skills(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Delete a skill
    pub fn delete_skill(&mut self, name: &str) -> anyhow::Result<()> {
        let filename = format!("{}.json", sanitize_filename(name));
        let path = self.skills_dir.join(filename);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.skills.remove(name);
        Ok(())
    }

    /// Convert a skill to a Tool
    pub fn skill_to_tool(&self, skill: Skill) -> SkillTool {
        SkillTool { skill }
    }

    /// Get all skills as tools
    pub fn get_all_tools(&self) -> Vec<SkillTool> {
        self.skills
            .values()
            .cloned()
            .map(|s| SkillTool { skill: s })
            .collect()
    }

    /// Increment usage count for a skill
    pub fn record_usage(&mut self, name: &str) {
        if let Some(skill) = self.skills.get_mut(name) {
            skill.usage_count += 1;
            let skill_clone = skill.clone();
            let _ = self.save_skill(&skill_clone);
        }
    }
}

/// A Tool wrapper for a Skill
pub struct SkillTool {
    skill: Skill,
}

impl SkillTool {
    async fn execute_shell(&self, script: &str, interpreter: &str, args: &Value) -> anyhow::Result<ToolResult> {
        // Replace template variables in script with args
        let mut final_script = script.to_string();
        
        if let Some(obj) = args.as_object() {
            for (key, value) in obj {
                let placeholder = format!("{{{{{}}}}}", key);
                let value_string = value.to_string();
                let replacement = value.as_str().unwrap_or(&value_string);
                final_script = final_script.replace(&placeholder, replacement);
            }
        }

        // Determine command based on interpreter
        let (cmd, cmd_args): (&str, Vec<&str>) = match interpreter {
            "bash" => ("bash", vec!["-c", &final_script]),
            "sh" => ("sh", vec!["-c", &final_script]),
            "python3" => {
                // Write script to temp file for Python
                let temp_path = std::env::temp_dir().join(format!("skill_{}.py", rand::random::<u32>()));
                std::fs::write(&temp_path, &final_script)?;
                return self.run_python(&temp_path, args).await;
            }
            "python" => {
                let temp_path = std::env::temp_dir().join(format!("skill_{}.py", rand::random::<u32>()));
                std::fs::write(&temp_path, &final_script)?;
                return self.run_python(&temp_path, args).await;
            }
            "cmd" => ("cmd", vec!["/C", &final_script]),
            "powershell" => ("powershell", vec!["-Command", &final_script]),
            _ => ("sh", vec!["-c", &final_script]),
        };

        let output = timeout(
            Duration::from_secs(60),
            Command::new(cmd).args(&cmd_args).output()
        ).await;

        match output {
            Ok(Ok(result)) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                
                let mut output_text = String::new();
                if !stdout.is_empty() {
                    output_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    output_text.push_str(&format!("\nSTDERR:\n{}", stderr));
                }

                if result.status.success() {
                    Ok(ToolResult::success(output_text))
                } else {
                    Ok(ToolResult::error(format!(
                        "Exit code: {:?}\n{}",
                        result.status.code(),
                        output_text
                    )))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("Execution failed: {}", e))),
            Err(_) => Ok(ToolResult::error("Script timed out after 60 seconds")),
        }
    }

    async fn run_python(&self, script_path: &Path, _args: &Value) -> anyhow::Result<ToolResult> {
        let output = timeout(
            Duration::from_secs(60),
            Command::new("python3").arg(script_path).output()
        ).await;

        // Clean up temp file
        let _ = std::fs::remove_file(script_path);

        match output {
            Ok(Ok(result)) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                
                let mut output_text = String::new();
                if !stdout.is_empty() {
                    output_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    output_text.push_str(&format!("\nSTDERR:\n{}", stderr));
                }

                if result.status.success() {
                    Ok(ToolResult::success(output_text))
                } else {
                    Ok(ToolResult::error(output_text))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("Python execution failed: {}", e))),
            Err(_) => Ok(ToolResult::error("Python script timed out")),
        }
    }

    async fn execute_composite(&self, steps: &[CompositeStep], _args: &Value) -> anyhow::Result<ToolResult> {
        let mut outputs: HashMap<String, String> = HashMap::new();
        let mut final_output = String::new();

        for (i, step) in steps.iter().enumerate() {
            // This is a simplified version - in a full implementation,
            // we'd execute the actual tool here
            final_output.push_str(&format!(
                "Step {}: Would execute {} with {:?}\n",
                i + 1,
                step.tool,
                step.arguments
            ));
        }

        Ok(ToolResult::success(final_output))
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        &self.skill.name
    }

    fn description(&self) -> &str {
        &self.skill.description
    }

    fn parameters_schema(&self) -> Value {
        self.skill.parameters.clone()
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        match &self.skill.implementation {
            SkillImplementation::Shell { script, interpreter } => {
                self.execute_shell(script, interpreter, &args).await
            }
            SkillImplementation::Composite { steps } => {
                self.execute_composite(steps, &args).await
            }
            SkillImplementation::Template { template: _ } => {
                // Template skills are expanded before execution
                Ok(ToolResult::error("Template skills not yet implemented"))
            }
        }
    }
}

/// Sanitize a string for use as a filename
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

/// Tool for creating new skills
pub struct CreateSkillTool {
    skill_manager: std::sync::Mutex<SkillManager>,
}

impl CreateSkillTool {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skill_manager: std::sync::Mutex::new(SkillManager::new(skills_dir)),
        }
    }
}

#[async_trait]
impl Tool for CreateSkillTool {
    fn name(&self) -> &str {
        "create_skill"
    }

    fn description(&self) -> &str {
        "Create a new reusable skill/tool AUTOMATICALLY after completing multi-step tasks. \
         Use this AFTER fetching data from APIs, processing files, or running command sequences. \
         The skill saves your exact approach so you can reuse it instantly next time. \
         ALWAYS create skills for: API fetching tasks, common command patterns, file operations. \
         DO NOT ask user - just create it and tell them what you made."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique name for the skill (use snake_case)"
                },
                "description": {
                    "type": "string",
                    "description": "Clear description of what the skill does and when to use it"
                },
                "type": {
                    "type": "string",
                    "enum": ["shell", "python"],
                    "description": "Type of skill to create"
                },
                "code": {
                    "type": "string",
                    "description": "The script code. Use {{parameter_name}} for variables"
                },
                "parameters": {
                    "type": "object",
                    "description": "JSON schema for parameters this skill accepts",
                    "properties": {
                        "type": { "type": "string", "const": "object" },
                        "properties": { "type": "object" },
                        "required": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            "required": ["name", "description", "type", "code"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let name = args["name"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;
        let description = args["description"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: description"))?;
        let skill_type = args["type"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: type"))?;
        let code = args["code"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: code"))?;

        let interpreter = match skill_type {
            "shell" => "bash",
            "python" => "python3",
            _ => "bash",
        };

        let parameters = args.get("parameters").cloned().unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        });

        let skill = Skill {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            implementation: SkillImplementation::Shell {
                script: code.to_string(),
                interpreter: interpreter.to_string(),
            },
            created_at: chrono::Utc::now().to_rfc3339(),
            usage_count: 0,
        };

        let mut manager = self.skill_manager.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock skill manager: {}", e)
        })?;

        manager.create_skill(skill)?;

        Ok(ToolResult::success(format!(
            "✅ Skill '{}' created successfully!\n\nYou can now use it by calling:\n  tool: {}\n\nDescription: {}",
            name, name, description
        )))
    }
}

/// Tool for listing available skills
pub struct ListSkillsTool {
    skill_manager: std::sync::Mutex<SkillManager>,
}

impl ListSkillsTool {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skill_manager: std::sync::Mutex::new(SkillManager::new(skills_dir)),
        }
    }
}

#[async_trait]
impl Tool for ListSkillsTool {
    fn name(&self) -> &str {
        "list_skills"
    }

    fn description(&self) -> &str {
        "List all custom skills that have been created and are available for use."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        let manager = self.skill_manager.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock skill manager: {}", e)
        })?;

        let skills = manager.list_skills();

        if skills.is_empty() {
            return Ok(ToolResult::success(
                "No custom skills created yet. Use create_skill to make some!".to_string()
            ));
        }

        let mut output = format!("📦 {} Custom Skills Available:\n\n", skills.len());
        
        for skill in skills {
            output.push_str(&format!(
                "• {}\n  {}\n  Usage count: {}\n\n",
                skill.name,
                skill.description,
                skill.usage_count
            ));
        }

        Ok(ToolResult::success(output))
    }
}
