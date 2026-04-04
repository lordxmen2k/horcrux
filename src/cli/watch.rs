use crate::db::Db;
use crate::types::Collection;
use anyhow::Result;
use clap::Args;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Collection to watch (default: all)
    #[arg(short, long)]
    pub collection: Option<String>,

    /// Debounce duration in seconds
    #[arg(short, long, default_value = "2")]
    pub debounce: u64,
}

pub fn run(args: WatchArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    let collections: Vec<Collection> = if let Some(name) = args.collection {
        vec![db
            .get_collection(&name)?
            .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", name))?]
    } else {
        db.list_collections()?
    };

    if collections.is_empty() {
        println!("No collections to watch. Add one first:");
        println!("  hoard collection add <path>");
        return Ok(());
    }

    println!("👁️  Watching {} collection(s) for changes...", collections.len());
    for col in &collections {
        println!("  📁 {} -> {}", col.name, col.path);
    }
    println!("\nPress Ctrl+C to stop\n");

    let (tx, rx) = channel::<notify::Result<Event>>();

    let mut watcher: RecommendedWatcher = Watcher::new(
        tx,
        Config::default().with_poll_interval(Duration::from_secs(args.debounce)),
    )?;

    // Watch all collection paths
    for col in &collections {
        let path = std::path::Path::new(&col.path);
        if path.exists() {
            watcher.watch(path, RecursiveMode::Recursive)?;
            info!("Watching: {}", col.path);
        } else {
            warn!("Path does not exist: {}", col.path);
        }
    }

    let mut last_update = std::time::Instant::now();
    let debounce = Duration::from_secs(args.debounce);

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                // Only react to file modifications, not metadata changes
                if matches!(
                    event.kind,
                    notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
                ) {
                    // Check if it's a markdown file
                    let is_markdown = event
                        .paths
                        .iter()
                        .any(|p| p.extension().map(|e| e == "md").unwrap_or(false));

                    if is_markdown && last_update.elapsed() > debounce {
                        println!("📝 Change detected: {:?}", event.paths);
                        println!("🔄 Re-indexing...");

                        // Trigger update
                        if let Err(e) = trigger_update(&collections, db_path) {
                            warn!("Update failed: {}", e);
                        } else {
                            println!("✅ Re-index complete\n");
                        }

                        last_update = std::time::Instant::now();
                    }
                }
            }
            Ok(Err(e)) => warn!("Watch error: {:?}", e),
            Err(e) => {
                warn!("Channel error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}

fn trigger_update(collections: &[Collection], _db_path: &Path) -> Result<()> {
    for col in collections {
        println!("  [stub] Would update '{}'", col.name);
    }
    Ok(()) // clearly a stub, no false DB open
}
