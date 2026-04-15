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
use serde_json::Value;
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

/// Slack bot that can send messages and files
pub struct SlackBot {
    client: reqwest::Client,
    bot_token: String,
}

impl SlackBot {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    pub async fn send_message(&self, channel: &str, text: &str) -> anyhow::Result<()> {
        for chunk in split_message(text, 3000) {
            self.client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(&self.bot_token)
                .json(&serde_json::json!({
                    "channel": channel,
                    "text": chunk,
                    "mrkdwn": true
                }))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Slack send failed: {}", e))?;
        }
        Ok(())
    }

    pub async fn send_file(&self, channel: &str, file_path: &str, caption: Option<&str>) -> anyhow::Result<String> {
        let path = std::path::Path::new(file_path);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_path));
        }
        
        let file_content = tokio::fs::read(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
        let filename = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut form = reqwest::multipart::Form::new()
            .text("channels", channel.to_string())
            .text("filename", filename)
            .part("file", reqwest::multipart::Part::bytes(file_content));
        
        // Add caption if provided
        if let Some(cap) = caption {
            form = form.text("initial_comment", cap.to_string());
        }

        self.client
            .post("https://slack.com/api/files.upload")
            .bearer_auth(&self.bot_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Slack file upload failed: {}", e))?;

        Ok(format!("✅ File sent to Slack channel {}", channel))
    }
}

/// Tool interface for Slack operations
pub struct SlackTool {
    bot: Arc<Mutex<Option<SlackBot>>>,
}

impl SlackTool {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(Mutex::new(None)),
        }
    }

    /// Called by SlackHandler after it creates the bot
    pub fn inject_live_bot(bot: Arc<Mutex<Option<SlackBot>>>) -> Self {
        Self { bot }
    }
}

#[async_trait]
impl crate::tools::Tool for SlackTool {
    fn name(&self) -> &str {
        "slack"
    }

    fn description(&self) -> &str {
        "Send messages and files via Slack bot. \
         Use this to communicate with users through Slack. \
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
                    "description": "The Slack operation to perform"
                },
                "channel": {
                    "type": "string",
                    "description": "Slack channel ID (for send_message, send_file)",
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
                let channel = args["channel"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing channel"))?;
                let message = args["message"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

                if let Some(ref bot) = *bot_guard {
                    match bot.send_message(channel, message).await {
                        Ok(_) => Ok(crate::tools::ToolResult::success(format!("✅ Message sent to channel {}", channel))),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send message: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Slack bot not initialized".to_string()))
                }
            }
            "send_file" => {
                let channel = args["channel"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing channel"))?;
                let file_path = args["file_path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
                let caption = args["caption"].as_str();

                if let Some(ref bot) = *bot_guard {
                    match bot.send_file(channel, file_path, caption).await {
                        Ok(msg) => Ok(crate::tools::ToolResult::success(msg)),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send file: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Slack bot not initialized".to_string()))
                }
            }
            _ => Ok(crate::tools::ToolResult::error(format!("Unknown operation: {}", operation))),
        }
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
    shared_bot: Arc<Mutex<Option<SlackBot>>>,
}

pub struct SlackIntegration;

impl SlackIntegration {
    pub async fn start(config: SlackConfig, db_path: std::path::PathBuf) -> Result<()> {
        let port = config.webhook_port.unwrap_or(8081);
        let bot_token = config.bot_token.clone().unwrap_or_default();
        
        let state = Arc::new(SlackState {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
            shared_bot: Arc::new(Mutex::new(None)),
        });

        // Create and inject the bot first
        let bot = SlackBot::new(bot_token);
        *state.shared_bot.lock().await = Some(bot);

        let app = Router::new()
            .route("/slack/events", post(handle_slack_event))
            .with_state(state.clone());

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
        let shared_bot = state.shared_bot.clone();

        // Process in background to respond to Slack quickly (3s limit)
        tokio::spawn(async move {
            let mut agents_map = agents.lock().await;
            let agent = agents_map
                .entry(user.clone())
                .or_insert_with(|| {
                    let cfg = crate::agent::AgentConfig::new(db_path.clone())
                        .with_session_id(format!("slack_{}", user));
                    // Create agent with Slack tool injected
                    let slack_tool = SlackTool::inject_live_bot(shared_bot.clone());
                    crate::agent::Agent::new_with_slack(cfg, slack_tool)
                        .expect("Failed to create agent")
                });

            // Run with context injection
            let mut context = std::collections::HashMap::new();
            context.insert("platform".to_string(), "slack".to_string());
            context.insert("channel".to_string(), channel.clone());
            context.insert("user".to_string(), user.clone());

            match agent.run_with_context(&text, context).await {
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
