//! Messaging Platform Integrations
//!
//! Connect your agent to multiple platforms:
//! - Telegram Bot
//! - Discord Bot  
//! - WhatsApp (via QR/phone)
//! - Slack
//! - Matrix
//! - Webhook (generic HTTP)

pub mod discord;
pub mod matrix;
pub mod slack;
pub mod telegram;
pub mod webhook;
pub mod whatsapp;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for all integrations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationsConfig {
    pub telegram: Option<telegram::TelegramConfig>,
    pub discord: Option<discord::DiscordConfig>,
    pub whatsapp: Option<whatsapp::WhatsAppConfig>,
    pub slack: Option<slack::SlackConfig>,
    pub matrix: Option<matrix::MatrixConfig>,
    pub webhook: Option<webhook::WebhookConfig>,
}

/// Manager for all active integrations
pub struct IntegrationManager {
    integrations: HashMap<String, Box<dyn Integration>>,
}

impl IntegrationManager {
    pub fn new() -> Self {
        Self {
            integrations: HashMap::new(),
        }
    }

    pub async fn initialize(&mut self, config: &IntegrationsConfig, agent_handler: AgentHandler) -> Result<()> {
        // Initialize Telegram
        if let Some(cfg) = &config.telegram {
            if cfg.enabled {
                match telegram::TelegramIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("telegram".into(), Box::new(integration));
                        println!("✅ Telegram bot started");
                    }
                    Err(e) => println!("⚠️  Telegram failed: {}", e),
                }
            }
        }

        // Initialize Discord
        if let Some(cfg) = &config.discord {
            if cfg.enabled {
                match discord::DiscordIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("discord".into(), Box::new(integration));
                        println!("✅ Discord bot started");
                    }
                    Err(e) => println!("⚠️  Discord failed: {}", e),
                }
            }
        }

        // Initialize WhatsApp
        if let Some(cfg) = &config.whatsapp {
            if cfg.enabled {
                match whatsapp::WhatsAppIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("whatsapp".into(), Box::new(integration));
                        println!("✅ WhatsApp integration started");
                    }
                    Err(e) => println!("⚠️  WhatsApp failed: {}", e),
                }
            }
        }

        // Initialize Slack
        if let Some(cfg) = &config.slack {
            if cfg.enabled {
                match slack::SlackIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("slack".into(), Box::new(integration));
                        println!("✅ Slack bot started");
                    }
                    Err(e) => println!("⚠️  Slack failed: {}", e),
                }
            }
        }

        // Initialize Matrix
        if let Some(cfg) = &config.matrix {
            if cfg.enabled {
                match matrix::MatrixIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("matrix".into(), Box::new(integration));
                        println!("✅ Matrix bot started");
                    }
                    Err(e) => println!("⚠️  Matrix failed: {}", e),
                }
            }
        }

        // Initialize Webhook
        if let Some(cfg) = &config.webhook {
            if cfg.enabled {
                match webhook::WebhookIntegration::new(cfg.clone(), agent_handler.clone()).await {
                    Ok(integration) => {
                        self.integrations.insert("webhook".into(), Box::new(integration));
                        println!("✅ Webhook server started on port {}", cfg.port);
                    }
                    Err(e) => println!("⚠️  Webhook failed: {}", e),
                }
            }
        }

        let count = self.integrations.len();
        if count > 0 {
            println!("\n🚀 {} integration(s) active", count);
            println!("   You can chat with your agent from any connected platform!\n");
        } else {
            println!("\n⚠️  No integrations are active.");
            println!("   Run 'horcrux setup' to configure messaging platforms.\n");
        }

        Ok(())
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.integrations.contains_key(name)
    }

    pub fn active_count(&self) -> usize {
        self.integrations.len()
    }

    pub fn list_active(&self) -> Vec<&str> {
        self.integrations.keys().map(|s| s.as_str()).collect()
    }
}

/// Trait for all integrations
trait Integration: Send + Sync {
    fn name(&self) -> &str;
    async fn shutdown(&self) -> Result<()>;
}

/// Handler callback for agent processing
pub type AgentHandler = Box<dyn Fn(String) -> futures::future::BoxFuture<'static, String> + Send + Sync>;
