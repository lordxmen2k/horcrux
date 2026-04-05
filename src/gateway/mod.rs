//! Gateway trait - unified interface for all messaging platforms
//!
//! Each platform (Telegram, Discord, WhatsApp, Slack, Matrix) implements
//! this trait. The send_response() default handles [IMAGE_N] parsing
//! and markdown cleaning for ALL platforms.

use async_trait::async_trait;
use anyhow::Result;

/// Unified messaging gateway interface
#[async_trait]
pub trait Gateway: Send + Sync {
    /// Send plain text to a chat/channel
    async fn send_text(&self, chat_id: &str, text: &str) -> Result<()>;

    /// Send an image file to a chat/channel
    async fn send_image(&self, chat_id: &str, file_path: &str) -> Result<()>;

    /// Send a document/file attachment
    async fn send_file(&self, chat_id: &str, file_path: &str, caption: Option<&str>) -> Result<()> {
        // Default: send as image, platforms can override
        self.send_image(chat_id, file_path).await
    }

    /// Parse and route an agent response — handles [IMAGE_N] tags automatically
    /// This default implementation works for ALL platforms
    async fn send_response(&self, chat_id: &str, response: &str) -> Result<()> {
        let (text, image_paths) = parse_agent_response(response);
        let clean_text = clean_for_platform(&text);

        if !clean_text.trim().is_empty() {
            // Split at platform limit (handled per-platform in send_text)
            self.send_text(chat_id, &clean_text).await?;
        }

        for (i, path) in image_paths.iter().enumerate() {
            let p = std::path::Path::new(path);

            if !p.exists() {
                println!("⚠️ Skipping hallucinated path {}: {}", i + 1, path);
                continue;
            }

            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if size < 5_000 {
                println!("⚠️ Skipping small/corrupt file {}: {} bytes", i + 1, size);
                continue;
            }

            println!("📸 Sending image {} ({} bytes): {}", i + 1, size, path);
            match self.send_image(chat_id, path).await {
                Ok(_) => println!("✅ Image {} sent", i + 1),
                Err(e) => println!("❌ Image {} failed: {}", i + 1, e),
            }
        }

        Ok(())
    }
}

/// Parse [IMAGE_N] tags from agent response
/// Returns (text_without_tags, vec_of_file_paths)
pub fn parse_agent_response(response: &str) -> (String, Vec<String>) {
    let mut text_parts: Vec<String> = Vec::new();
    let mut image_paths: Vec<String> = Vec::new();
    let mut last_end = 0;

    // Handles both quoted: file="C:\path\img.jpg" and unquoted: file=/tmp/img.jpg
    let re = regex::Regex::new(r#"\[IMAGE_\d+\]\s*file="?([^"\s\n\[]+)"?"#).unwrap();

    for cap in re.captures_iter(response) {
        let full = cap.get(0).unwrap();
        let before = response[last_end..full.start()].trim();
        if !before.is_empty() {
            text_parts.push(before.to_string());
        }
        if let Some(path) = cap.get(1) {
            image_paths.push(path.as_str().to_string());
        }
        last_end = full.end();
    }

    let remaining = response[last_end..].trim();
    if !remaining.is_empty() {
        text_parts.push(remaining.to_string());
    }

    (text_parts.join("\n"), image_paths)
}

/// Clean markdown for plain-text platforms (Telegram plain, WhatsApp, SMS)
/// Converts markdown to readable plain text with unicode bullets
pub fn clean_for_platform(text: &str) -> String {
    let mut output = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Headers → plain text (keep content, remove #)
        let content = if trimmed.starts_with("### ") {
            trimmed[4..].to_string()
        } else if trimmed.starts_with("## ") {
            trimmed[3..].to_string()
        } else if trimmed.starts_with("# ") {
            trimmed[2..].to_string()
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            // Markdown bullets → unicode bullet
            format!("• {}", &trimmed[2..])
        } else {
            // Remove bold/italic markers
            trimmed
                .replace("**", "")
                .replace("__", "")
                .replace('*', "")
                .replace('`', "")
        };

        output.push_str(&content);
        output.push('\n');
    }

    output.trim().to_string()
}

/// Convert markdown to Telegram HTML format
/// Use this when you want bold/italic in Telegram
pub fn markdown_to_telegram_html(text: &str) -> String {
    // Simple conversion - for production use a proper markdown parser
    let mut result = text.to_string();
    
    // Bold: **text** → <b>text</b>
    let bold_re = regex::Regex::new(r"\*\*(.+?)\*\*").unwrap();
    result = bold_re.replace_all(&result, "<b>$1</b>").to_string();
    
    // Code: `text` → <code>text</code>
    let code_re = regex::Regex::new(r"`([^`]+)`").unwrap();
    result = code_re.replace_all(&result, "<code>$1</code>").to_string();
    
    // Headers: ## text → <b>text</b>
    let header_re = regex::Regex::new(r"(?m)^#{1,3}\s+(.+)$").unwrap();
    result = header_re.replace_all(&result, "<b>$1</b>").to_string();
    
    result
}

/// Sanitize agent output — remove leaked tool call JSON
pub fn sanitize_agent_output(text: &str) -> String {
    use regex::Regex;
    let mut result = text.to_string();

    // Remove JSON tool call objects: {"name": "tool", "arguments": {...}}
    if let Ok(re) = Regex::new(r#"\{[^{}]*"name"\s*:\s*"[^"]+"\s*,\s*"arguments"[^{}]*\}"#) {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove XML tool call tags
    if let Ok(re) = Regex::new(r"<tool_call>[\s\S]*?</tool_call>") {
        result = re.replace_all(&result, "").to_string();
    }

    // Clean up resulting empty lines
    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split long text at natural boundaries for platform character limits
pub fn split_message(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > max_chars {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}
