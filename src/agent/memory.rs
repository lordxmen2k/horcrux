//! Conversation Memory - Store and retrieve conversation history

use crate::db::{ConversationMessage, Db};
use super::llm::{ChatMessage, ToolCall};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Mutex;

/// Manages conversation history for an agent session
pub struct ConversationMemory {
    db_path: PathBuf,
    session_id: String,
    cache: Mutex<Vec<ChatMessage>>,
}

impl ConversationMemory {
    pub fn new(db_path: PathBuf, session_id: String) -> Self {
        Self {
            db_path,
            session_id,
            cache: Mutex::new(Vec::new()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Load messages from database
    pub async fn load(&self) -> Result<Vec<ChatMessage>> {
        let db = Db::open(&self.db_path)?;
        let messages = db.get_conversation_history(&self.session_id, 1000)?;
        
        let chat_messages: Vec<ChatMessage> = messages
            .into_iter()
            .map(|m| self.db_message_to_chat_message(m))
            .collect();

        // Update cache
        if let Ok(mut cache) = self.cache.lock() {
            *cache = chat_messages.clone();
        }

        Ok(chat_messages)
    }

    /// Get messages (from cache or database)
    pub async fn get_messages(&self, limit: usize) -> Result<Vec<ChatMessage>> {
        // Try cache first
        if let Ok(cache) = self.cache.lock() {
            if !cache.is_empty() {
                let start = cache.len().saturating_sub(limit);
                return Ok(cache[start..].to_vec());
            }
        }

        // Load from database
        let db = Db::open(&self.db_path)?;
        let messages = db.get_conversation_history(&self.session_id, limit as usize)?;
        
        // DEBUG: Print all messages to diagnose tool_call_id issues
        println!("📋 Loading {} messages from DB:", messages.len());
        for (i, m) in messages.iter().enumerate() {
            let tc_preview = m.tool_calls.as_ref().map(|t| if t.len() > 20 { format!("{}...", &t[..20]) } else { t.clone() }).unwrap_or_else(|| "None".to_string());
            println!("  DB[{}]: role={:10} tool_calls={:25} content={:.50}...", 
                i, m.role, tc_preview, m.content);
        }
        
        let chat_messages: Vec<ChatMessage> = messages
            .into_iter()
            .filter_map(|m| {
                let cm = self.db_message_to_chat_message(m);
                // Filter out invalid tool messages (those with empty tool_call_id)
                if cm.role == "tool" {
                    let id_preview = cm.tool_call_id.as_ref().map(|s| s.as_str()).unwrap_or("None");
                    println!("  Converting tool message: call_id={}", id_preview);
                    if let Some(ref id) = cm.tool_call_id {
                        if id.is_empty() {
                            println!("⚠️ Filtering out tool message with empty tool_call_id");
                            return None;
                        }
                    } else {
                        println!("⚠️ Filtering out tool message with missing tool_call_id");
                        return None;
                    }
                }
                Some(cm)
            })
            .collect();
        
        Ok(chat_messages)
    }

    /// Add a user message
    pub async fn add_user_message(&self, content: &str) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        db.add_conversation_message(&self.session_id, "user", content, None, None)?;

        if let Ok(mut cache) = self.cache.lock() {
            cache.push(ChatMessage::user(content));
        }

        Ok(())
    }

    /// Add an assistant message (optionally with tool calls)
    pub async fn add_assistant_message(&self, content: &str, tool_calls: Option<&Vec<ToolCall>>) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        
        let tool_calls_json = tool_calls.map(|tc| serde_json::to_string(tc).unwrap_or_default());
        
        db.add_conversation_message(
            &self.session_id,
            "assistant",
            content,
            tool_calls_json.as_deref(),
            None
        )?;

        if let Ok(mut cache) = self.cache.lock() {
            if let Some(tcs) = tool_calls {
                cache.push(ChatMessage::assistant_with_tools(content, tcs.clone()));
            } else {
                cache.push(ChatMessage::assistant(content));
            }
        }

        Ok(())
    }

    /// Add a tool result
    pub async fn add_tool_result(&self, tool_call_id: &str, result: &str) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        
        // Store with tool_call_id in the tool_calls field for reference
        db.add_conversation_message(
            &self.session_id,
            "tool",
            result,
            Some(tool_call_id),
            None
        )?;

        if let Ok(mut cache) = self.cache.lock() {
            cache.push(ChatMessage::tool(tool_call_id, result));
        }

        Ok(())
    }

    /// Add a system reminder message (used to force tool usage)
    /// Uses user role with [SYSTEM NOTE] prefix to avoid breaking tool call sequences
    pub async fn add_system_reminder(&self, content: &str) -> Result<()> {
        // Don't store system reminders in database, just add to cache
        // Using user role with prefix instead of system to avoid API issues
        if let Ok(mut cache) = self.cache.lock() {
            cache.push(ChatMessage::user(format!("[SYSTEM NOTE: {}]", content)));
        }

        Ok(())
    }

    /// Get the last assistant message
    pub async fn get_last_assistant_message(&self) -> Option<String> {
        if let Ok(cache) = self.cache.lock() {
            for msg in cache.iter().rev() {
                if msg.role == "assistant" && !msg.content.is_empty() {
                    return Some(msg.content.clone());
                }
            }
        }

        // Try database
        if let Ok(db) = Db::open(&self.db_path) {
            if let Ok(messages) = db.get_conversation_history(&self.session_id, 100) {
                for msg in messages.iter().rev() {
                    if msg.role == "assistant" && !msg.content.is_empty() {
                        return Some(msg.content.clone());
                    }
                }
            }
        }

        None
    }

    /// Clear conversation history for this session
    pub async fn clear(&self) -> Result<()> {
        let db = Db::open(&self.db_path)?;
        db.clear_session(&self.session_id)?;

        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }

        Ok(())
    }

    /// Convert database message to ChatMessage
    fn db_message_to_chat_message(&self, msg: ConversationMessage) -> ChatMessage {
        match msg.role.as_str() {
            "system" => ChatMessage::system(msg.content),
            "user" => ChatMessage::user(msg.content),
            "assistant" => {
                if let Some(tool_calls_json) = msg.tool_calls {
                    if let Ok(tool_calls) = serde_json::from_str::<Vec<ToolCall>>(&tool_calls_json) {
                        return ChatMessage::assistant_with_tools(msg.content, tool_calls);
                    }
                }
                ChatMessage::assistant(msg.content)
            }
            "tool" => {
                let tool_call_id = msg.tool_calls.unwrap_or_default();
                let id_display = if tool_call_id.is_empty() { "<EMPTY>".to_string() } else { tool_call_id.clone() };
                println!("  Converting tool message: call_id='{}', content_len={}", id_display, msg.content.len());
                if tool_call_id.is_empty() {
                    // Fallback to user message if ID is empty - prevents API error
                    println!("⚠️ Converting tool message with empty ID to user note");
                    ChatMessage::user(format!("[Tool result]: {}", msg.content))
                } else {
                    ChatMessage::tool(tool_call_id, msg.content)
                }
            }
            _ => ChatMessage::user(msg.content),
        }
    }
}
