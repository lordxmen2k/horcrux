use crate::chunk::{chunk_markdown, extract_title};
use crate::db::Db;
use crate::types::{Chunk, Document};
use anyhow::{Context, Result};
use clap::Args;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Collection to update (default: all collections)
    #[arg(short, long)]
    pub collection: Option<String>,

    /// Dry run: show what would change without modifying
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: UpdateArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    let collections = if let Some(name) = args.collection {
        vec![db
            .get_collection(&name)?
            .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", name))?]
    } else {
        db.list_collections()?
    };

    if collections.is_empty() {
        println!("No collections to update. Add one first:");
        println!("  horcrux collection add <path>");
        return Ok(());
    }

    let mut total_added = 0;
    let mut total_updated = 0;
    let mut total_removed = 0;

    // Max file size: 10 MB (skip larger files)
    const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

    for col in collections {
        println!("Updating collection '{}'...", col.name);

        let base_path = Path::new(&col.path);
        let pattern = &col.pattern;

        // Compile pattern regex once before the file walk
        let compiled_pattern = compile_pattern(&col.pattern);

        // Find all matching files
        let mut found_docids = HashSet::new();
        let mut files_to_process = Vec::new();

        for entry in WalkDir::new(base_path).follow_links(true) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            
            // Check file size first
            let metadata = entry.metadata()?;
            let file_size = metadata.len();
            
            if file_size > MAX_FILE_SIZE {
                eprintln!("  Warning: Skipping large file ({} MB): {}", 
                    file_size / 1024 / 1024, 
                    path.display());
                continue;
            }
            
            // Skip binary files (check first 1KB for null bytes)
            if is_binary_file(path)? {
                eprintln!("  Warning: Skipping binary file: {}", path.display());
                continue;
            }
            
            let rel_path = path.strip_prefix(base_path).unwrap_or(path);
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");

            // Simple glob check (supports **/*.md patterns)
            if !matches_pattern(&rel_str, pattern, compiled_pattern.as_ref()) {
                continue;
            }

            let docid = generate_docid(&col.name, &rel_str);
            found_docids.insert(docid.clone());
            files_to_process.push((path.to_path_buf(), rel_str, docid, file_size));
        }
        
        println!("  Found {} files to process", files_to_process.len());

        // Process each file
        for (abs_path, rel_path, docid, _size) in files_to_process {
            let content = std::fs::read_to_string(&abs_path)
                .with_context(|| format!("Failed to read: {}", abs_path.display()))?;

            let hash = sha256_hex(&content);
            let title = extract_title(&content, &rel_path);

            // Generate chunks first, then move content into doc
            let chunks_raw = chunk_markdown(&content);

            let doc = Document {
                docid: docid.clone(),
                path: rel_path.clone(),
                collection: col.name.clone(),
                title,
                body: content, // move, don't clone
                hash,
                updated_at: chrono::Utc::now(),
            };

            let (changed, is_new) = if args.dry_run {
                let existing = db.get_document(&docid)?;
                let new = existing.is_none();
                let changed = existing.map(|d| d.hash != doc.hash).unwrap_or(true);
                (changed, new)
            } else {
                (db.upsert_document(&doc)?, false)
            };

            if changed {
                if args.dry_run {
                    if is_new {
                        println!("  [DRY-RUN] Would add: {}", rel_path);
                        total_added += 1;
                    } else {
                        println!("  [DRY-RUN] Would update: {}", rel_path);
                        total_updated += 1;
                    }
                } else {
                    // Drop doc to free body memory before chunk allocation
                    drop(doc);

                    let db_chunks: Vec<Chunk> = chunks_raw
                        .into_iter()
                        .map(|c| Chunk {
                            docid: docid.clone(),
                            seq: c.seq,
                            text: c.text,
                            pos: c.pos,
                            embedding: None,
                        })
                        .collect();
                    db.insert_chunks(&db_chunks)?;

                    let existing = db.get_document(&docid)?;
                    if existing.is_some() {
                        total_updated += 1;
                    } else {
                        total_added += 1;
                    }
                }
            }
        }

        // Remove documents that no longer exist on disk
        if !args.dry_run {
            let found_vec: Vec<String> = found_docids.into_iter().collect();
            let removed = db.remove_missing_documents(&col.name, &found_vec)?;
            total_removed += removed;
        }
    }

    println!();
    if args.dry_run {
        println!("Dry-run complete:");
        println!("  Would add:    {}", total_added);
        println!("  Would update: {}", total_updated);
    } else {
        println!("Update complete:");
        println!("  Added:   {}", total_added);
        println!("  Updated: {}", total_updated);
        println!("  Removed: {}", total_removed);

        if total_added > 0 || total_updated > 0 {
            println!("\nRun 'hoard embed' to generate embeddings for new chunks.");
        }

        // Invalidate search cache since index changed
        crate::cache::global_cache().invalidate_all();
    }

    Ok(())
}

fn generate_docid(collection: &str, rel_path: &str) -> String {
    let input = format!("{}:{}", collection, rel_path);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    // Take first 3 bytes (6 hex chars), matching the existing format
    hex::encode(&hasher.finalize()[..3])
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Check if a file appears to be binary (contains null bytes in first 1KB)
fn is_binary_file(path: &Path) -> Result<bool> {
    use std::io::Read;
    
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0u8; 1024];
    let bytes_read = file.read(&mut buffer)?;
    
    // Check for null bytes (common in binary files)
    Ok(buffer[..bytes_read].contains(&0))
}

fn compile_pattern(pattern: &str) -> Option<regex::Regex> {
    if pattern.contains('*') {
        let regex = pattern
            .replace('.', r"\.")
            .replace("**", "___DS___")
            .replace('*', "[^/]*")
            .replace("___DS___", ".*");
        regex::Regex::new(&format!("^{}$", regex)).ok()
    } else {
        None
    }
}

fn matches_pattern(path: &str, pattern: &str, compiled: Option<&regex::Regex>) -> bool {
    if pattern == "**/*.md" || pattern == "*.md" {
        return path.ends_with(".md");
    }
    compiled.map(|re| re.is_match(path)).unwrap_or(path == pattern)
}
