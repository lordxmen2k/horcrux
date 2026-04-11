//! Configuration management - Load/save settings from config.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// LLM provider configuration
    #[serde(default)]
    pub llm: LlmConfig,
    
    /// Image search provider configuration
    #[serde(default)]
    pub images: ImageConfig,
    
    /// Telegram bot configuration
    #[serde(default)]
    pub telegram: TelegramConfig,
    
    /// Discord bot configuration
    #[serde(default)]
    pub discord: DiscordPlatformConfig,
    
    /// WhatsApp configuration
    #[serde(default)]
    pub whatsapp: WhatsAppPlatformConfig,
    
    /// Slack configuration
    #[serde(default)]
    pub slack: SlackPlatformConfig,
    
    /// Matrix configuration
    #[serde(default)]
    pub matrix: MatrixPlatformConfig,
    
    /// Agent behavior settings
    #[serde(default)]
    pub agent: AgentBehaviorConfig,
    
    /// Web search configuration
    #[serde(default)]
    pub web_search: WebSearchConfig,
    
    /// Vision/AI image analysis configuration
    #[serde(default)]
    pub vision: VisionConfig,
}

/// LLM configuration section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub max_tokens: Option<i32>,
}

fn default_temperature() -> f32 {
    0.6
}

/// Image search provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageConfig {
    /// Provider: "unsplash", "pixabay", or "pexels"
    pub provider: Option<String>,
    /// API key for the chosen provider
    pub api_key: Option<String>,
}

impl ImageConfig {
    /// Check if image search is properly configured
    pub fn is_configured(&self) -> bool {
        self.provider.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
            && self.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }
    
    /// Get provider name or default
    pub fn provider(&self) -> &str {
        self.provider.as_deref().unwrap_or("unsplash")
    }
}

/// Telegram configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    pub bot_token: Option<String>,
    pub chat_id: Option<i64>,
    pub enabled: bool,
}

/// Discord platform configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordPlatformConfig {
    pub bot_token: Option<String>,
    pub enabled: bool,
    pub allowed_channels: Vec<u64>,
    pub command_prefix: Option<String>,
}

/// WhatsApp platform configuration (via Twilio)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppPlatformConfig {
    pub account_sid: Option<String>,
    pub auth_token: Option<String>,
    pub from_number: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

/// Slack platform configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackPlatformConfig {
    pub bot_token: Option<String>,
    pub signing_secret: Option<String>,
    pub webhook_port: Option<u16>,
    pub enabled: bool,
}

/// Matrix platform configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatrixPlatformConfig {
    pub homeserver: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
}

/// Agent behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBehaviorConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_context_messages")]
    pub max_context_messages: usize,
}

/// Web search configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSearchConfig {
    /// Provider: "tavily", "serper", or "brave"
    pub provider: Option<String>,
    /// API key for the chosen provider
    pub api_key: Option<String>,
}

impl WebSearchConfig {
    /// Check if web search is properly configured
    pub fn is_configured(&self) -> bool {
        self.provider.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
            && self.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }
    
    /// Get provider name or default
    pub fn provider(&self) -> &str {
        self.provider.as_deref().unwrap_or("tavily")
    }
}

/// Vision/AI image analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisionConfig {
    /// Provider: "openai", "anthropic", or "ollama"
    pub provider: Option<String>,
    /// API key for the chosen provider (not needed for ollama)
    pub api_key: Option<String>,
    /// Model to use (optional, uses provider default if not set)
    pub model: Option<String>,
    /// Base URL for API (optional, uses provider default if not set)
    pub base_url: Option<String>,
}

impl VisionConfig {
    /// Check if vision is properly configured
    pub fn is_configured(&self) -> bool {
        let has_provider = self.provider.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
        let is_ollama = self.provider.as_deref().map(|p| p == "ollama").unwrap_or(false);
        // Ollama doesn't need an API key, others do
        has_provider && (is_ollama || self.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false))
    }
    
    /// Get provider name or default
    pub fn provider(&self) -> &str {
        self.provider.as_deref().unwrap_or("openai")
    }
}

impl Default for AgentBehaviorConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_context_messages: default_context_messages(),
        }
    }
}

fn default_max_iterations() -> usize {
    15
}

fn default_context_messages() -> usize {
    30
}

impl Config {
    /// Get the config directory path
    pub fn config_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".horcrux")
    }
    
    /// Get the config file path
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }
    
    /// Load configuration from file, or create default if not exists
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {:?}", path))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {:?}", path))?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }
    
    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create config directory {:?}", dir))?;
        }
        
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;
        
        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("Failed to set permissions on {:?}", path))?;
        }
        
        Ok(())
    }
    
    /// Check if configuration file exists
    pub fn exists() -> bool {
        Self::config_path().exists()
    }
    
    /// Get configuration as a string for display
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize config")
    }
}

/// Image provider presets for setup wizard
#[derive(Debug, Clone)]
pub struct ImageProviderPreset {
    pub name: &'static str,
    pub key_name: &'static str,
    pub signup_url: &'static str,
    pub rate_limit: &'static str,
    pub description: &'static str,
}

pub const IMAGE_PROVIDER_PRESETS: &[ImageProviderPreset] = &[
    ImageProviderPreset {
        name: "unsplash",
        key_name: "Unsplash",
        signup_url: "https://unsplash.com/developers",
        rate_limit: "50 requests/hour",
        description: "Beautiful, curated photos. Great quality, requires signup.",
    },
    ImageProviderPreset {
        name: "pixabay",
        key_name: "Pixabay",
        signup_url: "https://pixabay.com/api/docs",
        rate_limit: "100 requests/minute",
        description: "Huge library, generous limits, instant key approval.",
    },
    ImageProviderPreset {
        name: "pexels",
        key_name: "Pexels",
        signup_url: "https://www.pexels.com/api",
        rate_limit: "200 requests/hour",
        description: "High quality stock photos, easy API.",
    },
];
