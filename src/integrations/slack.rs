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
