//! Agent CLI - Interactive and one-shot agent commands

use horcrux::agent::{show_current_config, Agent, AgentConfig, ConfigWizard};
use horcrux::db::Db;
use horcrux::tools::TelegramAgentBot;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct AgentArgs {
    /// Optional initial message (if not provided, enters interactive mode)
    pub message: Option<String>,

    /// Resume a previous session
    #[arg(short, long)]
    pub session: Option<String>,

    /// List previous sessions
    #[arg(long)]
    pub list_sessions: bool,

    /// Clear conversation history
    #[arg(long)]
    pub clear: bool,

    /// Run configuration wizard
    #[arg(long)]
    pub setup: bool,

    /// Show current configuration
    #[arg(long)]
    pub show_config: bool,

    /// Path to skills directory
    #[arg(long)]
    pub skills_dir: Option<PathBuf>,
    
    /// Run as Telegram bot (continuous mode)
    #[arg(long)]
    pub telegram: bool,
}

#[derive(Subcommand, Debug)]
pub enum AgentCommands {
    /// Run configuration wizard
    Setup,
    /// Show current configuration
    Config,
    /// List previous sessions
    Sessions,
    /// Clear session history
    Clear,
}

pub async fn run(args: AgentArgs, db_path: &PathBuf) -> Result<()> {
    // Handle special flags first
    if args.setup {
        let wizard = ConfigWizard::new();
        wizard.run().await?;
        return Ok(());
    }

    if args.show_config {
        show_current_config();
        return Ok(());
    }

    if args.list_sessions {
        list_sessions(db_path).await?;
        return Ok(());
    }

    if args.clear {
        clear_sessions(db_path).await?;
        return Ok(());
    }

    // Run as Telegram bot
    if args.telegram {
        return run_telegram_bot(db_path).await;
    }

    // Check if LLM is configured
    if !is_llm_configured() {
        eprintln!("❌ LLM not configured!");
        eprintln!();
        show_current_config();
        eprintln!();
        eprintln!("Run `horcrux agent --setup` to configure your LLM provider.");
        std::process::exit(1);
    }

    // Create agent configuration
    let mut config = AgentConfig::new(db_path.clone());
    
    if let Some(session_id) = args.session {
        config = config.with_session_id(session_id);
    }

    // Create agent
    let mut agent = match Agent::new(config) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("❌ Failed to create agent: {}", e);
            std::process::exit(1);
        }
    };

    // One-shot mode or interactive mode
    if let Some(message) = args.message {
        // One-shot mode
        println!("🤔 Thinking...\n");
        match agent.run(&message).await {
            Ok(response) => {
                println!("{}", response);
            }
            Err(e) => {
                eprintln!("❌ Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Interactive mode
        run_interactive(&mut agent).await?;
    }

    Ok(())
}

async fn run_interactive(agent: &mut Agent) -> Result<()> {
    println!("\n🤖 Horcrux Agent - Interactive Mode\n");
    println!("Session ID: {}", agent.session_id());
    println!("Type 'help' for commands, 'exit' or 'quit' to exit.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("\n💬 You: ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Handle special commands
        match input.to_lowercase().as_str() {
            "exit" | "quit" | ":q" => {
                println!("\n👋 Goodbye!");
                break;
            }
            "help" | ":h" => {
                print_help();
                continue;
            }
            "clear" | ":c" => {
                agent.clear_history().await?;
                println!("🗑️  Conversation history cleared.");
                continue;
            }
            "config" | ":cfg" => {
                show_current_config();
                continue;
            }
            _ => {}
        }

        // Process user input
        print!("\n🤖 Agent: ");
        stdout.flush()?;

        match agent.run_interactive(input).await {
            Ok(response) => {
                println!("{}\n", response);
            }
            Err(e) => {
                println!("❌ Error: {}\n", e);
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!("\n📖 Available Commands:\n");
    println!("  help, :h     Show this help message");
    println!("  clear, :c    Clear conversation history");
    println!("  config, :cfg Show current configuration");
    println!("  exit, :q     Exit the agent\n");
    println!("You can ask me to:\n");
    println!("  • Search your knowledge base");
    println!("  • Read, write, or edit files");
    println!("  • Execute shell commands");
    println!("  • Make HTTP requests");
    println!("  • Create custom skills/tools\n");
}

async fn list_sessions(db_path: &PathBuf) -> Result<()> {
    let db = Db::open(db_path)?;
    let sessions = db.list_sessions(20)?;

    if sessions.is_empty() {
        println!("No previous sessions found.");
        return Ok(());
    }

    println!("\n📚 Recent Sessions:\n");
    println!("{:<30} {:<20} {:<10}", "Session ID", "Last Active", "Messages");
    println!("{}", "-".repeat(70));

    for session in sessions {
        let short_id = if session.session_id.len() > 28 {
            format!("{}...", &session.session_id[..25])
        } else {
            session.session_id.clone()
        };
        
        // Format timestamp nicely
        let last_active = session.last_active.split('T').next()
            .unwrap_or(&session.last_active)
            .to_string();

        println!(
            "{:<30} {:<20} {:<10}",
            short_id,
            last_active,
            session.message_count
        );
    }

    println!("\n💡 Resume a session with: horcrux agent -s <session_id>");

    Ok(())
}

async fn clear_sessions(db_path: &PathBuf) -> Result<()> {
    print!("⚠️  This will delete ALL conversation history. Are you sure? (yes/no): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() == "yes" {
        let db = Db::open(db_path)?;
        let deleted = db.delete_old_conversations(0)?;
        println!("🗑️  Deleted {} conversation messages.", deleted);
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

async fn run_telegram_bot(db_path: &PathBuf) -> Result<()> {
    // Check for Telegram bot token
    if std::env::var("TELEGRAM_BOT_TOKEN").is_err() {
        eprintln!("❌ TELEGRAM_BOT_TOKEN not set!");
        eprintln!();
        eprintln!("To create a Telegram bot:");
        eprintln!("  1. Message @BotFather on Telegram");
        eprintln!("  2. Create a new bot with /newbot");
        eprintln!("  3. Copy the token and set it:");
        eprintln!();
        eprintln!("     export TELEGRAM_BOT_TOKEN='your-token-here'");
        eprintln!();
        std::process::exit(1);
    }

    println!("🚀 Starting Telegram Agent Bot...");
    println!("The bot will process all incoming messages through the AI agent.\n");

    let bot = TelegramAgentBot::new(db_path.clone());
    bot.run().await
}

fn is_llm_configured() -> bool {
    std::env::var("HORCRUX_LLM_URL").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("OLLAMA_HOST").is_ok()
}
