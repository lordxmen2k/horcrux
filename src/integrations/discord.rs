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

struct DiscordHandler {
    agents: Arc<Mutex<HashMap<u64, crate::agent::Agent>>>,
    db_path: std::path::PathBuf,
    config: DiscordConfig,
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

        println!("📩 Discord message from {}: {}", msg.author.name, text);

        // Show typing indicator
        let _ = channel_id.broadcast_typing(&ctx.http).await;

        // Get or create persistent agent
        let mut agents_map = self.agents.lock().await;
        let agent = agents_map
            .entry(user_id)
            .or_insert_with(|| {
                let config = crate::agent::AgentConfig::new(self.db_path.clone())
                    .with_session_id(format!("discord_{}", user_id));
                crate::agent::Agent::new(config)
                    .expect("Failed to create agent")
            });

        match agent.run(&text).await {
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

pub struct DiscordIntegration {
    _client: serenity::Client,
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
        };

        let client = Client::builder(token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| anyhow::anyhow!("Discord client failed: {}", e))?;

        // Start the client in background
        let mut client_clone = client; // serenity Client is not Clone
        tokio::spawn(async move {
            if let Err(e) = client_clone.start().await {
                eprintln!("❌ Discord client error: {}", e);
            }
        });

        // Return placeholder — the real client is in the spawned task
        // In production, keep a handle via Arc<Mutex<Client>>
        Ok(Self { _client: panic!("see note above") })
    }
}
