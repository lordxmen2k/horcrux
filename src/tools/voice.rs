//! Voice Transcription Tool
//!
//! Transcribes audio files to text using Whisper API or similar services.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use super::{Tool, ToolResult};

/// Voice transcription configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceConfig {
    /// API provider: "openai", "deepgram", "assemblyai"
    pub provider: String,
    /// API key
    pub api_key: String,
    /// API base URL (optional, for custom endpoints)
    pub base_url: Option<String>,
    /// Default language (ISO-639-1 code, e.g., "en", "es")
    pub language: Option<String>,
    /// Model to use (e.g., "whisper-1" for OpenAI)
    pub model: Option<String>,
}

impl VoiceConfig {
    /// Load from config file
    pub fn from_config() -> Result<Self> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("horcrux").join("config.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                if let Ok(toml_value) = content.parse::<toml::Value>() {
                    if let Some(voice) = toml_value.get("voice") {
                        let provider = voice.get("provider")
                            .and_then(|p| p.as_str())
                            .unwrap_or("openai")
                            .to_string();
                        
                        let api_key = voice.get("api_key")
                            .and_then(|k| k.as_str())
                            .unwrap_or("")
                            .to_string();
                        
                        let base_url = voice.get("base_url")
                            .and_then(|u| u.as_str())
                            .map(String::from);
                        
                        let language = voice.get("language")
                            .and_then(|l| l.as_str())
                            .map(String::from);
                        
                        let model = voice.get("model")
                            .and_then(|m| m.as_str())
                            .map(String::from);

                        return Ok(Self {
                            provider,
                            api_key,
                            base_url,
                            language,
                            model,
                        });
                    }
                }
            }
        }

        // Try environment variables as fallback
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            return Ok(Self {
                provider: "openai".to_string(),
                api_key,
                base_url: None,
                language: None,
                model: Some("whisper-1".to_string()),
            });
        }

        Err(anyhow::anyhow!("Voice transcription not configured. Set up config.toml or OPENAI_API_KEY"))
    }
}

/// Voice transcription tool
pub struct VoiceTranscriptionTool {
    client: reqwest::Client,
    config: Option<VoiceConfig>,
}

impl VoiceTranscriptionTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 min for large files
            .build()
            .expect("Failed to build HTTP client");

        let config = VoiceConfig::from_config().ok();

        Self { client, config }
    }

    /// Transcribe audio file using OpenAI Whisper
    async fn transcribe_openai(&self, file_path: &str, language: Option<&str>) -> Result<String> {
        let config = self.config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Voice transcription not configured"))?;

        if config.api_key.is_empty() {
            return Err(anyhow::anyhow!("OpenAI API key not configured"));
        }

        let file_data = tokio::fs::read(file_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read audio file: {}", e))?;

        let filename = Path::new(file_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let base_url = config.base_url.as_deref()
            .unwrap_or("https://api.openai.com/v1");

        // Build multipart form
        let mut form = reqwest::multipart::Form::new()
            .part("file", reqwest::multipart::Part::bytes(file_data)
                .file_name(filename))
            .text("model", config.model.clone().unwrap_or_else(|| "whisper-1".to_string()));

        if let Some(lang) = language.or(config.language.as_deref()) {
            form = form.text("language", lang.to_string());
        }

        let response = self.client
            .post(format!("{}/audio/transcriptions", base_url))
            .bearer_auth(&config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("API error: {}", error_text));
        }

        let result: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

        let text = result.get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("No transcription in response"))?;

        Ok(text.to_string())
    }

    /// Transcribe audio file using Deepgram
    async fn transcribe_deepgram(&self, file_path: &str, language: Option<&str>) -> Result<String> {
        let config = self.config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Voice transcription not configured"))?;

        if config.api_key.is_empty() {
            return Err(anyhow::anyhow!("Deepgram API key not configured"));
        }

        let file_data = tokio::fs::read(file_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read audio file: {}", e))?;

        let url = format!(
            "https://api.deepgram.com/v1/listen?model={}&smart_format=true",
            config.model.as_deref().unwrap_or("nova-2")
        );

        let url = if let Some(lang) = language.or(config.language.as_deref()) {
            format!("{}&language={}", url, lang)
        } else {
            format!("{}&detect_language=true", url)
        };

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Token {}", config.api_key))
            .header("Content-Type", "audio/wav") // Let Deepgram auto-detect
            .body(file_data)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("API error: {}", error_text));
        }

        let result: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

        // Parse Deepgram response
        let text = result
            .get("results")
            .and_then(|r| r.get("channels"))
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("alternatives"))
            .and_then(|a| a.get(0))
            .and_then(|a| a.get("transcript"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("No transcription in response"))?;

        Ok(text.to_string())
    }

    /// Detect audio format from file extension
    fn detect_audio_format(&self, file_path: &str) -> &str {
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "mp3" => "mp3",
            "mp4" | "m4a" => "mp4",
            "wav" => "wav",
            "ogg" => "ogg",
            "webm" => "webm",
            "flac" => "flac",
            _ => "wav", // default
        }
    }
}

#[async_trait]
impl Tool for VoiceTranscriptionTool {
    fn name(&self) -> &str {
        "transcribe_audio"
    }

    fn description(&self) -> &str {
        "Transcribe audio files (voice messages, recordings) to text. \
         Supports MP3, WAV, OGG, M4A, and other formats. \
         Uses OpenAI Whisper, Deepgram, or other configured providers."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the audio file to transcribe"
                },
                "language": {
                    "type": "string",
                    "description": "Optional language code (ISO-639-1, e.g., 'en', 'es', 'fr'). Auto-detected if not specified."
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        // Check if configured
        if self.config.is_none() {
            return Ok(ToolResult::error(
                "Voice transcription not configured. Add to ~/.horcrux/config.toml:\n\n[voice]\nprovider = 'openai'\napi_key = 'your-api-key'"
            ));
        }

        let file_path = args["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;
        let language = args["language"].as_str();

        // Check if file exists
        if !Path::new(file_path).exists() {
            return Ok(ToolResult::error(format!("Audio file not found: {}", file_path)));
        }

        let config = self.config.as_ref().unwrap();

        let result = match config.provider.as_str() {
            "openai" => self.transcribe_openai(file_path, language).await,
            "deepgram" => self.transcribe_deepgram(file_path, language).await,
            _ => Err(anyhow::anyhow!("Unknown provider: {}", config.provider)),
        };

        match result {
            Ok(text) => Ok(ToolResult::success(format!(
                "🎤 Transcription:\n\n{}\n\n(Audio file: {})",
                text,
                file_path
            ))),
            Err(e) => Ok(ToolResult::error(format!("Transcription failed: {}", e))),
        }
    }
}
