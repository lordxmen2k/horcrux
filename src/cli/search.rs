use crate::cache::global_cache;
use crate::db::Db;
use crate::embed::EmbedConfig;
use crate::search::run_search;
use crate::types::SearchOutput;
use anyhow::Result;
use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Output results as JSON (used by OpenClaw)
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

    /// Disable in-memory result caching for this query
    #[arg(long)]
    pub no_cache: bool,
}

pub fn run(args: SearchArgs, db_path: &Path, mode: &str) -> Result<()> {
    let db = Db::open(db_path)?;
    let limit = if args.all { 1000 } else { args.n };

    // Check in-memory cache first (unless disabled)
    let cache = if args.no_cache { None } else { Some(global_cache()) };
    let cache_key = crate::cache::SearchCache::make_key(&args.query, mode, args.collection.as_deref(), limit);

    if let Some(c) = cache {
        if let Some(cached) = c.get(&cache_key) {
            let output = SearchOutput {
                results: cached,
                query: args.query.clone(),
                backend: "horcrux".into(),
                total: 0,
            };
            print_output(&output, args.json)?;
            return Ok(());
        }
    }

    // Setup embedding client if needed for vector/hybrid modes
    let embed_config = EmbedConfig::from_env();
    let embed_client = if mode == "search" {
        None
    } else {
        // Try to create client, fall back to None if no URL configured
        if std::env::var("HORCRUX_EMBED_URL").is_ok()
            || std::env::var("HOARD_EMBED_URL").is_ok() // backward compat
            || std::env::var("CLAW_EMBED_URL").is_ok() // older compat
            || std::env::var("OLLAMA_HOST").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("HOARD_EMBED_API_KEY").is_ok()
        {
            Some(crate::embed::EmbedClient::new(embed_config.clone()))
        } else {
            None
        }
    };

    let results = run_search(
        &db,
        &args.query,
        mode,
        limit,
        args.min_score,
        args.collection.as_deref(),
        embed_client.as_ref(),
        &embed_config.model,
    )?;

    // Cache the results
    if let Some(c) = cache {
        c.set(cache_key, results.clone());
    }

    let output = SearchOutput {
        total: results.len(),
        query: args.query,
        backend: "horcrux".into(),
        results,
    };

    print_output(&output, args.json)?;
    Ok(())
}

fn print_output(output: &SearchOutput, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(output)?);
    } else {
        if output.results.is_empty() {
            println!("No results found.");
            return Ok(());
        }

        println!("Query: {}\n", output.query);
        for (i, r) in output.results.iter().enumerate() {
            println!(
                "{}. {} ({}) — score: {:.3}",
                i + 1,
                r.title,
                r.path,
                r.score
            );
            if let Some(ref ctx) = r.context {
                println!("   context: {}", ctx);
            }
            println!("   {}\n", r.snippet.replace('\n', " "));
        }
        println!("{} result(s) from {}", output.results.len(), output.backend);
    }
    Ok(())
}
