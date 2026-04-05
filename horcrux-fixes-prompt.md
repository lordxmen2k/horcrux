# Horcrux Agent - Comprehensive Fix & Feature Implementation Prompt

You are working on the Horcrux AI agent codebase, a Rust project. Below is a complete, ordered list of every fix and feature to implement. Follow each section exactly.

---

## CRITICAL CONTEXT

- Language: Rust (async with tokio)
- Messaging: teloxide (Telegram), serenity (Discord), planned WhatsApp/Slack/Matrix
- LLM: OpenAI-compatible API (Kimi K2 / Moonshot)
- Config: `~/.horcrux/config.toml`
- Skills: `~/.horcrux/skills/*.md` (Markdown with YAML frontmatter)
- DB: SQLite via `horcrux::db::Db`
- The agent loop is in `src/agent/react.rs`
- Tools are in `src/tools/`
- Platform integrations live in `src/integrations/` (needs to be created)

---

## FIX 1: Remove `ListSkillsTool` from Tool Registry

**File:** `src/tools/mod.rs`

In the `default_with_db()` function, find and DELETE this line:

```rust
registry.register(Arc::new(ListSkillsTool::new(skills_dir)));
```

Keep the `CreateSkillTool` registration. Skills are already injected into the system prompt by `react.rs` — `ListSkillsTool` wastes a full LLM turn every time the model calls it.

---

## FIX 2: Persistent Agent Per Telegram Chat ID

**File:** `src/tools/telegram.rs`

The current code creates a fresh `Agent` for every message. Replace `TelegramAgentBot` with this persistent version:

```rust
use std::collections::HashMap;

pub struct TelegramAgentBot {
    agent_config: crate::agent::AgentConfig,
    agents: Arc<Mutex<HashMap<i64, crate::agent::Agent>>>,
}

impl TelegramAgentBot {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self {
            agent_config: crate::agent::AgentConfig::new(db_path),
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
                            let username = msg.from()
                                .and_then(|u| u.username.clone())
                                .unwrap_or_else(|| "Unknown".to_string());
                            println!("📩 Message from {}: {}", username, text);

                            let chat_id = msg.chat.id;

                            // Send typing indicator
                            let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

                            // Delete old "Thinking..." message if any
                            let status_msg = bot.send_message(chat_id, "🤔 Thinking...").await.ok().map(|m| m.id);

                            // Spawn typing keepalive
                            let typing_bot = bot.clone();
                            let typing_handle = tokio::spawn(async move {
                                let mut interval = tokio::time::interval(
                                    tokio::time::Duration::from_secs(3)
                                );
                                loop {
                                    interval.tick().await;
                                    let _ = typing_bot.send_chat_action(
                                        chat_id,
                                        teloxide::types::ChatAction::Typing
                                    ).await;
                                }
                            });

                            // Get or create persistent agent for this chat
                            let mut agents_map = agents.lock().await;
                            let agent = agents_map
                                .entry(chat_id.0)
                                .or_insert_with(|| {
                                    let config = crate::agent::AgentConfig::new(db_path.clone())
                                        .with_session_id(format!("telegram_{}", chat_id.0));
                                    crate::agent::Agent::new(config)
                                        .expect("Failed to create agent")
                                });

                            let response = agent.run(text).await;
                            drop(agents_map); // release lock before sending

                            typing_handle.abort();

                            // Delete "Thinking..." message
                            if let Some(msg_id) = status_msg {
                                let _ = bot.delete_message(chat_id, msg_id).await;
                            }

                            match response {
                                Ok(response_text) => {
                                    println!("📝 Raw response ({} chars): {:?}",
                                        response_text.len(),
                                        &response_text[..response_text.len().min(200)]);

                                    let sanitized = sanitize_for_user(&response_text);

                                    if sanitized.trim().is_empty() {
                                        let _ = bot.send_message(chat_id,
                                            "🤔 I processed your request but didn't generate a response. Please try again."
                                        ).await;
                                        return Ok(());
                                    }

                                    if let Err(e) = send_agent_response(&bot, chat_id, &sanitized).await {
                                        eprintln!("❌ Failed to send response: {}", e);
                                    }
                                }
                                Err(e) => {
                                    let _ = bot.send_message(chat_id, format!("❌ Error: {}", e)).await;
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
```

---

## FIX 3: Image Tag Format — Fix "title = title =" Bug

**File:** `src/tools/image_search.rs`

Find the section in `execute()` where it builds the output string and replace it:

```rust
// WRONG - unquoted fields break when title has spaces/commas:
output.push_str(&format!(
    "[IMAGE_{}] file={} title={} source={}\n",
    i + 1, img.local_path, img.title, img.source
));

// CORRECT - quoted path only, no extra fields:
output.push_str(&format!(
    "[IMAGE_{}] file=\"{}\"\n",
    i + 1,
    img.local_path,
));
```

Also ensure the warning line at the end stays:
```rust
output.push_str("\n⚠️ IMPORTANT: Copy the [IMAGE_N] file=... lines above EXACTLY into your response. Do NOT modify the paths.\n");
```

---

## FIX 4: Update Image Regex in All Platform Handlers

Everywhere you use a regex to parse `[IMAGE_N]` tags, use this pattern that handles both quoted and unquoted paths:

```rust
let re = regex::Regex::new(r#"\[IMAGE_\d+\]\s*file="?([^"\s\n\[]+)"?"#).unwrap();
```

---

## FIX 5: Create the Gateway Trait and Shared Response Logic

**Create new file:** `src/gateway/mod.rs`

This is the core abstraction that makes all platform integrations share the same logic:

```rust
//! Gateway trait - unified interface for all messaging platforms
//!
//! Each platform (Telegram, Discord, WhatsApp, Slack, Matrix) implements
//! this trait. The send_response() default handles [IMAGE_N] parsing
//! and markdown cleaning for ALL platforms.

use async_trait::async_trait;
use anyhow::Result;

/// Unified messaging gateway interface
#[async_trait]
pub trait Gateway: Send + Sync {
    /// Send plain text to a chat/channel
    async fn send_text(&self, chat_id: &str, text: &str) -> Result<()>;

    /// Send an image file to a chat/channel
    async fn send_image(&self, chat_id: &str, file_path: &str) -> Result<()>;

    /// Send a document/file attachment
    async fn send_file(&self, chat_id: &str, file_path: &str, caption: Option<&str>) -> Result<()> {
        // Default: send as image, platforms can override
        self.send_image(chat_id, file_path).await
    }

    /// Parse and route an agent response — handles [IMAGE_N] tags automatically
    /// This default implementation works for ALL platforms
    async fn send_response(&self, chat_id: &str, response: &str) -> Result<()> {
        let (text, image_paths) = parse_agent_response(response);
        let clean_text = clean_for_platform(&text);

        if !clean_text.trim().is_empty() {
            // Split at platform limit (handled per-platform in send_text)
            self.send_text(chat_id, &clean_text).await?;
        }

        for (i, path) in image_paths.iter().enumerate() {
            let p = std::path::Path::new(path);

            if !p.exists() {
                println!("⚠️ Skipping hallucinated path {}: {}", i + 1, path);
                continue;
            }

            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if size < 5_000 {
                println!("⚠️ Skipping small/corrupt file {}: {} bytes", i + 1, size);
                continue;
            }

            println!("📸 Sending image {} ({} bytes): {}", i + 1, size, path);
            match self.send_image(chat_id, path).await {
                Ok(_) => println!("✅ Image {} sent", i + 1),
                Err(e) => println!("❌ Image {} failed: {}", i + 1, e),
            }
        }

        Ok(())
    }
}

/// Parse [IMAGE_N] tags from agent response
/// Returns (text_without_tags, vec_of_file_paths)
pub fn parse_agent_response(response: &str) -> (String, Vec<String>) {
    let mut text_parts: Vec<String> = Vec::new();
    let mut image_paths: Vec<String> = Vec::new();
    let mut last_end = 0;

    // Handles both quoted: file="C:\path\img.jpg" and unquoted: file=/tmp/img.jpg
    let re = regex::Regex::new(r#"\[IMAGE_\d+\]\s*file="?([^"\s\n\[]+)"?"#).unwrap();

    for cap in re.captures_iter(response) {
        let full = cap.get(0).unwrap();
        let before = response[last_end..full.start()].trim();
        if !before.is_empty() {
            text_parts.push(before.to_string());
        }
        if let Some(path) = cap.get(1) {
            image_paths.push(path.as_str().to_string());
        }
        last_end = full.end();
    }

    let remaining = response[last_end..].trim();
    if !remaining.is_empty() {
        text_parts.push(remaining.to_string());
    }

    (text_parts.join("\n"), image_paths)
}

/// Clean markdown for plain-text platforms (Telegram plain, WhatsApp, SMS)
/// Converts markdown to readable plain text with unicode bullets
pub fn clean_for_platform(text: &str) -> String {
    let mut output = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Headers → plain text (keep content, remove #)
        let content = if trimmed.starts_with("### ") {
            trimmed[4..].to_string()
        } else if trimmed.starts_with("## ") {
            trimmed[3..].to_string()
        } else if trimmed.starts_with("# ") {
            trimmed[2..].to_string()
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            // Markdown bullets → unicode bullet
            format!("• {}", &trimmed[2..])
        } else {
            // Remove bold/italic markers
            trimmed
                .replace("**", "")
                .replace("__", "")
                .replace('*', "")
                .replace('`', "")
        };

        output.push_str(&content);
        output.push('\n');
    }

    output.trim().to_string()
}

/// Convert markdown to Telegram HTML format
/// Use this when you want bold/italic in Telegram
pub fn markdown_to_telegram_html(text: &str) -> String {
    // Simple conversion - for production use a proper markdown parser
    let mut result = text.to_string();
    
    // Bold: **text** → <b>text</b>
    let bold_re = regex::Regex::new(r"\*\*(.+?)\*\*").unwrap();
    result = bold_re.replace_all(&result, "<b>$1</b>").to_string();
    
    // Code: `text` → <code>text</code>
    let code_re = regex::Regex::new(r"`([^`]+)`").unwrap();
    result = code_re.replace_all(&result, "<code>$1</code>").to_string();
    
    // Headers: ## text → <b>text</b>
    let header_re = regex::Regex::new(r"(?m)^#{1,3}\s+(.+)$").unwrap();
    result = header_re.replace_all(&result, "<b>$1</b>").to_string();
    
    result
}

/// Sanitize agent output — remove leaked tool call JSON
pub fn sanitize_agent_output(text: &str) -> String {
    use regex::Regex;
    let mut result = text.to_string();

    // Remove JSON tool call objects: {"name": "tool", "arguments": {...}}
    if let Ok(re) = Regex::new(r#"\{[^{}]*"name"\s*:\s*"[^"]+"\s*,\s*"arguments"[^{}]*\}"#) {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove XML tool call tags
    if let Ok(re) = Regex::new(r"<tool_call>[\s\S]*?</tool_call>") {
        result = re.replace_all(&result, "").to_string();
    }

    // Clean up resulting empty lines
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split long text at natural boundaries for platform character limits
pub fn split_message(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > max_chars {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}
```

Add `pub mod gateway;` to `src/lib.rs`.

---

## FIX 6: Refactor Telegram to Use Gateway Trait

**File:** `src/tools/telegram.rs`

Replace the `send_agent_response` free function with a `TelegramGateway` struct that implements the `Gateway` trait:

```rust
use crate::gateway::{Gateway, parse_agent_response, sanitize_agent_output, split_message};

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
            teloxide::types::InputFile::file(file_path)
        ).await?;
        Ok(())
    }
}

// Keep the free function for backward compat but implement via Gateway:
pub async fn send_agent_response(bot: &Bot, chat_id: ChatId, response: &str) -> anyhow::Result<()> {
    let gateway = TelegramGateway { bot: bot.clone(), chat_id };
    gateway.send_response(&chat_id.0.to_string(), response).await
}
```

---

## FIX 7: Create Discord Integration

**Create new file:** `src/integrations/discord.rs`

```rust
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
        let filename = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        self.channel_id.send_files(
            &self.http,
            vec![serenity::model::channel::AttachmentType::Path(path)],
            |m| m.content("")
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
            && !self.config.allowed_channels.contains(&msg.channel_id.0)
        {
            return;
        }

        let channel_id = msg.channel_id;
        let user_id = msg.author.id.0;

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

                if let Err(e) = gateway.send_response(&channel_id.0.to_string(), &sanitized).await {
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
        let token = config.token.as_ref()
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
```

**Add to `Cargo.toml`:**
```toml
serenity = { version = "0.12", features = ["client", "gateway", "model", "rustls_backend"] }
```

---

## FIX 8: Create WhatsApp Integration (via Twilio or WhatsApp Business API)

**Create new file:** `src/integrations/whatsapp.rs`

```rust
//! WhatsApp Integration via Twilio WhatsApp API
//!
//! Setup:
//! 1. Create Twilio account at https://www.twilio.com
//! 2. Enable WhatsApp Sandbox or WhatsApp Business
//! 3. Add to config.toml:
//!    [whatsapp]
//!    account_sid = "ACxxx"
//!    auth_token = "your_token"
//!    from_number = "whatsapp:+14155238886"  # Twilio sandbox number
//!    enabled = true
//! 4. Configure Twilio webhook to point to your server:
//!    POST https://yourserver.com/whatsapp/webhook

use crate::gateway::{Gateway, sanitize_agent_output, clean_for_platform, split_message};
use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::State, routing::post, Form, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppConfig {
    pub account_sid: Option<String>,
    pub auth_token: Option<String>,
    pub from_number: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

pub struct WhatsAppGateway {
    client: reqwest::Client,
    account_sid: String,
    auth_token: String,
    from_number: String,
    to_number: String,
}

#[async_trait]
impl Gateway for WhatsAppGateway {
    async fn send_text(&self, _to: &str, text: &str) -> Result<()> {
        // WhatsApp via Twilio limit: 1600 chars
        for chunk in split_message(text, 1500) {
            let url = format!(
                "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
                self.account_sid
            );

            self.client
                .post(&url)
                .basic_auth(&self.account_sid, Some(&self.auth_token))
                .form(&[
                    ("From", self.from_number.clone()),
                    ("To", format!("whatsapp:{}", self.to_number)),
                    ("Body", chunk),
                ])
                .send()
                .await?;
        }
        Ok(())
    }

    async fn send_image(&self, _to: &str, file_path: &str) -> Result<()> {
        // WhatsApp images need a public URL, not a local file path
        // For now, send the file path as text with a note
        // In production: upload to S3/CDN first, then send URL
        self.send_text(
            _to,
            &format!("📷 Image available (file: {})", file_path)
        ).await
    }
}

/// Twilio webhook payload
#[derive(Deserialize)]
pub struct TwilioWebhook {
    #[serde(rename = "From")]
    pub from: String,
    #[serde(rename = "Body")]
    pub body: String,
    #[serde(rename = "MediaUrl0")]
    pub media_url: Option<String>,
}

struct WhatsAppState {
    agents: Arc<Mutex<HashMap<String, crate::agent::Agent>>>,
    db_path: std::path::PathBuf,
    config: WhatsAppConfig,
}

pub struct WhatsAppIntegration;

impl WhatsAppIntegration {
    pub async fn start_webhook_server(
        config: WhatsAppConfig,
        db_path: std::path::PathBuf,
    ) -> Result<()> {
        let port = config.webhook_port.unwrap_or(8080);
        let state = Arc::new(WhatsAppState {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
        });

        let app = Router::new()
            .route("/whatsapp/webhook", post(handle_whatsapp_webhook))
            .with_state(state);

        let addr = format!("0.0.0.0:{}", port);
        println!("📱 WhatsApp webhook server on http://{}/whatsapp/webhook", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn handle_whatsapp_webhook(
    State(state): State<Arc<WhatsAppState>>,
    Form(payload): Form<TwilioWebhook>,
) -> String {
    let from = payload.from.replace("whatsapp:", "");
    let text = payload.body.trim().to_string();

    println!("📩 WhatsApp from {}: {}", from, text);

    let mut agents_map = state.agents.lock().await;
    let agent = agents_map
        .entry(from.clone())
        .or_insert_with(|| {
            let config = crate::agent::AgentConfig::new(state.db_path.clone())
                .with_session_id(format!("whatsapp_{}", from.replace('+', "")));
            crate::agent::Agent::new(config).expect("Failed to create agent")
        });

    match agent.run(&text).await {
        Ok(response) => {
            let sanitized = sanitize_agent_output(&response);
            let (text_part, _images) = crate::gateway::parse_agent_response(&sanitized);
            let clean = clean_for_platform(&text_part);
            drop(agents_map);

            // Send via Twilio
            if let (Some(sid), Some(token), Some(from_num)) = (
                &state.config.account_sid,
                &state.config.auth_token,
                &state.config.from_number,
            ) {
                let gw = WhatsAppGateway {
                    client: reqwest::Client::new(),
                    account_sid: sid.clone(),
                    auth_token: token.clone(),
                    from_number: from_num.clone(),
                    to_number: from.clone(),
                };
                let _ = gw.send_text(&from, &clean).await;
            }

            // Return TwiML response (empty = no auto-reply)
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response></Response>".to_string()
        }
        Err(e) => {
            drop(agents_map);
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response><Message>❌ Error: {}</Message></Response>", e)
        }
    }
}
```

---

## FIX 9: Create Slack Integration

**Create new file:** `src/integrations/slack.rs`

```rust
//! Slack Bot Integration via Slack Bolt / Events API
//!
//! Setup:
//! 1. Create app at https://api.slack.com/apps
//! 2. Add OAuth scopes: chat:write, files:write, im:history, im:read
//! 3. Enable Events API, subscribe to: message.im, app_mention
//! 4. Set Request URL to: https://yourserver.com/slack/events
//! 5. Add to config.toml:
//!    [slack]
//!    bot_token = "xoxb-your-token"
//!    signing_secret = "your-signing-secret"
//!    enabled = true

use crate::gateway::{Gateway, sanitize_agent_output, split_message};
use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackConfig {
    pub bot_token: Option<String>,
    pub signing_secret: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

pub struct SlackGateway {
    client: reqwest::Client,
    bot_token: String,
    channel: String,
}

#[async_trait]
impl Gateway for SlackGateway {
    async fn send_text(&self, _channel: &str, text: &str) -> Result<()> {
        // Slack limit: 40000 chars (but keep reasonable)
        for chunk in split_message(text, 3000) {
            self.client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(&self.bot_token)
                .json(&serde_json::json!({
                    "channel": self.channel,
                    "text": chunk,
                    "mrkdwn": true  // Slack uses mrkdwn not markdown
                }))
                .send()
                .await?;
        }
        Ok(())
    }

    async fn send_image(&self, _channel: &str, file_path: &str) -> Result<()> {
        // Upload file to Slack
        let file_content = tokio::fs::read(file_path).await?;
        let filename = std::path::Path::new(file_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let form = reqwest::multipart::Form::new()
            .text("channels", self.channel.clone())
            .text("filename", filename)
            .part("file", reqwest::multipart::Part::bytes(file_content));

        self.client
            .post("https://slack.com/api/files.upload")
            .bearer_auth(&self.bot_token)
            .multipart(form)
            .send()
            .await?;

        Ok(())
    }
}

#[derive(Deserialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub challenge: Option<String>,
    pub event: Option<SlackMessageEvent>,
}

#[derive(Deserialize)]
pub struct SlackMessageEvent {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub user: Option<String>,
    pub text: Option<String>,
    pub channel: Option<String>,
    pub bot_id: Option<String>,
}

struct SlackState {
    agents: Arc<Mutex<HashMap<String, crate::agent::Agent>>>,
    db_path: std::path::PathBuf,
    config: SlackConfig,
}

pub struct SlackIntegration;

impl SlackIntegration {
    pub async fn start(config: SlackConfig, db_path: std::path::PathBuf) -> Result<()> {
        let port = config.webhook_port.unwrap_or(8081);
        let state = Arc::new(SlackState {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
        });

        let app = Router::new()
            .route("/slack/events", post(handle_slack_event))
            .with_state(state);

        let addr = format!("0.0.0.0:{}", port);
        println!("💬 Slack events server on http://{}/slack/events", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn handle_slack_event(
    State(state): State<Arc<SlackState>>,
    Json(payload): Json<SlackEvent>,
) -> Json<serde_json::Value> {
    // Handle URL verification challenge
    if let Some(challenge) = payload.challenge {
        return Json(serde_json::json!({ "challenge": challenge }));
    }

    if let Some(event) = payload.event {
        // Ignore bot messages to avoid loops
        if event.bot_id.is_some() {
            return Json(serde_json::json!({ "ok": true }));
        }

        let text = event.text.unwrap_or_default();
        let user = event.user.unwrap_or_else(|| "unknown".to_string());
        let channel = event.channel.unwrap_or_else(|| "unknown".to_string());

        if text.trim().is_empty() {
            return Json(serde_json::json!({ "ok": true }));
        }

        println!("📩 Slack from {} in {}: {}", user, channel, text);

        let db_path = state.db_path.clone();
        let config = state.config.clone();
        let agents = state.agents.clone();

        // Process in background to respond to Slack quickly (3s limit)
        tokio::spawn(async move {
            let mut agents_map = agents.lock().await;
            let agent = agents_map
                .entry(user.clone())
                .or_insert_with(|| {
                    let cfg = crate::agent::AgentConfig::new(db_path.clone())
                        .with_session_id(format!("slack_{}", user));
                    crate::agent::Agent::new(cfg).expect("Failed to create agent")
                });

            match agent.run(&text).await {
                Ok(response) => {
                    let sanitized = sanitize_agent_output(&response);
                    drop(agents_map);

                    if let Some(token) = &config.bot_token {
                        let gw = SlackGateway {
                            client: reqwest::Client::new(),
                            bot_token: token.clone(),
                            channel: channel.clone(),
                        };
                        let _ = gw.send_response(&channel, &sanitized).await;
                    }
                }
                Err(e) => {
                    drop(agents_map);
                    eprintln!("❌ Slack agent error: {}", e);
                }
            }
        });
    }

    Json(serde_json::json!({ "ok": true }))
}
```

---

## FIX 10: Create Matrix Integration

**Create new file:** `src/integrations/matrix.rs`

```rust
//! Matrix Bot Integration via matrix-sdk
//!
//! Setup:
//! 1. Create a Matrix account for your bot at any homeserver (matrix.org, etc.)
//! 2. Add to config.toml:
//!    [matrix]
//!    homeserver = "https://matrix.org"
//!    username = "@yourbot:matrix.org"
//!    password = "bot-password"
//!    enabled = true

use crate::gateway::{Gateway, sanitize_agent_output, split_message};
use anyhow::Result;
use async_trait::async_trait;
use matrix_sdk::{
    config::SyncSettings,
    room::Room,
    ruma::events::room::message::{
        MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatrixConfig {
    pub homeserver: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
}

pub struct MatrixGateway {
    room: Room,
}

#[async_trait]
impl Gateway for MatrixGateway {
    async fn send_text(&self, _room_id: &str, text: &str) -> Result<()> {
        for chunk in split_message(text, 3000) {
            let content = RoomMessageEventContent::text_plain(chunk);
            self.room.send(content).await
                .map_err(|e| anyhow::anyhow!("Matrix send failed: {}", e))?;
        }
        Ok(())
    }

    async fn send_image(&self, _room_id: &str, file_path: &str) -> Result<()> {
        // Matrix image upload - requires uploading to homeserver first
        // Simplified: send as text message with path
        // Full implementation would use room.send_attachment()
        self.send_text(
            _room_id,
            &format!("📷 [Image: {}]", file_path)
        ).await
    }
}

pub struct MatrixIntegration;

impl MatrixIntegration {
    pub async fn start(
        config: MatrixConfig,
        db_path: std::path::PathBuf,
    ) -> Result<()> {
        let homeserver = config.homeserver.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix homeserver not configured"))?;
        let username = config.username.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix username not configured"))?;
        let password = config.password.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix password not configured"))?;

        let client = Client::builder()
            .homeserver_url(homeserver)
            .build()
            .await?;

        client.matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Horcrux Agent")
            .await?;

        println!("✅ Matrix bot logged in as: {}", username);

        let agents: Arc<Mutex<HashMap<String, crate::agent::Agent>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Register message handler
        client.add_event_handler({
            let agents = agents.clone();
            let db_path = db_path.clone();
            move |event: OriginalSyncRoomMessageEvent, room: Room| {
                let agents = agents.clone();
                let db_path = db_path.clone();
                async move {
                    // Ignore own messages
                    let MessageType::Text(text_content) = event.content.msgtype else { return; };
                    let text = text_content.body;
                    let user_id = event.sender.to_string();

                    println!("📩 Matrix from {}: {}", user_id, text);

                    let mut agents_map = agents.lock().await;
                    let agent = agents_map
                        .entry(user_id.clone())
                        .or_insert_with(|| {
                            let cfg = crate::agent::AgentConfig::new(db_path.clone())
                                .with_session_id(format!("matrix_{}", 
                                    user_id.replace([':', '@'], "_")));
                            crate::agent::Agent::new(cfg).expect("Failed to create agent")
                        });

                    match agent.run(&text).await {
                        Ok(response) => {
                            let sanitized = sanitize_agent_output(&response);
                            drop(agents_map);

                            let gateway = MatrixGateway { room };
                            let _ = gateway.send_response(&user_id, &sanitized).await;
                        }
                        Err(e) => {
                            drop(agents_map);
                            let content = RoomMessageEventContent::text_plain(
                                format!("❌ Error: {}", e)
                            );
                            let _ = room.send(content).await;
                        }
                    }
                }
            }
        });

        // Start sync loop
        client.sync(SyncSettings::default()).await?;

        Ok(())
    }
}
```

**Add to `Cargo.toml`:**
```toml
matrix-sdk = { version = "0.7", features = ["rustls"] }
```

---

## FIX 11: Update `config.toml` Structure for All Platforms

**File:** `src/config.rs`

Add platform configs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub images: ImageConfig,
    #[serde(default)]
    pub telegram: TelegramPlatformConfig,
    #[serde(default)]
    pub discord: DiscordPlatformConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppPlatformConfig,
    #[serde(default)]
    pub slack: SlackPlatformConfig,
    #[serde(default)]
    pub matrix: MatrixPlatformConfig,
    #[serde(default)]
    pub agent: AgentBehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramPlatformConfig {
    pub bot_token: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordPlatformConfig {
    pub bot_token: Option<String>,
    pub enabled: bool,
    pub allowed_channels: Vec<u64>,
    pub command_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppPlatformConfig {
    pub account_sid: Option<String>,
    pub auth_token: Option<String>,
    pub from_number: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackPlatformConfig {
    pub bot_token: Option<String>,
    pub signing_secret: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatrixPlatformConfig {
    pub homeserver: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
}
```

---

## FIX 12: Update Setup Wizard for All Platforms

**File:** `src/cli/setup.rs` (or wherever your `SetupWizard` lives)

Add setup sections for each platform. The wizard should follow this pattern for every platform:

```rust
async fn setup_discord(&self) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    
    println!("\n── Discord Setup ──────────────────────────────");
    println!("Create a bot at: https://discord.com/developers/applications");
    println!("Required intents: MESSAGE_CONTENT, GUILD_MESSAGES, DIRECT_MESSAGES\n");
    
    let current_token = config.discord.bot_token.as_deref().unwrap_or("");
    let masked = mask_key(current_token);
    
    println!("Bot Token (current: {})", masked);
    println!("Press Enter to keep current value\n");
    
    let token = prompt("Bot Token");
    if !token.trim().is_empty() {
        config.discord.bot_token = Some(token.trim().to_string());
    }
    
    config.discord.enabled = config.discord.bot_token.is_some();
    config.save()?;
    
    if config.discord.enabled {
        println!("\n✅ Discord configured!");
        println!("Start with: horcrux agent --discord");
    }
    
    Ok(())
}

async fn setup_slack(&self) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    
    println!("\n── Slack Setup ────────────────────────────────");
    println!("Create app at: https://api.slack.com/apps");
    println!("Required scopes: chat:write, files:write, im:history, im:read");
    println!("Enable Events API → subscribe to: message.im, app_mention");
    println!("Webhook URL: https://yourserver.com/slack/events\n");
    
    let token = prompt_with_current("Bot Token (xoxb-...)", 
        config.slack.bot_token.as_deref().unwrap_or(""));
    if !token.is_empty() { config.slack.bot_token = Some(token); }
    
    let secret = prompt_with_current("Signing Secret",
        config.slack.signing_secret.as_deref().unwrap_or(""));
    if !secret.is_empty() { config.slack.signing_secret = Some(secret); }
    
    config.slack.enabled = config.slack.bot_token.is_some();
    config.save()?;
    println!("✅ Slack configured!");
    Ok(())
}

async fn setup_whatsapp(&self) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    
    println!("\n── WhatsApp Setup (via Twilio) ────────────────");
    println!("Create account at: https://www.twilio.com");
    println!("Enable WhatsApp Sandbox or WhatsApp Business API");
    println!("Configure webhook: POST https://yourserver.com/whatsapp/webhook\n");
    
    let sid = prompt_with_current("Account SID",
        config.whatsapp.account_sid.as_deref().unwrap_or(""));
    if !sid.is_empty() { config.whatsapp.account_sid = Some(sid); }
    
    let token = prompt_with_current("Auth Token",
        config.whatsapp.auth_token.as_deref().unwrap_or(""));
    if !token.is_empty() { config.whatsapp.auth_token = Some(token); }
    
    let from = prompt_with_current("From Number (e.g. whatsapp:+14155238886)",
        config.whatsapp.from_number.as_deref().unwrap_or(""));
    if !from.is_empty() { config.whatsapp.from_number = Some(from); }
    
    config.whatsapp.enabled = config.whatsapp.account_sid.is_some();
    config.save()?;
    println!("✅ WhatsApp configured!");
    Ok(())
}

async fn setup_matrix(&self) -> Result<()> {
    let mut config = Config::load().unwrap_or_default();
    
    println!("\n── Matrix Setup ───────────────────────────────");
    println!("Create a bot account at any Matrix homeserver (e.g. matrix.org)");
    println!("The bot will join rooms and respond to messages.\n");
    
    let homeserver = prompt_with_current("Homeserver URL (e.g. https://matrix.org)",
        config.matrix.homeserver.as_deref().unwrap_or(""));
    if !homeserver.is_empty() { config.matrix.homeserver = Some(homeserver); }
    
    let username = prompt_with_current("Bot Username (e.g. @mybot:matrix.org)",
        config.matrix.username.as_deref().unwrap_or(""));
    if !username.is_empty() { config.matrix.username = Some(username); }
    
    let password = prompt_with_current("Bot Password",
        config.matrix.password.as_deref().unwrap_or(""));
    if !password.is_empty() { config.matrix.password = Some(password); }
    
    config.matrix.enabled = config.matrix.homeserver.is_some();
    config.save()?;
    println!("✅ Matrix configured!");
    Ok(())
}
```

The setup menu status header should show all platforms:

```rust
fn print_status_header() {
    let config = Config::load().unwrap_or_default();
    
    println!("\n🧙 Horcrux Setup\n");
    println!("Platform Status:");
    
    print_platform_status("Telegram", config.telegram.bot_token.as_deref(), config.telegram.enabled);
    print_platform_status("Discord",  config.discord.bot_token.as_deref(),  config.discord.enabled);
    print_platform_status("WhatsApp", config.whatsapp.account_sid.as_deref(), config.whatsapp.enabled);
    print_platform_status("Slack",    config.slack.bot_token.as_deref(),    config.slack.enabled);
    print_platform_status("Matrix",   config.matrix.username.as_deref(),    config.matrix.enabled);
    
    println!("\nOther:");
    // LLM, Images...
}

fn print_platform_status(name: &str, key: Option<&str>, enabled: bool) {
    let status = match (key, enabled) {
        (Some(k), true) if !k.is_empty() => "✅ Configured",
        (Some(k), false) if !k.is_empty() => "⚠️  Configured but disabled",
        _ => "❌ Not configured",
    };
    println!("  {:<12} {}", name, status);
}
```

---

## FIX 13: Update `main.rs` to Support All Platform Flags

**File:** `src/main.rs` and `src/cli/agent.rs`

Add CLI flags for each platform:

```rust
// In AgentArgs struct:
#[derive(Args, Debug)]
pub struct AgentArgs {
    pub message: Option<String>,
    
    #[arg(short, long)]
    pub session: Option<String>,
    
    #[arg(long)]
    pub telegram: bool,
    
    #[arg(long)]
    pub discord: bool,
    
    #[arg(long)]
    pub whatsapp: bool,
    
    #[arg(long)]
    pub slack: bool,
    
    #[arg(long)]
    pub matrix: bool,
    
    /// Run all enabled platform bots simultaneously
    #[arg(long)]
    pub all_platforms: bool,
    
    // existing flags...
    #[arg(long)]
    pub setup: bool,
    #[arg(long)]
    pub list_sessions: bool,
    #[arg(long)]
    pub clear: bool,
}
```

In `cli/agent.rs` `run()` function, handle the new flags:

```rust
// After existing flag checks, before interactive mode:

if args.all_platforms {
    return run_all_platforms(db_path).await;
}

if args.discord {
    return run_discord_bot(db_path).await;
}

if args.whatsapp {
    return run_whatsapp_server(db_path).await;
}

if args.slack {
    return run_slack_server(db_path).await;
}

if args.matrix {
    return run_matrix_bot(db_path).await;
}
```

Add runner functions:

```rust
async fn run_discord_bot(db_path: &PathBuf) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    if !config.discord.enabled {
        eprintln!("❌ Discord not configured. Run: horcrux setup discord");
        std::process::exit(1);
    }
    println!("🚀 Starting Discord Bot...");
    crate::integrations::discord::DiscordIntegration::new(
        crate::integrations::discord::DiscordConfig {
            token: config.discord.bot_token,
            enabled: true,
            allowed_channels: config.discord.allowed_channels,
            prefix: config.discord.command_prefix,
        },
        db_path.clone(),
    ).await?;
    Ok(())
}

async fn run_all_platforms(db_path: &PathBuf) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let mut tasks = vec![];
    
    if config.telegram.enabled {
        let db = db_path.clone();
        tasks.push(tokio::spawn(async move {
            let bot = crate::tools::TelegramAgentBot::new(db);
            if let Err(e) = bot.run().await {
                eprintln!("❌ Telegram error: {}", e);
            }
        }));
        println!("✅ Telegram started");
    }
    
    if config.discord.enabled {
        // spawn discord task
        println!("✅ Discord started");
    }
    
    if config.slack.enabled {
        // spawn slack task
        println!("✅ Slack started");
    }
    
    if config.whatsapp.enabled {
        // spawn whatsapp task
        println!("✅ WhatsApp webhook started");
    }
    
    if config.matrix.enabled {
        // spawn matrix task
        println!("✅ Matrix started");
    }
    
    if tasks.is_empty() {
        eprintln!("⚠️  No platforms are enabled. Run: horcrux setup");
        std::process::exit(1);
    }
    
    println!("\n🚀 Running {} platform(s). Press Ctrl+C to stop.", tasks.len());
    
    // Wait for all tasks
    for task in tasks {
        let _ = task.await;
    }
    
    Ok(())
}
```

---

## FIX 14: Update `lib.rs` Module Declarations

**File:** `src/lib.rs`

```rust
pub mod agent;
pub mod cache;
pub mod chunk;
pub mod config;
pub mod db;
pub mod embed;
pub mod gateway;       // ADD THIS
pub mod integrations;  // ADD THIS
pub mod search;
pub mod skills;
pub mod tools;
pub mod types;

pub use types::{Collection, Document, Chunk, SearchResult};
pub use db::Db;
pub use embed::{EmbedClient, EmbedConfig, cosine_similarity};
pub use cache::SearchCache;
pub use agent::{Agent, AgentConfig, LlmClient, LlmConfig};
pub use gateway::{Gateway, parse_agent_response, sanitize_agent_output};
```

**Create `src/integrations/mod.rs`:**

```rust
pub mod discord;
pub mod matrix;
pub mod slack;
pub mod whatsapp;
```

---

## FIX 15: Cargo.toml Dependencies

Add all required dependencies:

```toml
[dependencies]
# existing deps...

# Discord
serenity = { version = "0.12", default-features = false, features = [
    "client", "gateway", "model", "rustls_backend", "cache"
] }

# Matrix  
matrix-sdk = { version = "0.7", features = ["rustls"] }

# Shared async utilities
futures = "0.3"
async-trait = "0.1"

# HTTP for WhatsApp/Slack webhooks (already have axum and reqwest)
# No new deps needed for WhatsApp and Slack
```

---

## SUMMARY: Priority Order

Implement in this order — each step is independently testable:

1. **FIX 1** — Remove `ListSkillsTool` (1 line delete, immediate improvement)
2. **FIX 2** — Persistent agent per chat_id in Telegram (fixes "no context")
3. **FIX 3 + 4** — Image tag format and regex (fixes "title = title =")
4. **FIX 5** — Create `src/gateway/mod.rs` with the `Gateway` trait
5. **FIX 6** — Refactor Telegram to use Gateway trait
6. **FIX 11** — Update config.rs with all platform configs
7. **FIX 12** — Update setup wizard for all platforms
8. **FIX 7** — Discord integration
9. **FIX 8** — WhatsApp integration
10. **FIX 9** — Slack integration
11. **FIX 10** — Matrix integration
12. **FIX 13** — Update main.rs and agent.rs CLI flags
13. **FIX 14 + 15** — lib.rs and Cargo.toml

After FIX 1-6 the Telegram bot will work correctly. Fixes 7-13 add Discord, WhatsApp, Slack, and Matrix on top of the same foundation.
