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
use serde_json::Value;
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
        // WhatsApp images via Twilio need a public URL
        // For local files, we send a text message with the file info
        // In production: upload to cloud storage first, then send MediaUrl
        self.send_text(
            _to,
            &format!("📷 Image file: {}", file_path)
        ).await
    }
}

/// WhatsApp bot that can send messages via Twilio
pub struct WhatsAppBot {
    client: reqwest::Client,
    account_sid: String,
    auth_token: String,
    from_number: String,
}

impl WhatsAppBot {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            account_sid,
            auth_token,
            from_number,
        }
    }

    pub async fn send_message(&self, to_number: &str, text: &str) -> anyhow::Result<()> {
        // Remove whatsapp: prefix if present
        let to = to_number.strip_prefix("whatsapp:").unwrap_or(to_number);
        
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
                    ("To", format!("whatsapp:{}", to)),
                    ("Body", chunk.to_string()),
                ])
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("WhatsApp send failed: {}", e))?;
        }
        Ok(())
    }

    pub async fn send_file(&self, to_number: &str, file_path: &str, caption: Option<&str>) -> anyhow::Result<String> {
        // WhatsApp via Twilio requires public URLs for media
        // Local files cannot be sent directly - user would need to upload to cloud storage
        let msg = if let Some(cap) = caption {
            format!("{}\n📎 File: {}", cap, file_path)
        } else {
            format!("📎 File available: {}", file_path)
        };
        
        self.send_message(to_number, &msg).await?;
        
        Ok(format!("✅ File reference sent to WhatsApp {}", to_number))
    }
}

/// Tool interface for WhatsApp operations
pub struct WhatsAppTool {
    bot: Arc<Mutex<Option<WhatsAppBot>>>,
}

impl WhatsAppTool {
    pub fn new() -> Self {
        Self {
            bot: Arc::new(Mutex::new(None)),
        }
    }

    /// Called by WhatsAppHandler after it creates the bot
    pub fn inject_live_bot(bot: Arc<Mutex<Option<WhatsAppBot>>>) -> Self {
        Self { bot }
    }
}

#[async_trait]
impl crate::tools::Tool for WhatsAppTool {
    fn name(&self) -> &str {
        "whatsapp"
    }

    fn description(&self) -> &str {
        "Send messages via WhatsApp (Twilio). \
         Use this to communicate with users through WhatsApp. \
         Note: File sending sends file path reference (Twilio requires public URLs for media). \
         Operations: send_message, send_file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["send_message", "send_file"],
                    "description": "The WhatsApp operation to perform"
                },
                "to": {
                    "type": "string",
                    "description": "WhatsApp number to send to (e.g., +1234567890)",
                },
                "message": {
                    "type": "string",
                    "description": "Message text to send (for send_message)",
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to file (for send_file - sends as text reference)",
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
                let to = args["to"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing to"))?;
                let message = args["message"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

                if let Some(ref bot) = *bot_guard {
                    match bot.send_message(to, message).await {
                        Ok(_) => Ok(crate::tools::ToolResult::success(format!("✅ Message sent to {}", to))),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send message: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("WhatsApp bot not initialized".to_string()))
                }
            }
            "send_file" => {
                let to = args["to"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing to"))?;
                let file_path = args["file_path"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing file_path"))?;
                let caption = args["caption"].as_str();

                if let Some(ref bot) = *bot_guard {
                    match bot.send_file(to, file_path, caption).await {
                        Ok(msg) => Ok(crate::tools::ToolResult::success(msg)),
                        Err(e) => Ok(crate::tools::ToolResult::error(format!("Failed to send file: {}", e))),
                    }
                } else {
                    Ok(crate::tools::ToolResult::error("WhatsApp bot not initialized".to_string()))
                }
            }
            _ => Ok(crate::tools::ToolResult::error(format!("Unknown operation: {}", operation))),
        }
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
    shared_bot: Arc<Mutex<Option<WhatsAppBot>>>,
}

pub struct WhatsAppIntegration;

impl WhatsAppIntegration {
    pub async fn start_webhook_server(
        config: WhatsAppConfig,
        db_path: std::path::PathBuf,
    ) -> Result<()> {
        let port = config.webhook_port.unwrap_or(8080);
        let account_sid = config.account_sid.clone().unwrap_or_default();
        let auth_token = config.auth_token.clone().unwrap_or_default();
        let from_number = config.from_number.clone().unwrap_or_default();
        
        let state = Arc::new(WhatsAppState {
            agents: Arc::new(Mutex::new(HashMap::new())),
            db_path,
            config,
            shared_bot: Arc::new(Mutex::new(None)),
        });

        // Create and inject the bot
        let bot = WhatsAppBot::new(account_sid, auth_token, from_number);
        *state.shared_bot.lock().await = Some(bot);

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
            let cfg = crate::agent::AgentConfig::new(state.db_path.clone())
                .with_session_id(format!("whatsapp_{}", from.replace('+', "")));
            // Create agent with WhatsApp tool injected
            let whatsapp_tool = WhatsAppTool::inject_live_bot(state.shared_bot.clone());
            crate::agent::Agent::new_with_whatsapp(cfg, whatsapp_tool)
                .expect("Failed to create agent")
        });

    // Run with context injection
    let mut context = std::collections::HashMap::new();
    context.insert("platform".to_string(), "whatsapp".to_string());
    context.insert("from".to_string(), from.clone());
    context.insert("phone".to_string(), from.clone());

    match agent.run_with_context(&text, context).await {
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
