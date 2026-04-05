//! Image Search Tool - Find and download images from Pixabay or Unsplash
//!
//! Returns actual image data that can be sent directly to platforms

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;

pub struct ImageSearchTool {
    client: reqwest::Client,
    config: crate::config::ImageConfig,
}

/// Image search result
#[derive(Debug, Clone)]
struct ImageResult {
    id: String,
    url: String,
    local_path: String,
    title: String,
    source: String,
}

impl ImageSearchTool {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("HorcruxAgent/1.0 (image search bot)")
            .build()
            .expect("Failed to build HTTP client");
            
        let config = crate::config::Config::load()
            .map(|c| c.images)
            .unwrap_or_default();
            
        Ok(Self { client, config })
    }
    
    /// Download image from URL and save to temp file
    /// Validates that the content is actually an image, not an error page
    async fn download_image(&self, url: &str, filename: &str) -> anyhow::Result<String> {
        let response = self.client
            .get(url)
            .header("Accept", "image/jpeg,image/png,image/gif,image/webp,image/*")
            .send()
            .await?;
        
        // Check HTTP status
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
        }
        
        let bytes = response.bytes().await?;
        
        // Validate: must be reasonable size (> 10KB to avoid tiny/error pages)
        if bytes.len() < 10_000 {
            if bytes.starts_with(b"<!DOCTYPE") || bytes.starts_with(b"<html") {
                return Err(anyhow::anyhow!("Downloaded HTML error page instead of image"));
            }
            return Err(anyhow::anyhow!("Downloaded content too small ({} bytes)", bytes.len()));
        }
        
        // Check for image magic bytes
        let is_valid_image = match bytes.get(0..4) {
            Some(b"\xFF\xD8\xFF\xE0") => true, // JPEG
            Some(b"\xFF\xD8\xFF\xE1") => true, // JPEG EXIF
            Some(b"\x89PNG")          => true, // PNG
            Some(b"GIF8")             => true, // GIF
            Some(b"RIFF")             => true, // WebP
            _ => false,
        };
        
        if !is_valid_image {
            let is_webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
            if !is_webp {
                return Err(anyhow::anyhow!("Downloaded content is not a valid image"));
            }
        }
        
        // Save to temp directory
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(filename);
        tokio::fs::write(&path, &bytes).await?;
        
        Ok(path.to_string_lossy().to_string())
    }
    
    /// Search Unsplash for images
    async fn search_unsplash(&self, query: &str, api_key: &str, count: usize) -> anyhow::Result<Vec<ImageResult>> {
        let url = format!(
            "https://api.unsplash.com/search/photos?query={}&per_page={}",
            urlencoding::encode(query),
            count.min(10)
        );
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Client-ID {}", api_key))
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Unsplash API error: {} - {}", status, text));
        }
        
        let json: Value = response.json().await?;
        let mut results = Vec::new();
        
        if let Some(photos) = json["results"].as_array() {
            for (i, photo) in photos.iter().enumerate() {
                if let Some(img_url) = photo["urls"]["regular"].as_str() {
                    // Add delay between downloads
                    if i > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }
                    
                    match self.download_image(img_url, &format!("horcrux_img_{}.jpg", i)).await {
                        Ok(path) => {
                            results.push(ImageResult {
                                id: photo["id"].as_str().unwrap_or("unknown").to_string(),
                                url: img_url.to_string(),
                                local_path: path,
                                title: photo["alt_description"]
                                    .as_str()
                                    .or_else(|| photo["description"].as_str())
                                    .unwrap_or("Image")
                                    .to_string(),
                                source: "Unsplash".to_string(),
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to download Unsplash image {}: {}", i, e);
                        }
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    /// Search Pixabay for images
    async fn search_pixabay(&self, query: &str, api_key: &str, count: usize) -> anyhow::Result<Vec<ImageResult>> {
        let url = format!(
            "https://pixabay.com/api/?key={}&q={}&image_type=photo&per_page={}&safesearch=true",
            api_key,
            urlencoding::encode(query),
            count.min(20)
        );
        
        let response = self.client
            .get(&url)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Pixabay API error: {} - {}", status, text));
        }
        
        let json: Value = response.json().await?;
        let mut results = Vec::new();
        
        if let Some(hits) = json["hits"].as_array() {
            for (i, hit) in hits.iter().enumerate() {
                if let Some(img_url) = hit["webformatURL"].as_str() {
                    // Add delay between downloads
                    if i > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }
                    
                    match self.download_image(img_url, &format!("horcrux_img_{}.jpg", i)).await {
                        Ok(path) => {
                            results.push(ImageResult {
                                id: hit["id"].as_i64().map(|i| i.to_string()).unwrap_or_else(|| i.to_string()),
                                url: img_url.to_string(),
                                local_path: path,
                                title: hit["tags"].as_str().unwrap_or("Image").to_string(),
                                source: "Pixabay".to_string(),
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to download Pixabay image {}: {}", i, e);
                        }
                    }
                }
            }
        }
        
        Ok(results)
    }
}

#[async_trait]
impl Tool for ImageSearchTool {
    fn name(&self) -> &str {
        "image_search"
    }

    fn description(&self) -> &str {
        "Search and download images from Unsplash or Pixabay. \
         Returns ready-to-send image files with [IMAGE_X] file=... tags."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What image to search for (e.g., 'golden retriever', 'sunset beach')"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of images (1-5, default: 3)",
                    "default": 3,
                    "minimum": 1,
                    "maximum": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        // Check if image search is configured
        if !self.config.is_configured() {
            return Ok(ToolResult::success(
                "Image search is not configured.\n\n\
                Please add an image provider API key to your config:\n\
                1. Edit ~/.horcrux/config.toml\n\
                2. Add:\n\n\
                [images]\n\
                provider = \"unsplash\"  # or \"pixabay\"\n\
                api_key = \"your_api_key_here\"\n\n\
                Get a free API key:\n\
                - Unsplash: https://unsplash.com/developers (50 req/hour)\n\
                - Pixabay: https://pixabay.com/api/docs (100 req/minute)".to_string()
            ));
        }
        
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query parameter"))?;
        
        let count = args["count"].as_i64().unwrap_or(3) as usize;
        
        let api_key = self.config.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No API key configured"))?;
        
        // Search based on provider
        let provider = self.config.provider();
        let results = match provider {
            "unsplash" => self.search_unsplash(query, api_key, count).await?,
            "pixabay" => self.search_pixabay(query, api_key, count).await?,
            other => return Ok(ToolResult::error(format!(
                "Unknown image provider: {}. Use 'unsplash' or 'pixabay'.", other
            ))),
        };
        
        if results.is_empty() {
            return Ok(ToolResult::error(
                "No images found. Try a different search query.".to_string()
            ));
        }
        
        // Format results with [IMAGE_X] tags for Telegram
        // Verify each file exists before including it
        let mut output = format!("Found {} image(s) for '{}':\n\n", results.len(), query);
        
        for (i, img) in results.iter().enumerate() {
            let path = std::path::Path::new(&img.local_path);
            if !path.exists() {
                println!("⚠️ Image file not found at: {}", img.local_path);
                continue;
            }
            
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            println!("✅ Image {} saved at: {} ({} bytes)", i + 1, img.local_path, size);
            
            output.push_str(&format!(
                "[IMAGE_{}] file=\"{}\"\n",
                i + 1,
                img.local_path,
            ));
        }
        
        if output.trim() == format!("Found {} image(s) for '{}':", results.len(), query) {
            return Ok(ToolResult::error(
                "Images were downloaded but could not be located. Please try again.".to_string()
            ));
        }
        
        output.push_str("\n⚠️ IMPORTANT: Copy the [IMAGE_N] file=... lines above EXACTLY into your response. Do NOT modify the paths.\n");
        
        Ok(ToolResult::success(output))
    }
}
