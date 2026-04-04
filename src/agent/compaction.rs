//! Conversation Compaction - Reduce token usage by summarizing history
//!
//! This module provides intelligent conversation compaction:
//! 1. Summarize old conversation turns into condensed form
//! 2. Extract key facts to long-term memory
//! 3. Manage context window size

use super::llm::{ChatMessage, LlmClient};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Configuration for compaction behavior
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum messages before triggering compaction
    pub max_messages: usize,
    /// Target messages after compaction
    pub target_messages: usize,
    /// Enable fact extraction
    pub extract_facts: bool,
    /// Max token estimate before compaction (rough estimate)
    pub max_tokens_estimate: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_messages: 20,
            target_messages: 8,
            extract_facts: true,
            max_tokens_estimate: 6000,
        }
    }
}

/// Compacted conversation segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedSegment {
    pub original_turns: usize,
    pub summary: String,
    pub key_facts: Vec<String>,
    pub timestamp: String,
}

/// Long-term fact memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactMemory {
    pub facts: Vec<Fact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub content: String,
    pub source_session: String,
    pub created_at: String,
    pub importance: f32, // 0.0 to 1.0
    pub category: String, // "user_preference", "technical", "task", etc.
}

/// Manages conversation compaction
pub struct CompactionManager {
    config: CompactionConfig,
    llm: Option<LlmClient>,
    fact_memory: Vec<Fact>,
    compacted_segments: Vec<CompactedSegment>,
}

impl CompactionManager {
    pub fn new(config: CompactionConfig) -> Self {
        Self {
            config,
            llm: None,
            fact_memory: Vec::new(),
            compacted_segments: Vec::new(),
        }
    }

    pub fn with_llm(mut self, llm: LlmClient) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Check if compaction is needed
    pub fn needs_compaction(&self, messages: &[ChatMessage]) -> bool {
        if messages.len() > self.config.max_messages {
            return true;
        }
        
        // Rough token estimation (4 chars ~= 1 token)
        let total_chars: usize = messages.iter()
            .map(|m| m.content.len())
            .sum();
        let estimated_tokens = total_chars / 4;
        
        estimated_tokens > self.config.max_tokens_estimate
    }

    /// Compact messages, keeping recent ones intact
    pub async fn compact(&mut self, messages: &[ChatMessage]) -> Result<Vec<ChatMessage>> {
        if messages.len() <= self.config.target_messages {
            return Ok(messages.to_vec());
        }

        // Split into old (to compact) and recent (keep as-is)
        let split_point = messages.len() - (self.config.target_messages / 2);
        let old_messages = &messages[..split_point];
        let recent_messages = &messages[split_point..];

        // Generate summary of old messages
        let summary = if let Some(ref llm) = self.llm {
            self.generate_summary(llm, old_messages).await?
        } else {
            self.simple_summarize(old_messages)
        };

        // Extract key facts if enabled
        let key_facts = if self.config.extract_facts {
            if let Some(ref llm) = self.llm {
                self.extract_key_facts(llm, old_messages).await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Store compacted segment
        let segment = CompactedSegment {
            original_turns: old_messages.len(),
            summary: summary.clone(),
            key_facts: key_facts.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.compacted_segments.push(segment);

        // Add facts to memory
        for fact_content in &key_facts {
            let fact = Fact {
                content: fact_content.clone(),
                source_session: String::new(), // Would be set from context
                created_at: chrono::Utc::now().to_rfc3339(),
                importance: 0.8,
                category: "general".to_string(),
            };
            self.fact_memory.push(fact);
        }

        // Build compacted message list
        let mut result = Vec::new();
        
        // Add summary as system context
        if !summary.is_empty() {
            result.push(ChatMessage::system(format!(
                "Previous conversation summary: {}\n\nKey points from earlier:\n- {}",
                summary,
                key_facts.join("\n- ")
            )));
        }

        // Add recent messages unchanged
        result.extend_from_slice(recent_messages);

        Ok(result)
    }

    /// Generate intelligent summary using LLM
    async fn generate_summary(&self, llm: &LlmClient, messages: &[ChatMessage]) -> Result<String> {
        let conversation_text = messages.iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Summarize the following conversation concisely. Focus on:\n\
            1. What the user asked for\n\
            2. What actions were taken\n\
            3. What was the outcome\n\
            Keep it under 100 words.\n\n\
            Conversation:\n{}",
            conversation_text
        );

        let summary = llm.chat_simple(
            "You are a helpful assistant that summarizes conversations.",
            &prompt
        ).await?;

        Ok(summary.trim().to_string())
    }

    /// Simple rule-based summarization (no LLM needed)
    fn simple_summarize(&self, messages: &[ChatMessage]) -> String {
        let user_msgs: Vec<_> = messages.iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.clone())
            .collect();

        let tool_calls: Vec<_> = messages.iter()
            .filter(|m| m.role == "assistant")
            .filter_map(|m| m.tool_calls.as_ref())
            .flatten()
            .map(|tc| tc.function.name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let mut summary = String::new();
        
        if !user_msgs.is_empty() {
            summary.push_str(&format!(
                "User made {} requests about: {}. ",
                user_msgs.len(),
                Self::extract_topics(&user_msgs)
            ));
        }

        if !tool_calls.is_empty() {
            summary.push_str(&format!(
                "Tools used: {}.",
                tool_calls.join(", ")
            ));
        }

        summary
    }

    /// Extract key facts using LLM
    async fn extract_key_facts(&self, llm: &LlmClient, messages: &[ChatMessage]) -> Result<Vec<String>> {
        let conversation_text = messages.iter()
            .map(|m| format!("{}: {}", m.role, m.content.chars().take(500).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Extract 3-5 key facts from this conversation that would be useful to remember for future context. \
            Focus on: user preferences, important data, decisions made, or technical details. \
            Return as a JSON array of strings.\n\n\
            Conversation:\n{}\n\n\
            Respond ONLY with a JSON array like: [\"fact 1\", \"fact 2\", \"fact 3\"]",
            conversation_text
        );

        let response = llm.chat_simple(
            "You are a helpful assistant that extracts key facts.",
            &prompt
        ).await?;

        // Try to parse JSON array
        let facts: Vec<String> = serde_json::from_str(&response)
            .unwrap_or_else(|_| {
                // Fallback: extract lines that look like facts
                response
                    .lines()
                    .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('[') && !l.trim().starts_with(']'))
                    .map(|l| l.trim().trim_start_matches("- ").trim_matches('"').to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            });

        Ok(facts.into_iter().take(5).collect())
    }

    /// Extract topics from user messages
    fn extract_topics(messages: &[String]) -> String {
        // Simple keyword extraction
        let all_text = messages.join(" ").to_lowercase();
        
        let keywords: Vec<_> = [
            "file", "search", "code", "write", "read", "run", "execute",
            "build", "test", "debug", "install", "configure", "setup",
            "error", "bug", "fix", "issue", "problem",
            "create", "delete", "update", "modify", "change",
        ].iter()
            .filter(|&&kw| all_text.contains(kw))
            .take(3)
            .map(|&s| s.to_string())
            .collect();

        if keywords.is_empty() {
            "various topics".to_string()
        } else {
            keywords.join(", ")
        }
    }

    /// Get relevant facts for a query
    pub fn get_relevant_facts(&self, query: &str) -> Vec<&Fact> {
        let query_lower = query.to_lowercase();
        
        self.fact_memory
            .iter()
            .filter(|f| {
                let fact_lower = f.content.to_lowercase();
                // Simple relevance: shared words
                query_lower.split_whitespace()
                    .filter(|w| w.len() > 3)
                    .any(|word| fact_lower.contains(word))
            })
            .take(3)
            .collect()
    }

    /// Get compaction statistics
    pub fn stats(&self) -> CompactionStats {
        CompactionStats {
            segments_compacted: self.compacted_segments.len(),
            total_turns_compacted: self.compacted_segments.iter().map(|s| s.original_turns).sum(),
            facts_extracted: self.fact_memory.len(),
        }
    }

    /// Clear all compaction data
    pub fn clear(&mut self) {
        self.compacted_segments.clear();
        self.fact_memory.clear();
    }
}

/// Statistics about compaction
#[derive(Debug, Clone)]
pub struct CompactionStats {
    pub segments_compacted: usize,
    pub total_turns_compacted: usize,
    pub facts_extracted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_summarize() {
        let manager = CompactionManager::new(CompactionConfig::default());
        
        let messages = vec![
            ChatMessage::user("Search for files about rust"),
            ChatMessage::assistant("I found several files."),
            ChatMessage::user("Read the first one"),
        ];

        let summary = manager.simple_summarize(&messages);
        assert!(!summary.is_empty());
    }

    #[test]
    fn test_extract_topics() {
        let msgs = vec![
            "How do I search for files?".to_string(),
            "I want to write some code".to_string(),
        ];
        
        let topics = CompactionManager::extract_topics(&msgs);
        assert!(topics.contains("file") || topics.contains("code"));
    }
}
