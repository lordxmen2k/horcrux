//! Image Search Tool - Smart image finding with free sources first
//!
//! Strategy:
//! 1. Try free sources first (no API key needed)
//! 2. If user has paid API keys, use those
//! 3. Guide user to get keys if needed
//! 4. Platform-aware output (direct image for Telegram, URL for CLI)

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;

pub struct ImageSearchTool {
    client: reqwest::Client,
}

impl ImageSearchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Horcrux-Agent/1.0")
            .build()
            .expect("Failed to build HTTP client");
        Self { client }
    }

    /// FREE SOURCE 1: Picsum Photos (completely free, no API key)
    async fn get_picsum_images(&self, query: &str, count: usize) -> Vec<ImageResult> {
        let mut results = Vec::new();
        
        // Picsum doesn't have search, but we can use seeds for consistency
        // Generate different seeds from the query
        let seeds: Vec<u64> = (1..=count as u64)
            .map(|i| {
                let mut hash = 0u64;
                for (j, byte) in query.bytes().enumerate() {
                    hash = hash.wrapping_add((byte as u64).wrapping_mul(i).wrapping_add(j as u64));
                }
                hash
            })
            .collect();
        
        for (i, seed) in seeds.iter().enumerate() {
            // Different sizes for variety
            let (width, height) = match i % 3 {
                0 => (800, 600),
                1 => (1200, 800),
                _ => (600, 600),
            };
            
            results.push(ImageResult {
                id: format!("picsum-{}", seed),
                url: format!("https://picsum.photos/seed/{}/{}/{}", seed, width, height),
                thumbnail: format!("https://picsum.photos/seed/{}/200/150", seed),
                title: format!("Random image {} for '{}'", i + 1, query),
                source: "Picsum Photos (Free)".to_string(),
                author: "Random from Picsum".to_string(),
                source_url: format!("https://picsum.photos/seed/{}", seed),
            });
        }
        
        results
    }

    /// FREE SOURCE 2: Wikimedia Commons (Creative Commons images)
    async fn search_wikimedia(&self, query: &str, count: usize) -> anyhow::Result<Vec<ImageResult>> {
        let search_url = format!(
            "https://commons.wikimedia.org/w/api.php?action=query&list=search&srsearch={}&srnamespace=6&srlimit={}&format=json&origin=*",
            urlencoding::encode(query),
            count
        );

        let response = match self.client.get(&search_url).send().await {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()),
        };

        let body = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };

        let json: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };

        let mut results = Vec::new();
        
        if let Some(search_results) = json["query"]["search"].as_array() {
            for item in search_results.iter().take(count) {
                if let Some(title) = item["title"].as_str() {
                    // Extract filename from "File:Example.jpg"
                    let filename = title.strip_prefix("File:").unwrap_or(title);
                    let encoded = urlencoding::encode(filename);
                    
                    results.push(ImageResult {
                        id: filename.to_string(),
                        url: format!("https://commons.wikimedia.org/wiki/Special:FilePath/{}", encoded),
                        thumbnail: format!("https://commons.wikimedia.org/wiki/Special:FilePath/{}?width=300", encoded),
                        title: item["snippet"].as_str().map(|s| s.replace("<span class='searchmatch'>", "").replace("</span>", "")).unwrap_or_else(|| filename.to_string()),
                        source: "Wikimedia Commons (Free/Creative Commons)".to_string(),
                        author: "See image page for attribution".to_string(),
                        source_url: format!("https://commons.wikimedia.org/wiki/File:{}", encoded),
                    });
                }
            }
        }

        Ok(results)
    }

    /// FREE SOURCE 3: Direct search URLs (Google Images, Bing, etc.)
    fn get_search_links(&self, query: &str) -> String {
        format!(
            "Free image search links (open in browser to download):\n\
             - Google Images: https://www.google.com/search?tbm=isch&q={}\n\
             - Bing Images: https://www.bing.com/images/search?q={}\n\
             - DuckDuckGo: https://duckduckgo.com/?iax=images&ia=images&q={}\n\
             - Wikimedia Commons: https://commons.wikimedia.org/w/index.php?search={}&title=Special:MediaSearch&type=image",
            urlencoding::encode(query),
            urlencoding::encode(query),
            urlencoding::encode(query),
            urlencoding::encode(query)
        )
    }

    /// PAID SOURCE: Unsplash (requires API key)
    async fn search_unsplash(&self, query: &str, count: usize) -> anyhow::Result<Vec<ImageResult>> {
        let api_key = std::env::var("UNSPLASH_ACCESS_KEY")
            .map_err(|_| anyhow::anyhow!("UNSPLASH_ACCESS_KEY not set"))?;

        let url = format!(
            "https://api.unsplash.com/search/photos?query={}&per_page={}&client_id={}",
            urlencoding::encode(query),
            count,
            api_key
        );

        let response = self.client.get(&url).send().await?;
        let body = response.text().await?;
        let json: Value = serde_json::from_str(&body)?;

        let mut results = Vec::new();
        
        if let Some(photos) = json["results"].as_array() {
            for photo in photos.iter().take(count) {
                results.push(ImageResult {
                    id: photo["id"].as_str().unwrap_or("unknown").to_string(),
                    url: photo["urls"]["regular"].as_str().unwrap_or("").to_string(),
                    thumbnail: photo["urls"]["small"].as_str().unwrap_or("").to_string(),
                    title: photo["alt_description"].as_str()
                        .or_else(|| photo["description"].as_str())
                        .unwrap_or("Image")
                        .to_string(),
                    source: "Unsplash".to_string(),
                    author: photo["user"]["name"].as_str().unwrap_or("Unknown").to_string(),
                    source_url: photo["links"]["html"].as_str().unwrap_or("").to_string(),
                });
            }
        }

        Ok(results)
    }

    /// Format results based on platform
    fn format_results(&self, images: &[ImageResult], search_links: &str, platform: &str) -> String {
        match platform {
            "telegram" => {
                // For Telegram, return direct image URLs that can be sent
                let mut output = format!("🖼️ Found {} images:\n\n", images.len());
                for (i, img) in images.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {}\n   📥 {}\n   🔗 {}\n\n",
                        i + 1,
                        img.title,
                        img.url,
                        img.source_url
                    ));
                }
                output.push_str("\n💡 I can send these directly to the chat!");
                output
            }
            _ => {
                // Default format for CLI/Web
                let mut output = format!("🖼️ Found {} images:\n\n", images.len());
                for (i, img) in images.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {}\n   Source: {}\n   By: {}\n   📥 Direct URL: {}\n   🔗 Source: {}\n\n",
                        i + 1,
                        img.title,
                        img.source,
                        img.author,
                        img.url,
                        img.source_url
                    ));
                }
                output.push_str(&format!("\n{}\n", search_links));
                output
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ImageResult {
    id: String,
    url: String,
    thumbnail: String,
    title: String,
    source: String,
    author: String,
    source_url: String,
}

#[async_trait]
impl Tool for ImageSearchTool {
    fn name(&self) -> &str {
        "image_search"
    }

    fn description(&self) -> &str {
        "Search for images on the internet. Uses FREE sources first (no API key needed):\n\
         1. Picsum Photos - Random images, completely free\n\
         2. Wikimedia Commons - Creative Commons images\n\
         3. Search links to Google/Bing/DuckDuckGo\n\
         \n\
         If you have API keys, can also use:\n\
         - Unsplash (free tier: 50/hour)\n\
         \n\
         Platform-aware: On Telegram, images can be sent directly."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for (e.g., 'golden retriever', 'mountain sunset')"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of images (1-10, default: 3)",
                    "default": 3,
                    "minimum": 1,
                    "maximum": 10
                },
                "platform": {
                    "type": "string",
                    "enum": ["cli", "telegram", "web", "auto"],
                    "description": "Output format for platform",
                    "default": "auto"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let count = args["count"].as_u64().unwrap_or(3).min(10) as usize;
        let platform = args["platform"].as_str().unwrap_or("auto");

        // Try free sources first
        let mut all_images = Vec::new();
        let mut sources_used = Vec::new();

        // 1. Try Wikimedia Commons
        match self.search_wikimedia(query, count).await {
            Ok(mut images) => {
                if !images.is_empty() {
                    sources_used.push("Wikimedia Commons");
                    all_images.append(&mut images);
                }
            }
            Err(_) => {}
        }

        // 2. Add Picsum images for variety (always works)
        let picsum_count = count.saturating_sub(all_images.len());
        if picsum_count > 0 {
            let mut picsum = self.get_picsum_images(query, picsum_count).await;
            sources_used.push("Picsum Photos");
            all_images.append(&mut picsum);
        }

        // 3. Try Unsplash if user has API key
        if std::env::var("UNSPLASH_ACCESS_KEY").is_ok() {
            match self.search_unsplash(query, count).await {
                Ok(mut images) => {
                    if !images.is_empty() {
                        sources_used.push("Unsplash");
                        all_images.append(&mut images);
                    }
                }
                Err(_) => {}
            }
        }

        // Format results
        let search_links = self.get_search_links(query);
        let output = self.format_results(&all_images, &search_links, platform);

        // Build response
        let mut response = output;
        
        if !std::env::var("UNSPLASH_ACCESS_KEY").is_ok() {
            response.push_str("\n\n");
            response.push_str("💡 **Want better image results?**\n");
            response.push_str("Get a free Unsplash API key (50 requests/hour):\n");
            response.push_str("1. Go to https://unsplash.com/developers\n");
            response.push_str("2. Create an app (free)\n");
            response.push_str("3. Tell me: 'Set UNSPLASH_ACCESS_KEY to xxx'\n");
            response.push_str("I'll save it to your .env file!");
        }

        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_search_tool() {
        let tool = ImageSearchTool::new();
        assert_eq!(tool.name(), "image_search");
    }
}
