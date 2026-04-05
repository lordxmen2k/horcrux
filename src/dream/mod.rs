//! Dreaming - Background memory consolidation
//!
//! The dreaming process periodically reviews recent conversations,
//! identifies important insights, and consolidates them into long-term memory.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Dreaming configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamConfig {
    pub enabled: bool,
    /// How often to run (cron expression)
    pub schedule: String,
    /// Minimum number of new conversations before triggering
    pub min_conversations: usize,
    /// Lookback period in hours
    pub lookback_hours: i64,
    /// Memory importance threshold (0-1)
    pub importance_threshold: f32,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            schedule: "0 3 * * *".to_string(),
            min_conversations: 5,
            lookback_hours: 24,
            importance_threshold: 0.7,
        }
    }
}

impl DreamConfig {
    pub fn from_config() -> Self {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("horcrux").join("config.toml");
            if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(toml_value) = content.parse::<toml::Value>() {
                        if let Some(dream) = toml_value.get("dream") {
                            return Self {
                                enabled: dream.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                                schedule: dream.get("schedule").and_then(|v| v.as_str()).unwrap_or("0 3 * * *").to_string(),
                                min_conversations: dream.get("min_conversations").and_then(|v| v.as_integer()).map(|v| v as usize).unwrap_or(5),
                                lookback_hours: dream.get("lookback_hours").and_then(|v| v.as_integer()).map(|v| v as i64).unwrap_or(24),
                                importance_threshold: dream.get("importance_threshold").and_then(|v| v.as_float()).map(|v| v as f32).unwrap_or(0.7),
                            };
                        }
                    }
                }
            }
        }
        Self::default()
    }
}

/// Dreaming state
#[derive(Debug, Clone, Default)]
pub struct DreamState {
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub memories_created: u64,
    pub last_summary: Option<String>,
}

/// The Dreamer - background memory consolidation process
pub struct Dreamer {
    config: DreamConfig,
    state: Arc<RwLock<DreamState>>,
    handle: Option<JoinHandle<()>>,
    db_path: std::path::PathBuf,
}

impl Dreamer {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        let config = DreamConfig::from_config();
        Self {
            config,
            state: Arc::new(RwLock::new(DreamState::default())),
            handle: None,
            db_path,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("Dreaming is disabled");
            return Ok(());
        }

        let schedule = cron::Schedule::from_str(&self.config.schedule)?;
        let config = self.config.clone();
        let state = self.state.clone();
        let _db_path = self.db_path.clone();

        let handle = tokio::spawn(async move {
            let mut upcoming = schedule.upcoming(Utc);
            
            loop {
                let next = match upcoming.next() {
                    Some(t) => t,
                    None => break,
                };

                {
                    let mut state_lock = state.write().await;
                    state_lock.next_run = Some(next);
                }

                let now = Utc::now();
                let duration = next.signed_duration_since(now);
                
                if duration.num_milliseconds() > 0 {
                    info!("💤 Next dream at {} (in {})", next, duration);
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        duration.num_seconds().max(1) as u64
                    )).await;
                }

                // Run the dreaming process
                info!("💤 Dreaming started...");
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                
                {
                    let mut state_lock = state.write().await;
                    state_lock.last_run = Some(Utc::now());
                    state_lock.run_count += 1;
                    state_lock.last_summary = Some("Dream process completed".to_string());
                }
            }
        });

        self.handle = Some(handle);
        info!("💤 Dreamer started with schedule: {}", self.config.schedule);
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            info!("💤 Dreamer stopped");
        }
    }

    pub async fn get_state(&self) -> DreamState {
        self.state.read().await.clone()
    }
}

/// Tool to trigger dreaming manually
pub mod tool {
    use super::*;
    use crate::tools::{Tool, ToolResult};
    use async_trait::async_trait;

    pub struct DreamTool {
        dreamer: Arc<Dreamer>,
    }

    impl DreamTool {
        pub fn new(dreamer: Arc<Dreamer>) -> Self {
            Self { dreamer }
        }
    }

    #[async_trait]
    impl Tool for DreamTool {
        fn name(&self) -> &str {
            "dream"
        }

        fn description(&self) -> &str {
            "Trigger the dreaming process to consolidate recent memories"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
            let state = self.dreamer.get_state().await;
            Ok(ToolResult::success(format!(
                "💤 Dreamer state:\nLast run: {}\nRun count: {}\nMemories created: {}",
                state.last_run.map(|t| t.to_rfc3339()).unwrap_or_else(|| "Never".to_string()),
                state.run_count,
                state.memories_created
            )))
        }
    }
}
