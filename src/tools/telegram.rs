//! Telegram Bot Tool - Connect with users via Telegram
//!
//! This tool allows the agent to:
//! - Send messages to Telegram chats
//! - Receive and respond to messages
//! - Run as an interactive bot service

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tokio::sync::mpsc;
use std::collections::HashMap;

/// Sanitize agent output to remove internal tool call JSON
/// This prevents raw tool calls from leaking to users
fn sanitize_for_user(text: &str) -> String {
    use regex::Regex;
    
    let mut result = text.to_string();
    
    // Remove JSON tool call blocks like {"name": "tool_name", "arguments": {...}}
    if let Ok(re) = Regex::new(r#"\{\s*"name"\s*:\s*"[^"]+"\s*,\s*"arguments"\s*:\s*\{[^}]*\}\s*\}"#) {
        result = re.replace_all(&result, "").to_string();
    }
    
    // Remove XML tool call tags
    if let Ok(re) = Regex::new(r#"<tool_call>.*?</tool_call>"#) {
        result = re.replace_all(&result, "").to_string();
    }
    
    // Remove any remaining standalone JSON objects that look like tool calls
    if let Ok(re) = Regex::new(r#"\{[^{}]*"name"[^{}]*\}"#) {
        result = re.replace_all(&result, "").to_string();
    }
    
    // Clean up extra whitespace and empty lines
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

use crate::gateway::{Gateway, parse_agent_response, sanitize_agent_output, split_message};

/// Telegram implementation of the Gateway trait
pub struct TelegramGateway {
    bot: Bot,
    chat_id: ChatId,
}

#[async_trait]
impl Gateway for TelegramGateway {
    async fn send_text(&self, _chat_id: &str, text: &str) -> anyhow::Result<()> {
        // Telegram limit: 4096 chars
        for chunk in split_message(text, 4000) {
            self.bot.send_message(self.chat_id, &chunk).await?;
            // Small delay to avoid flood control
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        Ok(())
    }

    async fn send_image(&self, _chat_id: &str, file_path: &str) -> anyhow::Result<()> {
        self.bot.send_photo(
            self.chat_id,
            InputFile::file(file_path)
        ).await?;
        Ok(())
    }
}

/// Send agent response to Telegram, handling both text and images
/// This function uses the Gateway trait implementation
pub async fn send_agent_response(
    bot: &Bot,
    chat_id: ChatId,
    response: &str,
) -> anyhow::Result<()> {
    let gateway = TelegramGateway { 
        bot: bot.clone(), 
        chat_id 
    };
    gateway.send_response(&chat_id.0.to_string(), response).await
}

/// Message from Telegram user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub chat_id: i64,
    pub username: Option<String>,
    pub text: String,
    pub message_id: i32,
}

/// Telegram bot manager
pub struct TelegramBot {
    bot: Option<Bot>,
    pending_messages: Arc<Mutex<Vec<TelegramMessage>>>,
    message_sender: Option<mpsc::UnboundedSender<TelegramMessage>>,
}

impl TelegramBot {
    pub fn new() -> Self {
        Self {
            bot: None,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            message_sender: None,
        }
    }

    fn get_token_from_env() -> Option<String> {
        std::env::var("TELEGRAM_BOT_TOKEN").ok()
    }

    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<String> {
        if let Some(ref bot) = self.bot {
            // Split long messages (Telegram limit is 4096 chars)
            const MAX_LEN: usize = 4000;
            
            if text.len() <= MAX_LEN {
                bot.send_message(ChatId(chat_id), text).await?;
            } else {
                // Split into chunks
                for chunk in text.chars().collect::<Vec<_>>().chunks(MAX_LEN) {
                    let chunk_str: String = chunk.iter().collect();
                    bot.send_message(ChatId(chat_id), &chunk_str).await?;
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
            Ok(format!("Message sent to chat {}", chat_id))
        } else {
            Err(anyhow::anyhow!("Bot not initialized. Set TELEGRAM_BOT_TOKEN environment variable."))
        }
    }

    async fn start_bot(&mut self) -> anyhow::Result<String> {
        let token = Self::get_token_from_env()
            .ok_or_else(|| anyhow::anyhow!("TELEGRAM_BOT_TOKEN not set"))?;

        let bot = Bot::new(token);
        
        // Test the bot by getting me
        let me = bot.get_me().await?;
        let bot_name = me.username.clone().unwrap_or_default();
        
        self.bot = Some(bot.clone());

        // Setup message channel
        let (tx, _rx) = mpsc::unbounded_channel::<TelegramMessage>();
        self.message_sender = Some(tx.clone());

        // Start the bot in background
        let pending_messages = self.pending_messages.clone();
        
        tokio::spawn(async move {
            let handler = dptree::entry()
                .branch(Update::filter_message().endpoint(
                    move |bot: Bot, msg: Message| {
                        let tx = tx.clone();
                        let pending = pending_messages.clone();
                        
                        async move {
                            if let Some(text) = msg.text() {
                                let tg_msg = TelegramMessage {
                                    chat_id: msg.chat.id.0,
                                    username: msg.from().and_then(|u| u.username.clone()),
                                    text: text.to_string(),
                                    message_id: msg.id.0,
                                };
                                
                                // Store in pending messages
                                let mut queue = pending.lock().await;
                                queue.push(tg_msg.clone());
                                // Keep only last 100 messages
                                if queue.len() > 100 {
                                    queue.remove(0);
                                }
                                drop(queue);
                                
                                // Send to channel
                                let _ = tx.send(tg_msg);
                                
                                // Send acknowledgment
                                let _ = bot.send_message(
                                    msg.chat.id, 
                                    "🤔 Processing your request..."
                                ).await;
                            }
                            Ok::<(), teloxide::errors::RequestError>(())
                        }
                    }
                ));

            Dispatcher::builder(bot, handler)
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        });

        Ok(format!(
            "✅ Telegram bot '@{}' started!\n\nUsers can now send messages to the bot.",
            bot_name
        ))
    }

    async fn get_pending_messages(&self) -> Vec<TelegramMessage> {
        let mut queue = self.pending_messages.lock().await;
        let messages = queue.clone();
        queue.clear();
        messages
    }

    fn stop_bot(&mut self) -> String {
        self.bot = None;
        self.message_sender = None;
        "Telegram bot stopped".to_string()
    }
}

pub struct TelegramTool {
    bot: Arc<Mutex<TelegramBot>>,
}

impl TelegramTool {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(Mutex::new(TelegramBot::new())),
        }
    }
}

#[async_trait]
impl Tool for TelegramTool {
    fn name(&self) -> &str {
        "telegram"
    }

    fn description(&self) -> &str {
        "Send and receive messages via Telegram bot. \
         Use this to communicate with users through Telegram. \
         Operations: start_bot, stop_bot, send_message, get_messages"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["start_bot", "stop_bot", "send_message", "get_messages"],
                    "description": "The Telegram operation to perform"
                },
                "chat_id": {
                    "type": "integer",
                    "description": "Telegram chat ID (for send_message)",
                    "optional": true
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send (for send_message)",
                    "optional": true
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let operation = args["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: operation"))?;

        let mut bot = self.bot.lock().await;

        let result = match operation {
            "start_bot" => match bot.start_bot().await {
                Ok(msg) => ToolResult::success(msg),
                Err(e) => ToolResult::error(format!("Failed to start bot: {}", e)),
            },
            "stop_bot" => {
                let msg = bot.stop_bot();
                ToolResult::success(msg)
            }
            "send_message" => {
                let chat_id = args["chat_id"].as_i64()
                    .ok_or_else(|| anyhow::anyhow!("Missing chat_id"))?;
                let message = args["message"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing message"))?;
                
                match bot.send_message(chat_id, message).await {
                    Ok(msg) => ToolResult::success(msg),
                    Err(e) => ToolResult::error(format!("Failed to send message: {}", e)),
                }
            }
            "get_messages" => {
                let messages = bot.get_pending_messages().await;
                if messages.is_empty() {
                    ToolResult::success("No new messages".to_string())
                } else {
                    let formatted: Vec<String> = messages.iter()
                        .map(|m| format!(
                            "From: {} (chat: {})\nMessage: {}\n---",
                            m.username.as_deref().unwrap_or("Unknown"),
                            m.chat_id,
                            m.text
                        ))
                        .collect();
                    ToolResult::success(formatted.join("\n\n"))
                }
            }
            _ => ToolResult::error(format!("Unknown operation: {}", operation)),
        };

        Ok(result)
    }
}

/// Skill for creating a persistent Telegram bot that processes messages through the agent
pub struct TelegramAgentBot {
    agent_config: crate::agent::AgentConfig,
    agents: Arc<Mutex<HashMap<i64, crate::agent::Agent>>>,
}

impl TelegramAgentBot {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        let config = crate::agent::AgentConfig::new(db_path);
        Self {
            agent_config: config,
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN not set"))?;

        let bot = Bot::new(token);
        let me = bot.get_me().await?;
        
        println!("🤖 Telegram Agent Bot started!");
        println!("Bot: @{}", me.username.clone().unwrap_or_default());
        println!("Waiting for messages...\n");

        let db_path = self.agent_config.db_path.clone();
        let agents = self.agents.clone();

        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(
                move |bot: Bot, msg: Message| {
                    let db_path = db_path.clone();
                    let agents = agents.clone();
                    
                    async move {
                        if let Some(text) = msg.text() {
                            let username = msg.from().and_then(|u| u.username.clone())
                                .unwrap_or_else(|| "Unknown".to_string());
                            println!("📩 Message from {}: {}", username, text);

                            let chat_id = msg.chat.id;
                            
                            // Send status message that will be deleted later
                            let status_msg = match bot.send_message(
                                chat_id, 
                                "🤔 Thinking..."
                            ).await {
                                Ok(msg) => Some(msg.id),
                                Err(_) => None,
                            };
                            
                            // Send typing indicator to show "typing..." under bot name
                            let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;
                            
                            // Get or create agent for this chat (persistent memory per chat)
                            let mut agents_lock = agents.lock().await;
                            let agent = agents_lock.entry(chat_id.0).or_insert_with(|| {
                                let config = crate::agent::AgentConfig::new(db_path.clone());
                                crate::agent::Agent::new(config).expect("Failed to create agent")
                            });
                            
                            // Spawn a task to keep sending typing indicator every 3 seconds
                            let typing_bot = bot.clone();
                            let typing_chat_id = chat_id;
                            let typing_handle = tokio::spawn(async move {
                                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));
                                loop {
                                    interval.tick().await;
                                    let _ = typing_bot.send_chat_action(
                                        typing_chat_id, 
                                        teloxide::types::ChatAction::Typing
                                    ).await;
                                }
                            });
                            
                            // Run the agent
                            let response = agent.run(text).await;
                            
                            // Stop the typing indicator task
                            typing_handle.abort();
                            drop(agents_lock); // Release lock before async operations
                            
                            // Delete status message before sending response
                            if let Some(status_id) = status_msg {
                                let _ = bot.delete_message(chat_id, status_id).await;
                            }
                            
                            match response {
                                Ok(response_text) => {
                                    println!("📝 Raw agent response ({} chars): {:?}", response_text.len(), &response_text[..response_text.len().min(200)]);
                                    
                                    // Sanitize to remove any leaked tool call JSON
                                    let response_text = sanitize_for_user(&response_text);
                                    println!("📝 Sanitized response ({} chars)", response_text.len());
                                    
                                    // Skip empty responses
                                    if response_text.trim().is_empty() {
                                        eprintln!("❌ Empty response after sanitization!");
                                        let _ = bot.send_message(chat_id, "🤔 I processed your request but didn't generate a response. Please try again.").await;
                                        return Ok(());
                                    }
                                    
                                    // Send response with images using the shared function
                                    if let Err(e) = send_agent_response(&bot, chat_id, &response_text).await {
                                        eprintln!("❌ Failed to send response: {}", e);
                                    }
                                }
                                Err(e) => {
                                    let _ = bot.send_message(
                                        chat_id, 
                                        format!("❌ Error: {}", e)
                                    ).await;
                                }
                            }
                        }
                        Ok::<(), teloxide::errors::RequestError>(())
                    }
                }
            ));

        Dispatcher::builder(bot, handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }
}
