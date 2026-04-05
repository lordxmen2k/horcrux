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
