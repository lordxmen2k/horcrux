use crate::db::Db;
use anyhow::Result;
use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct CleanupArgs {
    /// Remove all embeddings (force re-embed)
    #[arg(long)]
    pub reset_embeddings: bool,

    /// Remove orphaned chunks (chunks without documents)
    #[arg(long)]
    pub orphan_chunks: bool,

    /// Vacuum the database to reclaim space
    #[arg(long)]
    pub vacuum: bool,
}

pub fn run(args: &CleanupArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    if args.reset_embeddings {
        println!("Clearing all embeddings...");
        db.conn.execute(
            "UPDATE chunks SET embedding = NULL, embed_model = NULL",
            [],
        )?;
        println!("Embeddings cleared. Run 'hoard embed' to regenerate.");
    }

    if args.orphan_chunks {
        println!("Removing orphaned chunks...");
        let deleted = db.conn.execute(
            "DELETE FROM chunks WHERE docid NOT IN (SELECT docid FROM documents)",
            [],
        )?;
        println!("Removed {} orphaned chunks", deleted);
    }

    if args.vacuum {
        println!("Vacuuming database...");
        db.conn.execute("VACUUM", [])?;
        let new_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
        println!("Database size after vacuum: {:.1} MB", new_size as f64 / 1_048_576.0);
    }

    // If no specific flags, show cleanup suggestions
    if !args.reset_embeddings && !args.orphan_chunks && !args.vacuum {
        let chunk_count: i64 = db.conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        let embedded_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM chunks WHERE embedding IS NOT NULL", [], |r| {
                r.get(0)
            })?;
        let orphan_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE docid NOT IN (SELECT docid FROM documents)",
            [],
            |r| r.get(0),
        )?;
        let cache_entries: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM embed_cache", [], |r| r.get(0))?;

        println!("Cleanup status:");
        println!("  Chunks:        {} total", chunk_count);
        println!("  Embedded:      {}/{} ({:.0}%)",
            embedded_count,
            chunk_count,
            if chunk_count > 0 {
                100.0 * embedded_count as f64 / chunk_count as f64
            } else {
                0.0
            }
        );
        println!("  Orphans:       {} chunks without documents", orphan_count);
        println!("  Embed cache:   {} entries", cache_entries);

        println!("\nCleanup options:");
        println!("  --reset-embeddings    Clear all embeddings (force re-embed)");
        println!("  --orphan-chunks       Remove chunks without documents");
        println!("  --vacuum              Compact database file");
    }

    // Invalidate cache after any cleanup
    crate::cache::global_cache().invalidate_all();

    Ok(())
}
