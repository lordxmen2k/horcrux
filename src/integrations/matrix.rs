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

// Note: Matrix support requires the matrix-sdk crate
// This is a placeholder implementation that compiles without it
// To enable full Matrix support, add to Cargo.toml:
// matrix-sdk = { version = "0.7", features = ["rustls"] }

pub struct MatrixPlaceholderGateway;

#[async_trait]
impl Gateway for MatrixPlaceholderGateway {
    async fn send_text(&self, _room_id: &str, _text: &str) -> Result<()> {
        println!("⚠️ Matrix support not compiled in. Add matrix-sdk to Cargo.toml to enable.");
        Ok(())
    }

    async fn send_image(&self, _room_id: &str, _file_path: &str) -> Result<()> {
        println!("⚠️ Matrix support not compiled in. Add matrix-sdk to Cargo.toml to enable.");
        Ok(())
    }
}

pub struct MatrixIntegration;

impl MatrixIntegration {
    pub async fn start(
        config: MatrixConfig,
        _db_path: std::path::PathBuf,
    ) -> Result<()> {
        if !config.enabled {
            println!("ℹ️ Matrix integration is disabled in config");
            return Ok(());
        }

        let homeserver = config.homeserver.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix homeserver not configured"))?;
        let username = config.username.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Matrix username not configured"))?;

        println!("⚠️ Matrix integration is a placeholder.");
        println!("   To enable full Matrix support, add this to Cargo.toml:");
        println!("   matrix-sdk = {{ version = \"0.7\", features = [\"rustls\"] }}");
        println!("   Configured for: {} on {}", username, homeserver);

        // Full implementation would use matrix-sdk here
        // See the prompt file for the complete implementation

        Ok(())
    }
}
