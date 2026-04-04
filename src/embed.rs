use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Embedding provider config, read from env vars (same as OpenClaw's memorySearch config)
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub batch_size: usize,
}

impl EmbedConfig {
    pub fn from_env() -> Self {
        Self {
            model: std::env::var("HORCRUX_EMBED_MODEL")
                .or_else(|_| std::env::var("HOARD_EMBED_MODEL")) // backward compat
                .or_else(|_| std::env::var("CLAW_EMBED_MODEL")) // older compat
                .unwrap_or_else(|_| "text-embedding-3-small".into()),
            base_url: std::env::var("HORCRUX_EMBED_URL")
                .or_else(|_| std::env::var("HOARD_EMBED_URL")) // backward compat
                .or_else(|_| std::env::var("CLAW_EMBED_URL")) // older compat
                .or_else(|_| std::env::var("OLLAMA_HOST").map(|h| format!("{}/v1", h)))
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            api_key: std::env::var("HORCRUX_EMBED_API_KEY")
                .or_else(|_| std::env::var("HOARD_EMBED_API_KEY")) // backward compat
                .or_else(|_| std::env::var("CLAW_EMBED_API_KEY")) // older compat
                .or_else(|_| std::env::var("OPENAI_API_KEY"))
                .unwrap_or_else(|_| "ollama".into()), // Ollama ignores the key
            batch_size: std::env::var("HORCRUX_EMBED_BATCH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(64),
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
    index: usize,
}

pub struct EmbedClient {
    config: EmbedConfig,
    client: reqwest::blocking::Client,
}

impl EmbedClient {
    pub fn new(config: EmbedConfig) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }

    pub fn model_name(&self) -> &str {
        &self.config.model
    }

    /// Embed a batch of texts. Returns embeddings in the same order as input.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/embeddings", self.config.base_url.trim_end_matches('/'));

        let response = self.client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&EmbedRequest {
                input: texts.to_vec(),
                model: self.config.model.clone(),
            })
            .send()
            .map_err(|e| anyhow!("Embedding request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow!("Embedding API error {}: {}", status, body));
        }

        let mut resp: EmbedResponse = response.json()
            .map_err(|e| anyhow!("Failed to parse embedding response: {}", e))?;

        // Sort by index to ensure order matches input
        resp.data.sort_by_key(|d| d.index);

        Ok(resp.data.into_iter().map(|d| d.embedding).collect())
    }

    /// Embed texts in batches, respecting batch_size limit.
    pub fn embed_all(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for batch in texts.chunks(self.config.batch_size) {
            let embeddings = self.embed_batch(&batch.to_vec())?;
            if embeddings.len() != batch.len() {
                return Err(anyhow!(
                    "API returned {} embeddings but {} were requested",
                    embeddings.len(), batch.len()
                ));
            }
            results.extend(embeddings);
        }
        Ok(results)
    }

    /// Embed a single text (with result caching support).
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text.to_string()])?;
        embeddings.into_iter().next().ok_or_else(|| anyhow!("No embedding returned"))
    }
}

/// Deterministic text hash for the embedding cache.
pub fn text_hash(text: &str, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"|");
    hasher.update(text.as_bytes());
    hex::encode(&hasher.finalize()[..8]) // 16 hex chars is enough
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_hash_deterministic() {
        let text = "Hello, world!";
        let model = "text-embedding-3-small";
        
        let hash1 = text_hash(text, model);
        let hash2 = text_hash(text, model);
        
        assert_eq!(hash1, hash2, "Hash should be deterministic");
        assert_eq!(hash1.len(), 16, "Hash should be 16 hex characters (8 bytes)");
    }

    #[test]
    fn test_text_hash_different_inputs() {
        let hash1 = text_hash("Hello", "model1");
        let hash2 = text_hash("Hello", "model2");
        let hash3 = text_hash("World", "model1");
        
        assert_ne!(hash1, hash2, "Different models should produce different hashes");
        assert_ne!(hash1, hash3, "Different texts should produce different hashes");
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001, "Identical vectors should have similarity 1.0");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 0.0001, "Opposite vectors should have similarity -1.0");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001, "Orthogonal vectors should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Empty vectors should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0];
        
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Different length vectors should return 0.0");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Zero vector should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_typical() {
        // Typical similar vectors
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.1, 1.9, 3.1];
        
        let sim = cosine_similarity(&a, &b);
        assert!(sim > 0.9 && sim < 1.0, "Similar vectors should have high similarity");
    }

    #[test]
    fn test_embed_config_defaults() {
        // This test might interfere with other tests if run in parallel
        // In a real scenario, we'd use a more isolated approach
        let config = EmbedConfig::from_env();
        
        // Just verify it doesn't panic and has reasonable defaults
        assert!(!config.model.is_empty());
        assert!(!config.base_url.is_empty());
        assert!(config.batch_size > 0);
    }
}