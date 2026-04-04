use crate::db::Db;
use crate::types::Collection;
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct CollectionArgs {
    #[command(subcommand)]
    pub cmd: CollectionCommands,
}

#[derive(Subcommand, Debug)]
pub enum CollectionCommands {
    /// Add a directory as a collection
    Add {
        /// Path to the directory
        path: PathBuf,
        /// Collection name (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,
        /// File pattern glob (default: "**/*.md")
        #[arg(short, long, default_value = "**/*.md")]
        pattern: String,
    },
    /// List all collections
    List,
    /// Remove a collection (does not delete files)
    Remove {
        /// Collection name
        name: String,
    },
    /// Rename a collection
    Rename {
        /// Current name
        old: String,
        /// New name
        new: String,
    },
}

pub fn run(args: CollectionArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    match args.cmd {
        CollectionCommands::Add { path, name, pattern } => {
            let abs_path = std::fs::canonicalize(&path)
                .with_context(|| format!("Cannot access path: {}", path.display()))?;

            if !abs_path.is_dir() {
                bail!("Path is not a directory: {}", abs_path.display());
            }

            let col_name = name.unwrap_or_else(|| {
                abs_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
                    .to_string()
            });

            let collection = Collection {
                name: col_name.clone(),
                path: abs_path.to_string_lossy().to_string(),
                pattern,
            };

            db.add_collection(&collection)?;
            println!("Added collection '{}' -> {}", col_name, abs_path.display());
        }

        CollectionCommands::List => {
            let cols = db.list_collections()?;
            if cols.is_empty() {
                println!("No collections. Add one with: hoard collection add <path>");
                return Ok(());
            }
            println!("Collections:");
            for c in cols {
                println!("  {:20} {} (pattern: {})", c.name, c.path, c.pattern);
            }
        }

        CollectionCommands::Remove { name } => {
            // Check if exists
            if db.get_collection(&name)?.is_none() {
                bail!("Collection '{}' not found", name);
            }
            db.remove_collection(&name)?;
            println!("Removed collection '{}'", name);
        }

        CollectionCommands::Rename { old, new } => {
            let col = db
                .get_collection(&old)?
                .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", old))?;

            let new_col = Collection {
                name: new.clone(),
                path: col.path,
                pattern: col.pattern,
            };

            // Wrap in transaction to prevent data loss if second write fails
            db.conn.execute_batch("BEGIN")?;
            let result = (|| -> Result<()> {
                db.remove_collection(&old)?;
                db.add_collection(&new_col)?;
                Ok(())
            })();
            match result {
                Ok(_) => { db.conn.execute_batch("COMMIT")?; }
                Err(e) => { db.conn.execute_batch("ROLLBACK")?; return Err(e); }
            }
            println!("Renamed '{}' -> '{}'", old, new);
        }
    }

    Ok(())
}
