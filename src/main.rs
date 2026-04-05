mod cache;
mod chunk;
mod cli;
mod db;
mod embed;
mod mcp;
mod search;
mod server;
mod tui;
mod types;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file automatically if present
    let _ = dotenvy::dotenv();
    
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let db_path = cli.db_path();

    match cli.command {
        // Default: launch TUI when no subcommand given
        None | Some(Commands::Tui) => {
            tui::run(db_path, cli.collection)?;
        }
        
        // CLI commands - for scripts and OpenClaw integration
        Some(Commands::Collection(cmd)) => cli::collection::run(cmd, &db_path)?,
        Some(Commands::Update(cmd)) => cli::update::run(cmd, &db_path)?,
        Some(Commands::Embed(cmd)) => cli::embed::run(cmd, &db_path)?,
        Some(Commands::Search(cmd)) => cli::search::run(cmd, &db_path, "search")?,
        Some(Commands::Vsearch(cmd)) => cli::search::run(cmd, &db_path, "vsearch")?,
        Some(Commands::Query(cmd)) => cli::search::run(cmd, &db_path, "query")?,
        Some(Commands::Get(cmd)) => cli::get::run(cmd, &db_path)?,
        Some(Commands::Status(_)) => cli::status::run(&db_path)?,
        Some(Commands::Cleanup(ref args)) => cli::cleanup::run(args, &db_path)?,
        Some(Commands::Context(cmd)) => cli::context::run(cmd, &db_path)?,
        
        // Server modes
        Some(Commands::Serve(args)) => {
            server::run_server(db_path, args.host, args.port).await?;
        }
        Some(Commands::Watch(args)) => cli::watch::run(args, &db_path)?,
        
        // MCP server for Claude Desktop
        Some(Commands::Mcp) => {
            let mut mcp = mcp::McpServer::new(db_path);
            mcp.run().await?;
        }
        
        // AI Agent
        Some(Commands::Agent(args)) => {
            cli::agent::run(args, &db_path).await?;
        }
        
        // Setup wizard
        Some(Commands::Setup { section }) => {
            let wizard = cli::setup::SetupWizard::new();
            wizard.run(section.as_deref()).await?;
        }
    }

    Ok(())
}
