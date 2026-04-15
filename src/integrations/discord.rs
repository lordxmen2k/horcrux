//! Discord Bot Integration
//!
//! Setup:
//! 1. Create app at https://discord.com/developers/applications
//! 2. Create a bot, copy token
//! 3. Enable: Message Content Intent, Server Members Intent
//! 4. Add to config.toml:
//!    [discord]
//!    token = "your-bot-token"
//!    enabled = true

use crate::gateway::{Gateway, sanitize_agent_output};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serenity::{
    async_trait as serenity_async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    pub token: Option<String>,
    pub enabled: bool,
    /// Only respond in these channel IDs (empty = all channels)
    pub allowed_channels: Vec<u64>,
    /// Command prefix (default: none, responds to all messages)
    pub prefix: Option<String>,
}

pub struct DiscordGateway {
    http: Arc<serenity::http::Http>,
    channel_id: serenity::model::id::ChannelId,
}

#[async_trait]
impl Gateway for DiscordGateway {
    async fn send_text(&self, _chat_id: &str, text: &str) -> Result<()> {
        // Discord limit: 2000 chars
        use crate::gateway::split_message;
        for chunk in split_message(text, 1900) {
            self.channel_id.say(&self.http, &chunk).await
                .map_err(|e| anyhow::anyhow!("Discord send failed: {}", e))?;
        }
        Ok(())
    }

    async fn send_image(&self, _chat_id: &str, file_path: &str) -> Result<()> {
        let path = std::path::Path::new(file_path);
        
        // Read file and send as attachment
        let file_data = tokio::fs::read(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read image file: {}", e))?;
        let filename = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        
        let attachment = serenity::builder::CreateAttachment::bytes(file_data, filename);
        
        // Send with empty message content, just the file
        self.channel_id.send_files(
            &self.http,
            vec![attachment],
            serenity::builder::CreateMessage::new()
        ).await.map_err(|e| anyhow::anyhow!("Discord image send failed: {}", e))?;

        Ok(())
    }
}

/// Discord bot that can send messages and files
pub struct DiscordBot {
    http: Arc<serenity::http::Http>,
}

impl DiscordBot {
    pub fn new(http: Arc<serenity::http::Http>) -> Self {
        Self { http }
    }

    pub async fn send_message(&self, channel_id: u64, text: &str) -> anyhow::Result<()> {
        let channel_id = serenity::model::id::ChannelId::new(channel_id);
        for chunk in crate::gateway::split_message(text, 1900) {
            channel_id.say(&self.http, &chunk).await
                .map_err(|e| anyhow::anyhow!("Discord send failed: {}", e))?;
        }
        Ok(())
    }

    pub async fn send_file(&self, channel_id: u64, file_path: &str, caption: Option<&str>) -> anyhow::Result<String> {
        let path = std::path::Path::new(file_path);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_path));
        }
        
        let file_data = tokio::fs::read(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
        let filename = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        
        let channel_id = serenity::model::id::ChannelId::new(channel_id);
        let attachment = serenity::builder::CreateAttachment::bytes(file_data, filename);
        
        // Build message with optional caption
        let mut message_builder = serenity::builder::CreateMessage::new();
        if let Some(cap) = caption {
            message_builder = message_builder.content(cap);
        }
        
        channel_id.send_files(
            &self.http,
            vec![attachment],
            message_builder
        ).await.map_err(|e| anyhow::anyhow!("Discord file send failed: {}", e))?;

        Ok(format!("✅ File sent to Discord channel {}", channel_id))
    }
}

/// Tool interface for Discord operations
pub struct DiscordTool {
    bot: Arc<Mutex<Option<DiscordBot>>>,
}

impl DiscordTool {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(Mutex::new(None)),
        }
    }

    /// Called by DiscordHandler after it creates the bot
    pub fn inject_live_bot(bot: Arc<Mutex<Option<DiscordBot>>>) -> Self {
        Self { bot }
    }
}

#[async_trait]
impl crate::tools::Tool for DiscordTool {
    fn name(&self) -> &str {
        "discord"
    }

    fn description(&self) -> &str {
        "Send messages and files via Discord bot. \
         Use this to communicate with users through Discord. \
         Can send text messages and files. \
         Operations: send_message, send_file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["send_message", "send_file"],
                    "description": "The Discord operation to perform"
                },
                "channel_id": {
                    "type": "integer",
                    "description": "Discord channel ID (for send_message, send_file)",
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send (for send_message)",
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to file to send (for send_file)",
                },
                "caption": {
                    "type": "string",
                    "description": "Optional caption for file (for send_file)",
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<crate::tools::ToolResult> {
        let operation = args["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing operation"))?;

        let bot_guard = self.bot.lock().await;
        
        match operation {
            "send_message" => {
                let channel_id = args["channel_id"].as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing channel_id"))?;
                let message = args["message"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

                if let Some(ref bot) = *bot_guard {
                    match bot.send_message(channel_id, message).await {
                        Ok(_) => Ok(crate::tools::ToolResult::success(format!("✅ Message sent to channel {}", channel_id))),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send message: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Discord bot not initialized".to_string()))
                }
            }
            "send_file" => {
                let channel_id = args["channel_id"].as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing channel_id"))?;
                let file_path = args["file_path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
                let caption = args["caption"].as_str();

                if let Some(ref bot) = *bot_guard {
                    match bot.send_file(channel_id, file_path, caption).await {
                        Ok(msg) => Ok(crate::tools::ToolResult::success(msg)),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send file: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Discord bot not initialized".to_string()))
                }
            }
            _ => Ok(crate::tools::ToolResult::error(format!("Unknown operation: {}", operation))),
        }
    }
}

struct DiscordHandler {
    agents: Arc<Mutex<HashMap<u64, crate::agent::Agent>>>,
    db_path: std::path::PathBuf,
    config: DiscordConfig,
    shared_bot: Arc<Mutex<Option<DiscordBot>>>,
}

#[serenity_async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("✅ Discord bot connected as: {}", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore bot messages
        if msg.author.bot { return; }

        // Check prefix if configured
        let text = if let Some(ref prefix) = self.config.prefix {
            if !msg.content.starts_with(prefix.as_str()) { return; }
            msg.content[prefix.len()..].trim().to_string()
        } else {
            msg.content.clone()
        };

        // Check allowed channels
        if !self.config.allowed_channels.is_empty()
            && !self.config.allowed_channels.contains(&msg.channel_id.get())
        {
            return;
        }

        let channel_id = msg.channel_id;
        let user_id = msg.author.id.get();
        let username = msg.author.name.clone();

        println!("📩 Discord message from {}: {}", username, text);

        // Show typing indicator
        let _ = channel_id.broadcast_typing(&ctx.http).await;

        // Inject the live bot
        let bot = DiscordBot::new(ctx.http.clone());
        *self.shared_bot.lock().await = Some(bot);

        // Get or create persistent agent
        let mut agents_map = self.agents.lock().await;
        let agent = agents_map
            .entry(user_id)
            .or_insert_with(|| {
                let config = crate::agent::AgentConfig::new(self.db_path.clone())
                    .with_session_id(format!("discord_{}", user_id));
                // Create agent with Discord tool injected
                let discord_tool = DiscordTool::inject_live_bot(self.shared_bot.clone());
                crate::agent::Agent::new_with_discord(config, discord_tool)
                    .expect("Failed to create agent")
            });

        // Run with context injection
        let mut context = std::collections::HashMap::new();
        context.insert("platform".to_string(), "discord".to_string());
        context.insert("channel_id".to_string(), channel_id.get().to_string());
        context.insert("user_id".to_string(), user_id.to_string());
        context.insert("username".to_string(), username);

        match agent.run_with_context(&text, context).await {
            Ok(response) => {
                let sanitized = sanitize_agent_output(&response);
                if sanitized.trim().is_empty() {
                    let _ = channel_id.say(&ctx.http,
                        "I processed your request but couldn't generate a response."
                    ).await;
                    return;
                }

                let gateway = DiscordGateway {
                    http: ctx.http.clone(),
                    channel_id,
                };

                drop(agents_map); // release lock before sending

                if let Err(e) = gateway.send_response(&channel_id.get().to_string(), &sanitized).await {
                    eprintln!("❌ Discord response failed: {}", e);
                }
            }
            Err(e) => {
                drop(agents_map);
                let _ = channel_id.say(&ctx.http, format!("❌ Error: {}", e)).await;
            }
        }
    }
}

/// Discord bot integration handle
pub struct DiscordIntegration {
    // We keep the client running in a background task
    _handle: tokio::task::JoinHandle<()>,
}

impl DiscordIntegration {
    pub async fn new(
        config: DiscordConfig,
        db_path: std::path::PathBuf,
    ) -> Result<Self> {
        let token = config.token.clone()
            .ok_or_else(|| anyhow::anyhow!("Discord token not configured"))?;

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let handler = DiscordHandler {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
            shared_bot: Arc::new(Mutex::new(None)),
        };

        let mut client = Client::builder(token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| anyhow::anyhow!("Discord client failed: {}", e))?;

        // Start the client in background
        let handle = tokio::spawn(async move {
            if let Err(e) = client.start().await {
                eprintln!("❌ Discord client error: {}", e);
            }
        });

        println!("🤖 Discord integration started");
        Ok(Self { _handle: handle })
    }
}
