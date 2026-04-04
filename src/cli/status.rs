use crate::db::Db;
use anyhow::Result;
use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct StatusArgs {}

pub fn run(db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;
    let collections = db.list_collections()?;
    let doc_count = db.document_count()?;
    let chunk_count = db.chunk_count()?;
    let embedded = db.embedded_chunk_count()?;
    let db_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    println!("hoard status");
    println!("  db:         {}", db_path.display());
    println!("  db size:    {:.1} MB", db_size as f64 / 1_048_576.0);
    println!("  documents:  {}", doc_count);
    println!("  chunks:     {} ({} embedded)", chunk_count, embedded);

    println!("\nCollections:");
    for c in &collections {
        println!("  {:20} {}", c.name, c.path);
    }
    if collections.is_empty() {
        println!("  (none — add with: hoard collection add <path>)");
    }

    let embed_url = std::env::var("HORCRUX_EMBED_URL")
        .or_else(|_| std::env::var("HOARD_EMBED_URL"))
        .or_else(|_| std::env::var("OLLAMA_HOST").map(|h| format!("{}/v1", h)))
        .unwrap_or_else(|_| "(not set — BM25-only mode)".into());

    let embed_model = std::env::var("HORCRUX_EMBED_MODEL")
        .or_else(|_| std::env::var("HOARD_EMBED_MODEL"))
        .unwrap_or_else(|_| "text-embedding-3-small".into());
    println!("\nEmbedding:");
    println!("  url:    {}", embed_url);
    println!("  model:  {}", embed_model);

    Ok(())
}