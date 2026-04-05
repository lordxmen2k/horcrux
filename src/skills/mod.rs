//! Skills System - Learned behaviors and task patterns
//!
//! Skills are Markdown files that encode successful task execution patterns.
//! They provide both positive guidance (what TO do) and negative constraints
//! (what NOT to do) learned from experience.

use std::path::PathBuf;
use anyhow::Result;

/// A loaded skill with metadata
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub uses: u32,
}

/// Manages skill discovery, loading, and creation
pub struct SkillsManager {
    skills_dir: PathBuf,
}

impl SkillsManager {
    pub fn new() -> Self {
        // Try project directory first (for development)
        let project_dir = PathBuf::from(".").join("skills");
        if project_dir.exists() {
            return Self { skills_dir: project_dir };
        }
        
        // Fall back to config directory
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("horcrux")
            .join("skills");
        std::fs::create_dir_all(&dir).ok();
        Self { skills_dir: dir }
    }

    /// List all available skills with names and descriptions
    /// Used for system prompt injection
    pub fn list_skills(&self) -> Vec<(String, String)> {
        let Ok(entries) = std::fs::read_dir(&self.skills_dir) else {
            return vec![];
        };

        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .filter_map(|e| {
                let content = std::fs::read_to_string(e.path()).ok()?;
                let name = e.path()
                    .file_stem()?
                    .to_string_lossy()
                    .to_string();
                let description = extract_frontmatter_field(&content, "description")?;
                Some((name, description))
            })
            .collect()
    }

    /// Load full skill content by name
    /// Also increments the uses counter
    pub fn load_skill(&self, name: &str) -> Option<Skill> {
        let path = self.skills_dir.join(format!("{}.md", name));
        let content = std::fs::read_to_string(&path).ok()?;
        
        let name = extract_frontmatter_field(&content, "name")
            .unwrap_or_else(|| name.to_string());
        let description = extract_frontmatter_field(&content, "description")?;
        let uses = extract_frontmatter_field(&content, "uses")
            .and_then(|u| u.parse().ok())
            .unwrap_or(0);
        
        // Increment uses
        let _ = self.increment_uses(&path, &content);
        
        Some(Skill {
            name,
            description,
            content,
            uses,
        })
    }

    /// Check if query indicates purchase/shopping intent
    fn is_purchase_intent(&self, query: &str) -> bool {
        let lower = query.to_lowercase();
        let purchase_verbs = ["buy", "purchase", "find me", "recommend", 
                              "best", "i need", "looking for", "which", "suggest"];
        let product_nouns = ["computer", "laptop", "phone", "tablet", 
                             "monitor", "keyboard", "headphone", "camera"];
        
        let has_purchase_verb = purchase_verbs.iter().any(|v| lower.contains(v));
        let has_product = product_nouns.iter().any(|p| lower.contains(p));
        let no_visual_words = !lower.contains("picture") && !lower.contains("photo") 
            && !lower.contains("image") && !lower.contains("show me");
        
        let is_match = has_product && (has_purchase_verb || no_visual_words);
        
        println!("🔍 Purchase intent check: '{}' -> verbs={}, product={}, no_visual={}, MATCH={}", 
                 query, has_purchase_verb, has_product, no_visual_words, is_match);
        
        is_match
    }

    /// Find the most relevant skill for a given query
    /// Matches based on description keywords
    pub fn find_relevant_skill(&self, query: &str) -> Option<Skill> {
        let query_lower = query.to_lowercase();
        let skills = self.list_skills();

        // Special case: purchase intent should match find-product-recommendations
        if self.is_purchase_intent(query) {
            if let Some(skill) = self.load_skill("find-product-recommendations") {
                return Some(skill);
            }
        }

        // Score each skill by keyword matches in description
        let mut best_match: Option<(Skill, usize)> = None;

        for (name, description) in &skills {
            let desc_lower = description.to_lowercase();
            
            // Extract meaningful words from query (>3 chars)
            let words: Vec<&str> = query_lower
                .split_whitespace()
                .filter(|w| w.len() > 3)
                .collect();

            // Count matching words
            let match_score = words.iter()
                .filter(|w| desc_lower.contains(*w))
                .count();

            // Need at least 2 matches to be relevant
            if match_score >= 2 {
                if let Some(skill) = self.load_skill(name) {
                    if best_match.as_ref().map(|(_, s)| match_score > *s).unwrap_or(true) {
                        best_match = Some((skill, match_score));
                    }
                }
            }
        }

        best_match.map(|(skill, _)| skill)
    }

    /// Save a new or updated skill
    pub fn save_skill(&self, name: &str, content: &str) -> Result<()> {
        let path = self.skills_dir.join(format!("{}.md", name));
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Check if a skill with similar name/content already exists
    pub fn similar_skill_exists(&self, query: &str) -> bool {
        self.find_relevant_skill(query).is_some()
    }

    fn increment_uses(&self, path: &PathBuf, content: &str) -> Result<()> {
        let updated = content.lines().map(|line| {
            if line.starts_with("uses: ") {
                let n: u32 = line["uses: ".len()..].trim().parse().unwrap_or(0);
                format!("uses: {}", n + 1)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
        
        std::fs::write(path, updated)?;
        Ok(())
    }
}

fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let prefix = format!("{}: ", field);
    content.lines()
        .find(|l| l.starts_with(&prefix))
        .map(|l| l[prefix.len()..].trim().to_string())
}

/// Evaluates whether a completed task should become a skill
pub struct SkillCreationEvaluator;

impl SkillCreationEvaluator {
    pub fn should_create_skill(
        tool_calls_made: u32,
        task_succeeded: bool,
        skill_already_exists: bool,
    ) -> bool {
        task_succeeded
            && tool_calls_made >= 3       // Complex enough to be worth saving
            && !skill_already_exists      // Don't duplicate
    }
}

/// Build skills section for system prompt
pub fn build_skills_section(skills: &[(String, String)]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let entries = skills.iter()
        .map(|(name, desc)| format!("- {}: {}", name, desc))
        .collect::<Vec<_>>()
        .join("\n");

    format!(r#"
## Your Skills (Learned Capabilities)
You have learned these skills from previous successful tasks:
{}

When a user's request matches a skill:
1. Follow the skill's Procedure exactly
2. Respect the "What NOT to Do" section
3. The skill encodes proven success - trust it over general knowledge
"#, entries)
}

/// Build prompt for skill creation after successful task
pub fn build_skill_creation_prompt(
    original_query: &str,
    tool_calls_summary: &str,
    final_response_preview: &str,
) -> String {
    format!(r#"You just successfully completed this task: "{}"

What you did:
{}

Your response summary:
{}

Create a skill file to remember this approach for similar future tasks.

The skill should include:
1. Triggers - what user queries should activate this skill
2. Procedure - step by step what to do
3. What NOT to Do - common mistakes or wrong approaches
4. Example - the query and correct handling

Format:
---
name: descriptive-kebab-name
description: One line describing when to use this (for matching)
version: 1.0.0
created: {}
uses: 0
last_used: {}
---

# Skill Name

## Triggers
List patterns that should trigger this skill

## Procedure
Step by step instructions

## What NOT to Do
Specific prohibitions (e.g., "Do NOT use image_search for purchase queries")

## Example
User: "..."
→ Your approach: ...
"#, 
        original_query,
        tool_calls_summary,
        &final_response_preview[..final_response_preview.len().min(200)],
        chrono::Local::now().format("%Y-%m-%d"),
        chrono::Local::now().format("%Y-%m-%d")
    )
}

/// Default skills to create on first run
pub const DEFAULT_SKILLS: &[(&str, &str)] = &[
    ("find-product-recommendations", r#"---
name: find-product-recommendations
description: User wants to find products to buy, compare, shop for, or get recommendations (computer, laptop, phone, electronics, purchase, buy, find me, recommend, best)
version: 1.3.0
created: 2024-01-15
uses: 0
last_used: 2026-04-05
---

# Find Product Recommendations

## Triggers
- "find me a computer/laptop/phone" (with OR without "to purchase")
- "recommend me a X" / "recommend a X"
- "best X under $Y" / "best X to buy"
- "I need a X" / "looking for a X"
- "which X should I buy" / "what X do you suggest"
- "compare X and Y"
- "buying guide for X"

## CRITICAL INSTRUCTIONS - FOLLOW EXACTLY

### Step 1: Use ONLY web_search tool
**NEVER use search_knowledge** - it only has old 2024 data.  
**ALWAYS use web_search** - it gets live 2026 data from the internet.

### Step 2: Use CURRENT YEAR 2026 in ALL queries
**WRONG**: "best laptops 2024"  
**RIGHT**: "best laptops 2026"

**WRONG**: "laptop buying guide" (no year)  
**RIGHT**: "laptop buying guide 2026"

### Step 3: Execute these exact searches:
1. web_search("best {product} 2026 buying guide")
2. web_search("{product} reviews 2026")
3. web_search("{product} price comparison April 2026")

### Step 4: Summarize results with 2026 pricing and models

## What NOT to Do
❌ **NEVER use http tool** - it doesn't work for web searches  
❌ **NEVER use shell tool** - don't run commands for this task  
❌ **NEVER use search_knowledge** - has ONLY old cached data  
❌ **NEVER use image_search** - this is shopping research, not pictures  
❌ "2024" - use 2026 in ALL queries  
❌ Generic queries without year - always include 2026  

## AVAILABLE TOOLS
You have these tools available:
- **web_search** ← USE THIS ONE for finding current product information
- image_search (for pictures, NOT for shopping research)
- search_knowledge (old cached data, DO NOT USE for products)

**FOR PRODUCT RESEARCH: USE web_search ONLY**

## CORRECT Example
User: "find me a laptop"  
→ web_search("best laptop 2026 buying guide")  
→ web_search("laptop reviews 2026")  
→ "Here are the best 2026 laptops..."

## INCORRECT Example (DO NOT DO)
→ search_knowledge("laptops") ← WRONG TOOL  
→ "best laptops 2024" ← WRONG YEAR  
→ image_search("laptop") ← WRONG TOOL
"#),

    ("image-search", r#"---
name: image-search
description: User explicitly wants to see images, photos, or pictures of something
version: 1.0.0
created: 2024-01-15
uses: 0
last_used: 2024-01-15
---

# Image Search

## Triggers
- "show me a picture of X"
- "find me an image of X"
- "photo of X"
- "what does X look like"
- Explicit visual requests only

## Procedure
1. Extract the search subject from the query
2. Call image_search tool with the subject
3. Present the images with brief descriptions

## What NOT to Do
- Do NOT trigger on "find me X to buy" (shopping intent)
- Do NOT trigger on research or information queries
- Only trigger when user explicitly asks to SEE something

## Example
User: "show me a golden retriever"
→ image_search("golden retriever")

User: "find me a dog to adopt"
→ NOT an image search (adoption/research intent)
"#),
];

/// Initialize default skills if skills directory is empty
pub fn init_default_skills(manager: &SkillsManager) -> Result<()> {
    let existing = manager.list_skills();
    if !existing.is_empty() {
        println!("📚 Skills already exist ({} skills), skipping creation", existing.len());
        return Ok(());  // Already have skills, don't overwrite
    }

    println!("📚 Creating default skills...");
    for (name, content) in DEFAULT_SKILLS {
        manager.save_skill(name, content)?;
        println!("  ✓ Created skill: {}", name);
    }
    
    Ok(())
}
