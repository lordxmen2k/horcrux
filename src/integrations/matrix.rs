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
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// Matrix bot that can send messages and files
pub struct MatrixBot {
    homeserver: String,
    access_token: Option<String>,
    client: reqwest::Client,
}

impl MatrixBot {
    pub fn new(homeserver: String) -> Self {
        Self {
            homeserver,
            access_token: None,
            client: reqwest::Client::new(),
        }
    }

    /// Login and get access token
    pub async fn login(&mut self, username: &str, password: &str) -> anyhow::Result<()> {
        let url = format!("{}/_matrix/client/v3/login", self.homeserver);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "type": "m.login.password",
                "user": username,
                "password": password
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Matrix login request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Matrix login failed: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;
        if let Some(token) = data.get("access_token").and_then(|t| t.as_str()) {
            self.access_token = Some(token.to_string());
            println!("✅ Matrix bot logged in successfully");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Matrix login response missing access_token"))
        }
    }

    pub async fn send_message(&self, room_id: &str, text: &str) -> anyhow::Result<()> {
        let token = self.access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix bot not logged in"))?;
        
        let url = format!("{}/_matrix/client/v3/rooms/{}/send/m.room.message", 
            self.homeserver, 
            room_id.replace("#", "%23").replace("!", "%21")
        );

        for chunk in split_message(text, 4000) {
            self.client
                .post(&url)
                .query(&[("access_token", token)])
                .json(&serde_json::json!({
                    "msgtype": "m.text",
                    "body": chunk
                }))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Matrix send failed: {}", e))?;
        }
        Ok(())
    }

    pub async fn send_file(&self, room_id: &str, file_path: &str, caption: Option<&str>) -> anyhow::Result<String> {
        let token = self.access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix bot not logged in"))?;
        
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
        
        // Upload file to Matrix media repository
        let upload_url = format!("{}/_matrix/media/v3/upload", self.homeserver);
        
        // Simple content type detection based on extension
        let content_type = match path.extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("pdf") => "application/pdf",
            Some("txt") => "text/plain",
            _ => "application/octet-stream",
        };
        
        let upload_response = self.client
            .post(&upload_url)
            .query(&[("access_token", token)])
            .query(&[("filename", &filename)])
            .header("Content-Type", content_type)
            .body(file_data)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Matrix file upload failed: {}", e))?;

        if !upload_response.status().is_success() {
            return Err(anyhow::anyhow!("Matrix file upload failed: {}", upload_response.status()));
        }

        let upload_data: serde_json::Value = upload_response.json().await?;
        let mxc_uri = upload_data.get("content_uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Matrix upload response missing content_uri"))?;

        // Send message with file
        let msg_url = format!("{}/_matrix/client/v3/rooms/{}/send/m.room.message",
            self.homeserver,
            room_id.replace("#", "%23").replace("!", "%21")
        );

        let mut message = serde_json::json!({
            "msgtype": "m.file",
            "body": caption.unwrap_or(&filename),
            "url": mxc_uri,
            "info": {
                "size": std::fs::metadata(path)?.len()
            }
        });

        // If it's an image, set appropriate msgtype
        if content_type.starts_with("image/") {
            message["msgtype"] = serde_json::json!("m.image");
            message["info"]["mimetype"] = serde_json::json!(content_type);
        }

        self.client
            .post(&msg_url)
            .query(&[("access_token", token)])
            .json(&message)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Matrix file message failed: {}", e))?;

        Ok(format!("✅ File sent to Matrix room {}", room_id))
    }
}

/// Tool interface for Matrix operations
pub struct MatrixTool {
    bot: Arc<Mutex<Option<MatrixBot>>>,
}

impl MatrixTool {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(Mutex::new(None)),
        }
    }

    /// Called by MatrixHandler after it creates the bot
    pub fn inject_live_bot(bot: Arc<Mutex<Option<MatrixBot>>>) -> Self {
        Self { bot }
    }
}

#[async_trait]
impl crate::tools::Tool for MatrixTool {
    fn name(&self) -> &str {
        "matrix"
    }

    fn description(&self) -> &str {
        "Send messages and files via Matrix bot. \
         Use this to communicate with users through Matrix. \
         Operations: send_message, send_file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["send_message", "send_file"],
                    "description": "The Matrix operation to perform"
                },
                "room_id": {
                    "type": "string",
                    "description": "Matrix room ID (e.g., !abc123:matrix.org)",
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
                let room_id = args["room_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing room_id"))?;
                let message = args["message"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

                if let Some(ref bot) = *bot_guard {
                    match bot.send_message(room_id, message).await {
                        Ok(_) => Ok(crate::tools::ToolResult::success(format!("✅ Message sent to room {}", room_id))),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send message: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Matrix bot not initialized".to_string()))
                }
            }
            "send_file" => {
                let room_id = args["room_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing room_id"))?;
                let file_path = args["file_path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
                let caption = args["caption"].as_str();

                if let Some(ref bot) = *bot_guard {
                    match bot.send_file(room_id, file_path, caption).await {
                        Ok(msg) => Ok(crate::tools::ToolResult::success(msg)),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send file: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("Matrix bot not initialized".to_string()))
                }
            }
            _ => Ok(crate::tools::ToolResult::error(format!("Unknown operation: {}", operation))),
        }
    }
}

struct MatrixState {
    agents: Arc<Mutex<HashMap<String, crate::agent::Agent>>>,
    db_path: std::path::PathBuf,
    config: MatrixConfig,
    shared_bot: Arc<Mutex<Option<MatrixBot>>>,
}

pub struct MatrixIntegration;

impl MatrixIntegration {
    pub async fn start(
        config: MatrixConfig,
        db_path: std::path::PathBuf,
    ) -> Result<()> {
        if !config.enabled {
            println!("ℹ️ Matrix integration is disabled in config");
            return Ok(());
        }

        let homeserver = config.homeserver.clone()
            .ok_or_else(|| anyhow::anyhow!("Matrix homeserver not configured"))?;
        let username = config.username.clone()
            .ok_or_else(|| anyhow::anyhow!("Matrix username not configured"))?;
        let password = config.password.clone()
            .ok_or_else(|| anyhow::anyhow!("Matrix password not configured"))?;

        // Create bot and login
        let mut bot = MatrixBot::new(homeserver.clone());
        bot.login(&username, &password).await?;

        let state = Arc::new(MatrixState {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
            shared_bot: Arc::new(Mutex::new(Some(bot))),
        });

        println!("🤖 Matrix bot started as {}", username);
        println!("   Homeserver: {}", homeserver);
        
        // Note: Full sync and message listening would require matrix-sdk
        // For now, the bot can send messages via the tool
        // TODO: Implement full sync loop with matrix-sdk

        Ok(())
    }
}

/// Matrix Gateway for sending responses
pub struct MatrixGateway {
    bot: Arc<Mutex<Option<MatrixBot>>>,
    room_id: String,
}

#[async_trait]
impl Gateway for MatrixGateway {
    async fn send_text(&self, _room_id: &str, text: &str) -> Result<()> {
        let bot_guard = self.bot.lock().await;
        if let Some(ref bot) = *bot_guard {
            bot.send_message(&self.room_id, text).await
                .map_err(|e| anyhow::anyhow!("Matrix gateway send failed: {}", e))
        } else {
            Err(anyhow::anyhow!("Matrix bot not available"))
        }
    }

    async fn send_image(&self, _room_id: &str, file_path: &str) -> Result<()> {
        let bot_guard = self.bot.lock().await;
        if let Some(ref bot) = *bot_guard {
            bot.send_file(&self.room_id, file_path, None).await
                .map_err(|e| anyhow::anyhow!("Matrix gateway image send failed: {}", e))?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Matrix bot not available"))
        }
    }
}
