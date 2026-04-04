use crate::db::Db;
use crate::types::PathContext;
use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use std::path::Path;

#[derive(Args, Debug)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub cmd: ContextCommands,
}

#[derive(Subcommand, Debug)]
pub enum ContextCommands {
    /// Add context metadata for a path
    Add {
        /// Path (can be collection-relative or #docid)
        path: String,
        /// Context string to associate
        context: String,
        /// Collection name (if path is relative)
        #[arg(short, long)]
        collection: Option<String>,
    },
    /// List all path contexts
    List,
    /// Remove context for a path
    Remove {
        /// Path to remove context from
        path: String,
        /// Collection name
        #[arg(short, long)]
        collection: Option<String>,
    },
    /// Show context for a specific path
    Show {
        /// Path to look up
        path: String,
        /// Collection name
        #[arg(short, long)]
        collection: Option<String>,
    },
}

pub fn run(args: ContextArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    match args.cmd {
        ContextCommands::Add {
            path,
            context,
            collection,
        } => {
            let collection = collection.unwrap_or_default();

            // Validate path exists if it looks like a docid
            if path.starts_with('#') {
                let docid = path.trim_start_matches('#');
                if db.get_document(docid)?.is_none() {
                    bail!("Document '{}' not found", path);
                }
            }

            let ctx = PathContext {
                collection: collection.clone(),
                path: path.clone(),
                context: context.clone(),
            };

            db.add_context(&ctx)?;
            println!("Added context for '{}': {}", path, context);
        }

        ContextCommands::List => {
            let contexts = db.list_contexts()?;
            if contexts.is_empty() {
                println!("No path contexts defined.");
                return Ok(());
            }

            println!("Path contexts:");
            for ctx in contexts {
                let col_display = if ctx.collection.is_empty() {
                    "(global)".to_string()
                } else {
                    ctx.collection.clone()
                };
                println!("  [{:15}] {:30} → {}", col_display, ctx.path, ctx.context);
            }
        }

        ContextCommands::Remove { path, collection } => {
            let collection = collection.unwrap_or_default();
            db.remove_context(&collection, &path)?;
            println!("Removed context for '{}'", path);
        }

        ContextCommands::Show { path, collection } => {
            let collection = collection.unwrap_or_default();

            let resolved_path = if path.starts_with('#') {
                let docid = path.trim_start_matches('#');
                match db.get_document(docid)? {
                    Some(doc) => doc.path,
                    None => {
                        bail!("Document '{}' not found", path);
                    }
                }
            } else {
                path.clone()
            };

            match db.get_context_for_document(&resolved_path, &collection)? {
                Some(ctx) => println!("Context for '{}': {}", path, ctx),
                None => println!("No context found for '{}'", path),
            }
        }
    }

    Ok(())
}
