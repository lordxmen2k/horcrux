use crate::db::Db;
use crate::embed::{text_hash, EmbedClient, EmbedConfig};
use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::Path;
use tracing::info;

#[derive(Args, Debug)]
pub struct EmbedArgs {
    /// Collection to embed (default: all)
    #[arg(short, long)]
    pub collection: Option<String>,

    /// Maximum number of chunks to embed (useful for testing)
    #[arg(long)]
    pub limit: Option<usize>,

    /// Model to use (overrides HOARD_EMBED_MODEL env var)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Skip the persistent embedding cache (always call API)
    #[arg(long)]
    pub no_cache: bool,
}

pub fn run(args: EmbedArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    // Check for embedding configuration
    let config = EmbedConfig::from_env();
    let model = args.model.as_ref().unwrap_or(&config.model).clone();

    if std::env::var("HORCRUX_EMBED_URL").is_err()
        && std::env::var("HOARD_EMBED_URL").is_err() // backward compat
        && std::env::var("CLAW_EMBED_URL").is_err() // older compat
        && std::env::var("OLLAMA_HOST").is_err()
        && std::env::var("OPENAI_API_KEY").is_err()
        && std::env::var("HOARD_EMBED_API_KEY").is_err()
    {
        bail!(
            "No embedding provider configured. Set one of:\n\
             - HORCRUX_EMBED_URL (e.g., http://localhost:11434/v1 for Ollama)\n\
             - HOARD_EMBED_URL (deprecated)\n\
             - OLLAMA_HOST (e.g., http://localhost:11434)\n\
             - OPENAI_API_KEY or HORCRUX_EMBED_API_KEY"
        );
    }

    let client = EmbedClient::new(config);
    info!("Using embedding model: {}", model);

    let limit = args.limit.unwrap_or(10000);

    // Get chunks needing embedding
    let chunks = db.chunks_needing_embedding(&model, limit)?;

    if chunks.is_empty() {
        println!("All chunks are already embedded with model '{}'.", model);
        return Ok(());
    }

    println!("Found {} chunks needing embedding...", chunks.len());

    // Process in batches
    let batch_size = 64;
    let mut processed = 0;
    let mut cached = 0;

    for batch in chunks.chunks(batch_size) {
        let ids: Vec<i64> = batch.iter().map(|(id, _, _)| *id).collect();

        // Check persistent cache first
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(batch.len());
        let mut to_fetch: Vec<String> = Vec::new();
        let mut fetch_indices: Vec<usize> = Vec::new();

        if args.no_cache {
            // Skip cache, fetch everything
            for (_, _, text) in batch.iter() {
                to_fetch.push(text.clone());
            }
            fetch_indices = (0..batch.len()).collect();
            // Add empty placeholders
            for _ in 0..batch.len() {
                embeddings.push(Vec::new());
            }
        } else {
            // Check cache for each text
            for (i, (_, _, text)) in batch.iter().enumerate() {
                let hash = text_hash(text, &model);
                match db.get_cached_embedding(&hash, &model)? {
                    Some(emb) => {
                        embeddings.push(emb);
                        cached += 1;
                    }
                    None => {
                        embeddings.push(Vec::new()); // placeholder
                        to_fetch.push(text.clone());
                        fetch_indices.push(i);
                    }
                }
            }
        }

        // Fetch embeddings from API
        if !to_fetch.is_empty() {
            info!("Fetching {} embeddings from API...", to_fetch.len());
            let fetched = client
                .embed_batch(&to_fetch)
                .with_context(|| "Failed to fetch embeddings from API")?;

            // Validate that we got the expected number of embeddings
            if fetched.len() != to_fetch.len() {
                bail!(
                    "API returned {} embeddings but {} were requested",
                    fetched.len(),
                    to_fetch.len()
                );
            }

            // Store in persistent cache and fill in results
            for ((fetch_idx, emb), fetched_text) in fetch_indices
                .iter()
                .zip(fetched.into_iter())
                .zip(to_fetch.iter())
            {
                let hash = text_hash(fetched_text, &model);
                db.set_cached_embedding(&hash, &model, &emb)?;
                embeddings[*fetch_idx] = emb;
            }
        }

        // Save to database - verify no empty embeddings slipped through
        for (chunk_id, emb) in ids.iter().zip(embeddings.iter()) {
            if emb.is_empty() {
                bail!("Empty embedding for chunk {} - API may have failed", chunk_id);
            }
            db.save_embedding(*chunk_id, &model, emb)?;
        }

        processed += batch.len();
        if processed % 256 == 0 || processed == chunks.len() {
            println!("  Progress: {}/{} ({} cached)", processed, chunks.len(), cached);
        }
    }

    println!(
        "\nEmbedding complete: {} processed, {} from cache",
        processed, cached
    );

    // Invalidate search cache since embeddings changed
    crate::cache::global_cache().invalidate_all();

    Ok(())
}
