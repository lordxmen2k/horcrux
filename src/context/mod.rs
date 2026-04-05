//! Context Files - Per-project AGENTS.md support

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Context file (AGENTS.md) structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextFile {
    pub project: Option<String>,
    pub description: Option<String>,
    pub instructions: Vec<String>,
    pub key_paths: Vec<String>,
    pub conventions: Vec<String>,
    pub technologies: Vec<String>,
    pub variables: HashMap<String, String>,
    pub raw_content: String,
    pub source_path: PathBuf,
}

impl ContextFile {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut ctx = Self::parse(&content)?;
        ctx.source_path = path.to_path_buf();
        Ok(ctx)
    }

    pub fn parse(content: &str) -> Result<Self> {
        let (frontmatter, body) = Self::split_frontmatter(content);
        
        let mut ctx: ContextFile = if let Some(fm) = frontmatter {
            serde_yaml::from_str(&fm).unwrap_or_default()
        } else {
            ContextFile::default()
        };

        if ctx.instructions.is_empty() && ctx.project.is_none() {
            ctx.parse_body(body.unwrap_or(content));
        }

        ctx.raw_content = content.to_string();
        Ok(ctx)
    }

    fn split_frontmatter(content: &str) -> (Option<String>, Option<&str>) {
        if content.starts_with("---") {
            if let Some(end) = content[3..].find("---") {
                let fm = content[3..end+3].trim();
                let body = &content[end+6..];
                return (Some(fm.to_string()), Some(body));
            }
        }
        (None, None)
    }

    fn parse_body(&mut self, content: &str) {
        let mut current_section: Option<String> = None;
        let mut current_content = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
                if let Some(section) = current_section.take() {
                    self.add_section(&section, &current_content);
                }
                current_section = Some(trimmed.trim_start_matches("# ").trim_start_matches("## ").to_lowercase());
                current_content.clear();
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        if let Some(section) = current_section {
            self.add_section(&section, &current_content);
        }
    }

    fn add_section(&mut self, section: &str, content: &str) {
        let content = content.trim();
        if content.is_empty() { return; }

        match section {
            "project" | "name" => self.project = Some(content.to_string()),
            "description" | "about" => self.description = Some(content.to_string()),
            "instructions" => self.instructions.extend(content.lines().map(|s| s.trim().to_string())),
            "conventions" => self.conventions.extend(content.lines().map(|s| s.trim().to_string())),
            "technologies" => self.technologies.extend(content.split(',').map(|s| s.trim().to_string())),
            "key paths" => self.key_paths.extend(content.lines().map(|s| s.trim().to_string())),
            _ => { self.variables.insert(section.to_string(), content.to_string()); }
        }
    }

    pub fn to_prompt_section(&self) -> String {
        let mut sections = Vec::new();
        if let Some(p) = &self.project { sections.push(format!("Project: {}", p)); }
        if let Some(d) = &self.description { sections.push(format!("Description: {}", d)); }
        if !self.technologies.is_empty() { sections.push(format!("Technologies: {}", self.technologies.join(", "))); }
        if !self.key_paths.is_empty() { sections.push(format!("Key Paths:\n{}", self.key_paths.iter().map(|p| format!("  - {}", p)).collect::<Vec<_>>().join("\n"))); }
        if !self.conventions.is_empty() { sections.push(format!("Conventions:\n{}", self.conventions.iter().map(|c| format!("  - {}", c)).collect::<Vec<_>>().join("\n"))); }
        if !self.instructions.is_empty() { sections.push(format!("Instructions:\n{}", self.instructions.iter().map(|i| format!("  - {}", i)).collect::<Vec<_>>().join("\n"))); }
        sections.join("\n\n")
    }
}

/// Context file manager
pub struct ContextManager {
    contexts: Vec<ContextFile>,
    search_paths: Vec<PathBuf>,
}

impl ContextManager {
    pub fn new() -> Self {
        Self { contexts: Vec::new(), search_paths: vec![PathBuf::from(".")] }
    }

    pub fn discover(&mut self) -> Result<usize> {
        self.contexts.clear();
        for path in &self.search_paths {
            let agents_md = path.join("AGENTS.md");
            if agents_md.exists() {
                match ContextFile::load(&agents_md) {
                    Ok(ctx) => { info!("Loaded context: {}", agents_md.display()); self.contexts.push(ctx); }
                    Err(e) => warn!("Failed to load {}: {}", agents_md.display(), e),
                }
            }
        }
        Ok(self.contexts.len())
    }

    pub fn to_prompt_section(&self) -> String {
        if self.contexts.is_empty() { return String::new(); }
        let contexts: Vec<String> = self.contexts.iter().map(|c| c.to_prompt_section()).filter(|s| !s.is_empty()).collect();
        if contexts.is_empty() { return String::new(); }
        format!("## Project Context\n\n{}\n", contexts.join("\n\n---\n\n"))
    }

    pub fn has_context(&self) -> bool { !self.contexts.is_empty() }
}

impl Default for ContextManager {
    fn default() -> Self { Self::new() }
}
