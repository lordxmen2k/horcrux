pub mod agent;
pub mod collection;
pub mod context;
pub mod embed;
pub mod get;
pub mod search;
pub mod serve;
pub mod setup;
pub mod status;
pub mod update;
pub mod watch;
pub mod cleanup;

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// horcrux: AI Agent with Knowledge Memory.
/// 
/// Distributed intelligence for your tasks — part AI agent, part memory system.
/// Index your documents and let the agent help you work with them.
/// 
/// USAGE:
///   horcrux                  Launch interactive TUI (default)
///   horcrux search "query"   Search from command line
///   horcrux collection add   Add a folder to your knowledge base
///   horcrux status           Check index status
///   horcrux agent            Start the AI agent
///
/// ENVIRONMENT:
///   HORCRUX_EMBED_URL      Embedding API URL (default: Ollama/OpenAI)
///   HORCRUX_EMBED_MODEL    Model name (default: text-embedding-3-small)
///   HORCRUX_EMBED_API_KEY  API key for OpenAI etc.
///   HORCRUX_LLM_URL        LLM API URL for agent
///   HORCRUX_LLM_MODEL      LLM model for agent (e.g., gpt-4o-mini, qwen2.5:7b)
///   HORCRUX_LLM_API_KEY    LLM API key for agent
///   TELEGRAM_BOT_TOKEN     Telegram bot token for telegram integration
#[derive(Parser, Debug)]
#[command(name = "horcrux", version, about, long_about = None)]
pub struct Cli {
    /// Path to the SQLite index (overrides default location)
    #[arg(long, global = true)]
    pub index: Option<PathBuf>,

    /// Default collection to search
    #[arg(short, long, global = true)]
    pub collection: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch interactive TUI (default when no subcommand)
    #[command(alias = "i")]
    Tui,
    
    /// Manage collections (add / list / remove / rename)
    Collection(collection::CollectionArgs),
    
    /// Re-index collections by scanning the filesystem
    Update(update::UpdateArgs),
    
    /// Generate vector embeddings for indexed documents
    Embed(embed::EmbedArgs),
    
    /// BM25 full-text search
    Search(search::SearchArgs),
    
    /// Vector semantic search
    Vsearch(search::SearchArgs),
    
    /// Hybrid search: BM25 + vector + re-ranking (recommended)
    Query(search::SearchArgs),
    
    /// Retrieve a document by path or docid
    Get(get::GetArgs),
    
    /// Show index status and collection stats
    Status(status::StatusArgs),
    
    /// Remove orphaned cache entries
    Cleanup(cleanup::CleanupArgs),
    
    /// Manage path contexts (metadata that improves search relevance)
    Context(context::ContextArgs),
    
    /// Start HTTP API server
    Serve(serve::ServeArgs),
    
    /// Run MCP server (Model Context Protocol)
    Mcp,
    
    /// Watch collections for changes and auto-reindex
    Watch(watch::WatchArgs),
    
    /// AI Agent - Interactive assistant with tool use
    #[command(alias = "a")]
    Agent(agent::AgentArgs),
    
    /// Interactive setup wizard for configuration
    #[command(alias = "configure")]
    Setup,
}

impl Cli {
    pub fn db_path(&self) -> PathBuf {
        if let Some(ref p) = self.index {
            return p.clone();
        }
        // Default: $HOME/.local/share/horcrux/horcrux.db (Linux)
        //          $HOME/Library/Application Support/horcrux/horcrux.db (macOS)
        //          %LOCALAPPDATA%/horcrux/horcrux.db (Windows)
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("horcrux")
            .join("horcrux.db")
    }
}

// ── Shared output format args ───────────────────────────────────────────────

#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// Output results as JSON
    #[arg(long)]
    pub json: bool,

    /// Number of results to return
    #[arg(short = 'n', default_value = "5")]
    pub n: usize,

    /// Filter to a specific collection
    #[arg(short = 'c', long)]
    pub collection: Option<String>,

    /// Minimum relevance score (0.0–1.0)
    #[arg(long, default_value = "0.0")]
    pub min_score: f32,

    /// Return all matches above min_score (no result cap)
    #[arg(long)]
    pub all: bool,
}
