//! Interactive Setup Wizard for Horcrux Configuration

use anyhow::Result;
use std::io::{self, Write};

/// Provider configuration with metadata
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub needs_api_key: bool,
    pub description: &'static str,
    pub free_tier: bool,
    pub requires_vpn_in_china: bool,
}

/// LOCAL MODELS (via Ollama)
pub const LOCAL_PROVIDERS: &[ProviderConfig] = &[
    ProviderConfig {
        name: "Llama 3.1 8B (Recommended)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.1:8b",
        needs_api_key: false,
        description: "Best balance of capability and speed. Excellent tool use, runs on 8GB+ RAM",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Llama 3.2 3B (Lightweight)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.2:3b",
        needs_api_key: false,
        description: "Ultra-fast, runs on 4GB RAM. Good for simple tasks, less capable for complex reasoning",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Qwen 2.5 14B (Powerful)",
        base_url: "http://localhost:11434/v1",
        default_model: "qwen2.5:14b",
        needs_api_key: false,
        description: "Strong reasoning, requires 16GB+ RAM. Best for complex multi-step tasks",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Qwen 2.5 7B (Balanced)",
        base_url: "http://localhost:11434/v1",
        default_model: "qwen2.5:7b",
        needs_api_key: false,
        description: "Good balance, runs on 8GB RAM. Decent tool use and reasoning",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Mistral 7B (Fast)",
        base_url: "http://localhost:11434/v1",
        default_model: "mistral:7b",
        needs_api_key: false,
        description: "Fast responses, good for simple queries. Decent tool support",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "DeepSeek-R1 8B (Reasoning)",
        base_url: "http://localhost:11434/v1",
        default_model: "deepseek-r1:8b",
        needs_api_key: false,
        description: "EXCELLENT reasoning but NO tool support. Good for thinking tasks only",
        free_tier: true,
        requires_vpn_in_china: false,
    },
];

/// CLOUD PROVIDERS - All major providers supported
pub const CLOUD_PROVIDERS: &[ProviderConfig] = &[
    // TIER 1: Recommended / Best Experience
    ProviderConfig {
        name: "Kimi (Moonshot) - RECOMMENDED",
        base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-8k",
        needs_api_key: true,
        description: "Best tool use for the price. $0.50/1M tokens. Chinese & English support",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Anthropic (Claude) - Premium",
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-3-5-sonnet-20241022",
        needs_api_key: true,
        description: "Best-in-class reasoning and tool use. $3/1M tokens. Industry standard",
        free_tier: false,
        requires_vpn_in_china: true,
    },
    ProviderConfig {
        name: "OpenAI (GPT-4o) - Popular",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
        needs_api_key: true,
        description: "Reliable, fast, excellent tool support. $0.15/1M tokens",
        free_tier: false,
        requires_vpn_in_china: true,
    },
    
    // TIER 2: Aggregators / Multi-Model
    ProviderConfig {
        name: "OpenRouter - Universal",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-3.5-sonnet",
        needs_api_key: true,
        description: "One API key for 100+ models. Claude, GPT, Llama, Mistral, etc",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    ProviderConfig {
        name: "Groq - Ultra Fast",
        base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.1-70b-versatile",
        needs_api_key: true,
        description: "Fastest inference. Llama 3.1 70B/8B, Mixtral. Generous free tier",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    
    // TIER 3: Specialized / Regional
    ProviderConfig {
        name: "DeepSeek - China Optimized",
        base_url: "https://api.deepseek.com/v1",
        default_model: "deepseek-chat",
        needs_api_key: true,
        description: "Excellent reasoning, cheap pricing. Good for complex tasks",
        free_tier: true,
        requires_vpn_in_china: false,
    },
    ProviderConfig {
        name: "Together AI - Open Source",
        base_url: "https://api.together.xyz/v1",
        default_model: "meta-llama/Llama-3.1-70B-Instruct-Turbo",
        needs_api_key: true,
        description: "Focus on open source models. Llama, Mixtral, Qwen, etc",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    ProviderConfig {
        name: "Fireworks AI - Fast",
        base_url: "https://api.fireworks.ai/inference/v1",
        default_model: "accounts/fireworks/models/llama-v3p1-70b-instruct",
        needs_api_key: true,
        description: "Fast inference for open source models. Competitive pricing",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    ProviderConfig {
        name: "Cohere - Command R+",
        base_url: "https://api.cohere.ai/v1",
        default_model: "command-r-plus",
        needs_api_key: true,
        description: "Strong RAG and tool use. Good for knowledge tasks",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    ProviderConfig {
        name: "AI21 Labs - Jurassic",
        base_url: "https://api.ai21.com/studio/v1",
        default_model: "jamba-1.5-large",
        needs_api_key: true,
        description: "Long context (256K), good for document analysis",
        free_tier: true,
        requires_vpn_in_china: true,
    },
    
    // TIER 4: Azure / Enterprise
    ProviderConfig {
        name: "Azure OpenAI - Enterprise",
        base_url: "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT",
        default_model: "gpt-4",
        needs_api_key: true,
        description: "Enterprise-grade OpenAI. Requires Azure subscription",
        free_tier: false,
        requires_vpn_in_china: false,
    },
];

pub struct SetupWizard;

impl SetupWizard {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(&self) -> Result<()> {
        println!("\n🧙 Horcrux Setup Wizard\n");
        println!("Let's configure your AI agent with all the bells and whistles!\n");

        // Step 1: AI Model Configuration
        let model_config = self.setup_model().await?;
        
        // Step 2: Choose Personality (optional, defaults to Voldemort)
        let personality_config = self.setup_personality().await?;
        
        // Step 3: Messaging Integrations (optional)
        let integrations_config = self.setup_integrations().await?;
        
        // Step 4: Server & API (optional)
        let server_config = self.setup_server().await?;
        
        // Step 5: Advanced Features (optional)
        let advanced_config = self.setup_advanced().await?;
        
        // Save all configurations
        self.save_complete_config(model_config, personality_config, integrations_config, server_config, advanced_config).await?;
        
        Ok(())
    }
    
    async fn setup_model(&self) -> Result<ModelConfig> {
        println!("\n📦 STEP 1: AI Model Configuration\n");
        
        let choice = self.select_deployment_type()?;
        
        match choice {
            DeploymentType::Local => {
                // Use existing local setup logic
                let (base_url, model, api_key) = self.setup_local_model().await?;
                Ok(ModelConfig { base_url, model, api_key })
            }
            DeploymentType::Cloud => {
                // Use existing cloud setup logic
                let (base_url, model, api_key) = self.setup_cloud_model().await?;
                Ok(ModelConfig { base_url, model, api_key })
            }
        }
    }
    
    async fn setup_local_model(&self) -> Result<(String, String, String)> {
        // This wraps the existing setup_local logic but returns values
        println!("\n📦 Local Model Setup (Ollama)\n");
        
        // Check if Ollama is installed
        println!("Step 1: Checking if Ollama is installed...\n");
        
        let ollama_installed = self.check_ollama_installed();
        
        if !ollama_installed {
            println!("❌ Ollama is not installed on your system.\n");
            println!("📋 To use local models, you must install Ollama yourself:\n");
            println!("   ┌─────────────────────────────────────────────────────────┐");
            println!("   │  macOS or Linux:                                        │");
            println!("   │  curl -fsSL https://ollama.com/install.sh | sh          │");
            println!("   │                                                         │");
            println!("   │  Windows:                                               │");
            println!("   │  Download from https://ollama.com/download/windows      │");
            println!("   │  Or use: winget install Ollama.Ollama                   │");
            println!("   └─────────────────────────────────────────────────────────┘\n");
            println!("📋 After installing Ollama:");
            println!("   1. Ollama runs as a background service");
            println!("   2. Verify it's working: ollama --version");
            println!("   3. Then return here to continue setup\n");
            
            let continue_setup = self.prompt_yes_no("Have you installed Ollama?")?;
            
            if !continue_setup {
                println!("\n⏹️  Setup paused.");
                println!("   Install Ollama using the instructions above,");
                println!("   then run: horcrux setup\n");
                return Err(anyhow::anyhow!("Ollama not installed"));
            }
            
            // Verify installation
            if !self.check_ollama_installed() {
                println!("\n❌ Ollama is still not detected.");
                println!("   Make sure Ollama is running (check system tray on Windows)");
                println!("   and try again.\n");
                return Err(anyhow::anyhow!("Ollama not detected"));
            }
        }
        
        println!("✅ Ollama is installed!\n");
        
        // Select model
        println!("Step 2: Select a model\n");
        println!("These models will be downloaded through Ollama:\n");
        
        for (i, provider) in LOCAL_PROVIDERS.iter().enumerate() {
            let recommended = if i == 0 { " ⭐ RECOMMENDED" } else { "" };
            println!("{}) {}{}", i + 1, provider.name, recommended);
            println!("   {}\n", provider.description);
        }
        
        let model_choice = self.prompt_number(
            "Enter model number",
            1,
            LOCAL_PROVIDERS.len()
        )?;
        
        let selected = &LOCAL_PROVIDERS[model_choice - 1];
        
        println!("\n📋 Next Steps:\n");
        println!("   You selected: {}", selected.default_model);
        println!("\n   To download this model, run this command in your terminal:");
        println!("   ┌─────────────────────────────────────────────────────────┐");
        println!("   │  ollama pull {:43}│", selected.default_model);
        println!("   └─────────────────────────────────────────────────────────┘\n");
        
        let has_model = self.prompt_yes_no("Have you downloaded the model?")?;
        
        if !has_model {
            println!("\n⏹️  Setup paused.");
            println!("   Run: ollama pull {}", selected.default_model);
            println!("   Then run: horcrux setup\n");
            return Err(anyhow::anyhow!("Model not downloaded"));
        }
        
        // Verify model exists
        if !self.check_model_exists(selected.default_model) {
            println!("\n⚠️  Model '{}' not found.", selected.default_model);
            println!("   Make sure you ran: ollama pull {}", selected.default_model);
            println!("   Check installed models: ollama list\n");
            return Err(anyhow::anyhow!("Model not found"));
        }

        println!("\n✅ Local model configured!");
        println!("   Model: {}\n", selected.default_model);
        
        Ok((selected.base_url.to_string(), selected.default_model.to_string(), "ollama".to_string()))
    }
    
    async fn setup_cloud_model(&self) -> Result<(String, String, String)> {
        // This wraps the existing setup_cloud logic but returns values
        println!("\n☁️  Cloud API Setup\n");
        
        println!("Select a cloud provider:\n");
        
        // Show providers by tier
        println!("⭐ RECOMMENDED:");
        let recommended: Vec<_> = CLOUD_PROVIDERS.iter()
            .filter(|p| p.name.contains("RECOMMENDED") || p.name.contains("Premium"))
            .collect();
        for (i, provider) in recommended.iter().enumerate() {
            let free_badge = if provider.free_tier { " [FREE TIER]" } else { "" };
            println!("{}) {}{}", i + 1, provider.name, free_badge);
            println!("   {}\n", provider.description);
        }
        
        println!("☁️  ALL PROVIDERS:");
        for (i, provider) in CLOUD_PROVIDERS.iter().enumerate() {
            let free_badge = if provider.free_tier { " [FREE]" } else { "" };
            println!("{}) {}{}", i + 1, provider.name, free_badge);
        }
        println!();
        
        let provider_choice = self.prompt_number(
            "Enter provider number",
            1,
            CLOUD_PROVIDERS.len()
        )?;
        
        let selected = &CLOUD_PROVIDERS[provider_choice - 1];
        
        println!("\n📋 Provider: {}", selected.name);
        
        // Get API key if needed
        let api_key = if selected.needs_api_key {
            println!("\nThis provider requires an API key.");
            self.show_provider_instructions(selected.name);
            
            self.prompt_secret("Enter your API key")?
        } else {
            String::new()
        };

        // Test the configuration
        println!("\n🧪 Testing configuration...");
        let test_passed = self.test_cloud_config(selected, &api_key).await;
        
        if test_passed {
            println!("✅ Configuration works!");
        } else {
            println!("⚠️  Could not verify configuration (this might be OK)");
        }

        println!("\n✅ Cloud provider configured!");
        println!("Provider: {}", selected.name);
        println!("Model: {}\n", selected.default_model);
        
        Ok((selected.base_url.to_string(), selected.default_model.to_string(), api_key))
    }
    
    fn show_provider_instructions(&self, name: &str) {
        match name {
            "Kimi (Moonshot) - RECOMMENDED" => {
                println!("Get your key at: https://platform.moonshot.cn");
            }
            "Groq - Ultra Fast" => {
                println!("Get your key at: https://console.groq.com");
            }
            "OpenAI (GPT-4o) - Popular" => {
                println!("Get your key at: https://platform.openai.com");
            }
            "OpenRouter - Universal" => {
                println!("Get your key at: https://openrouter.ai");
            }
            "Anthropic (Claude) - Premium" => {
                println!("Get your key at: https://console.anthropic.com");
            }
            "DeepSeek - China Optimized" => {
                println!("Get your key at: https://platform.deepseek.com");
            }
            "Together AI - Open Source" => {
                println!("Get your key at: https://api.together.xyz");
            }
            "Fireworks AI - Fast" => {
                println!("Get your key at: https://app.fireworks.ai");
            }
            "Cohere - Command R+" => {
                println!("Get your key at: https://dashboard.cohere.com");
            }
            _ => {}
        }
        println!();
    }

    fn select_deployment_type(&self) -> Result<DeploymentType> {
        println!("Where do you want to run your AI model?\n");
        println!("1) Local (Ollama) - FREE, private, runs on your computer");
        println!("   - Requires installing Ollama");
        println!("   - Uses your GPU/RAM");
        println!("   - Works offline\n");
        
        println!("2) Cloud API - Fast, powerful, requires internet");
        println!("   - No installation needed");
        println!("   - Some have free tiers");
        println!("   - Pay-as-you-go or subscription\n");

        let choice = self.prompt_number("Enter 1 or 2", 1, 2)?;
        
        Ok(if choice == 1 {
            DeploymentType::Local
        } else {
            DeploymentType::Cloud
        })
    }

    fn check_ollama_installed(&self) -> bool {
        std::process::Command::new("ollama")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    async fn pull_ollama_model(&self, model: &str) -> bool {
        println!("Running: ollama pull {}\n", model);
        
        match tokio::process::Command::new("ollama")
            .arg("pull")
            .arg(model)
            .status()
            .await
        {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }
    
    fn check_model_exists(&self, model: &str) -> bool {
        match std::process::Command::new("ollama")
            .arg("list")
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.lines().any(|line| line.contains(model))
            }
            Err(_) => false,
        }
    }

    async fn test_cloud_config(&self, provider: &ProviderConfig, api_key: &str) -> bool {
        // Simple connectivity test
        let client = reqwest::Client::new();
        
        if provider.base_url.contains("localhost") {
            return true; // Skip for local
        }
        
        match client
            .get(format!("{}/models", provider.base_url.trim_end_matches("/v1")))
            .bearer_auth(api_key)
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    // Helper methods
    fn prompt_number(&self, prompt: &str, min: usize, max: usize) -> Result<usize> {
        loop {
            print!("{} ({}-{}): ", prompt, min, max);
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            match input.trim().parse::<usize>() {
                Ok(n) if n >= min && n <= max => return Ok(n),
                _ => println!("Please enter a number between {} and {}", min, max),
            }
        }
    }

    fn prompt_yes_no(&self, prompt: &str) -> Result<bool> {
        loop {
            print!("{} (yes/no): ", prompt);
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            match input.trim().to_lowercase().as_str() {
                "yes" | "y" => return Ok(true),
                "no" | "n" => return Ok(false),
                _ => println!("Please enter 'yes' or 'no'"),
            }
        }
    }

    fn prompt_secret(&self, prompt: &str) -> Result<String> {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        
        // Note: In production, use rpassword or similar for hidden input
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        Ok(input.trim().to_string())
    }
    
    // New setup methods for comprehensive configuration
    
    async fn setup_personality(&self) -> Result<PersonalityConfig> {
        use horcrux::agent::personality::PERSONALITIES;
        
        println!("\n🎭 STEP 2: Choose Your Agent's Personality\n");
        println!("Select a character that defines how your agent communicates:\n");
        
        for (i, p) in PERSONALITIES.iter().enumerate() {
            println!("{}) {} - {}", i + 1, p.name, p.description);
        }
        println!();
        
        let choice = self.prompt_number(
            "Enter personality number (or press Enter for Voldemort)",
            1,
            PERSONALITIES.len()
        )?;
        
        let selected = &PERSONALITIES[choice - 1];
        
        println!("\n✅ Personality selected: {}", selected.name);
        println!("   Tone: {}", selected.tone);
        println!();
        
        Ok(PersonalityConfig {
            id: selected.id.to_string(),
            name: selected.name.to_string(),
        })
    }
    
    async fn setup_integrations(&self) -> Result<IntegrationConfig> {
        println!("\n💬 STEP 3: Messaging Platform Integrations (Optional)\n");
        println!("Connect your agent to messaging platforms you use.");
        println!("You can chat with your agent from any of these apps!\n");
        
        let mut config = IntegrationConfig::default();
        
        // Telegram
        if self.prompt_yes_no("Enable Telegram Bot?")? {
            println!("\n📱 Telegram Setup:");
            println!("   1. Message @BotFather on Telegram");
            println!("   2. Create new bot with /newbot");
            println!("   3. Copy the bot token\n");
            
            let token = self.prompt_secret("Enter Telegram Bot Token")?;
            if !token.is_empty() {
                config.telegram_token = Some(token);
                println!("✅ Telegram configured\n");
            }
        }
        
        // Discord
        if self.prompt_yes_no("Enable Discord Bot?")? {
            println!("\n💜 Discord Setup:");
            println!("   1. Go to https://discord.com/developers/applications");
            println!("   2. Create New Application → Bot");
            println!("   3. Copy the bot token\n");
            
            let token = self.prompt_secret("Enter Discord Bot Token")?;
            if !token.is_empty() {
                config.discord_token = Some(token);
                println!("✅ Discord configured\n");
            }
        }
        
        // WhatsApp
        if self.prompt_yes_no("Enable WhatsApp? (Requires WhatsApp Business API)")? {
            println!("\n💚 WhatsApp Setup:");
            println!("   Options:");
            println!("   1. WhatsApp Business API (Meta official)");
            println!("   2. WhatsApp Web via QR code (unofficial, may break)\n");
            
            let phone = self.prompt_string("Enter WhatsApp phone number (with country code, e.g., +1234567890)")?;
            if !phone.is_empty() {
                config.whatsapp_phone = Some(phone);
                println!("⚠️  Note: WhatsApp integration requires additional setup.");
                println!("   See documentation for QR pairing instructions.\n");
            }
        }
        
        // Slack
        if self.prompt_yes_no("Enable Slack Bot?")? {
            println!("\n💙 Slack Setup:");
            println!("   1. Go to https://api.slack.com/apps");
            println!("   2. Create New App → From scratch");
            println!("   3. Add Bot Token Scopes: chat:write, im:history");
            println!("   4. Install to workspace and copy Bot User OAuth Token\n");
            
            let token = self.prompt_secret("Enter Slack Bot Token (starts with xoxb-)")?;
            if !token.is_empty() {
                config.slack_token = Some(token);
                println!("✅ Slack configured\n");
            }
        }
        
        // Matrix
        if self.prompt_yes_no("Enable Matrix Bot?")? {
            println!("\n🗨️  Matrix Setup:");
            println!("   1. Create account on any Matrix homeserver");
            println!("   2. Get access token from Element settings\n");
            
            let homeserver = self.prompt_string("Enter Matrix homeserver URL (e.g., https://matrix.org)")?;
            let token = self.prompt_secret("Enter Matrix Access Token")?;
            
            if !homeserver.is_empty() && !token.is_empty() {
                config.matrix_homeserver = Some(homeserver);
                config.matrix_token = Some(token);
                println!("✅ Matrix configured\n");
            }
        }
        
        // Webhook
        if self.prompt_yes_no("Enable HTTP Webhook? (for custom integrations)")? {
            println!("\n🌐 Webhook Setup:");
            println!("   This creates an HTTP endpoint for external apps to call your agent.\n");
            
            let port = self.prompt_number("Port number", 1024, 65535)?;
            config.webhook_port = Some(port);
            println!("✅ Webhook server will run on port {}\n", port);
        }
        
        let active_count = config.active_count();
        if active_count == 0 {
            println!("ℹ️  No messaging integrations enabled.");
            println!("   You can still use 'horcrux agent' in the terminal.\n");
        } else {
            println!("✅ {} messaging platform(s) configured!\n", active_count);
        }
        
        Ok(config)
    }
    
    async fn setup_server(&self) -> Result<ServerConfig> {
        println!("\n🌐 STEP 4: API Server & Web Interface (Optional)\n");
        
        let mut config = ServerConfig::default();
        
        // REST API
        if self.prompt_yes_no("Enable REST API Server?")? {
            println!("\n   This allows other apps to interact with your agent via HTTP.\n");
            
            let port = self.prompt_number("API Port", 1024, 65535)?;
            config.api_enabled = true;
            config.api_port = port;
            
            println!("   API endpoints:");
            println!("   - POST /chat - Send message to agent");
            println!("   - GET /status - Check agent status");
            println!("   - POST /search - Search knowledge base");
            println!("   - GET /skills - List available skills\n");
        }
        
        // Web UI
        if self.prompt_yes_no("Enable Web Interface? (Browser-based chat)")? {
            println!("\n   This creates a web-based chat interface you can access from any browser.\n");
            
            let port = self.prompt_number("Web UI Port", 1024, 65535)?;
            config.webui_enabled = true;
            config.webui_port = port;
            
            println!("   Web UI will be available at: http://localhost:{}\n", port);
        }
        
        // MCP Server
        if self.prompt_yes_no("Enable MCP Server? (Model Context Protocol for Claude Desktop)")? {
            println!("\n   This allows Claude Desktop to use horcrux as a memory source.\n");
            config.mcp_enabled = true;
            println!("   Add to Claude Desktop config to enable:\n");
            println!("   {{");
            println!("     \"mcpServers\": {{");
            println!("       \"horcrux\": {{");
            println!("         \"command\": \"horcrux\",");
            println!("         \"args\": [\"mcp\"]");
            println!("       }}");
            println!("     }}");
            println!("   }}\n");
        }
        
        Ok(config)
    }
    
    async fn setup_advanced(&self) -> Result<AdvancedConfig> {
        println!("\n⚙️  STEP 5: Advanced Features (Optional)\n");
        
        let mut config = AdvancedConfig::default();
        
        // Scheduled Tasks
        if self.prompt_yes_no("Enable scheduled tasks? (Run skills on a schedule)")? {
            println!("\n   Example: Check news every morning, backup files weekly\n");
            config.scheduled_tasks_enabled = true;
        }
        
        // Multi-agent mode
        if self.prompt_yes_no("Enable multi-agent mode? (Specialized agents for different tasks)")? {
            println!("\n   This creates specialized agents:");
            println!("   - Researcher: For gathering information");
            println!("   - Coder: For programming tasks");
            println!("   - Writer: For content creation");
            println!("   - Analyzer: For data analysis\n");
            config.multi_agent_enabled = true;
        }
        
        // Memory settings
        println!("\n🧠 Memory Settings:\n");
        
        let memory_levels = vec![
            ("Basic", "Conversations only, no long-term memory"),
            ("Standard", "Conversations + document search (recommended)"),
            ("Advanced", "Full memory: conversations, facts, user preferences"),
        ];
        
        for (i, (name, desc)) in memory_levels.iter().enumerate() {
            println!("{}) {} - {}", i + 1, name, desc);
        }
        
        let memory_choice = self.prompt_number("Memory level", 1, 3)?;
        config.memory_level = match memory_choice {
            1 => "basic",
            3 => "advanced",
            _ => "standard",
        }.to_string();
        
        // Auto-skill creation
        if self.prompt_yes_no("Auto-create skills? (Save workflows automatically)")? {
            println!("\n   The agent will detect repetitive tasks and offer to save them as skills.\n");
            config.auto_skill_creation = true;
        }
        
        // Data export/backup
        if self.prompt_yes_no("Enable automatic backups?")? {
            println!("\n   Backup frequency:\n");
            println!("   1) Daily");
            println!("   2) Weekly");
            println!("   3) On exit only\n");
            
            let backup_choice = self.prompt_number("Select frequency", 1, 3)?;
            config.backup_frequency = match backup_choice {
                1 => "daily",
                2 => "weekly",
                _ => "on_exit",
            }.to_string();
        }
        
        Ok(config)
    }
    
    async fn save_complete_config(
        &self,
        model: ModelConfig,
        personality: PersonalityConfig,
        integrations: IntegrationConfig,
        server: ServerConfig,
        advanced: AdvancedConfig,
    ) -> Result<()> {
        let mut config_lines = vec![
            "# Horcrux Configuration".to_string(),
            "# Generated by setup wizard".to_string(),
            "".to_string(),
            "# === AI Model ===".to_string(),
            format!("HORCRUX_LLM_URL={}", model.base_url),
            format!("HORCRUX_LLM_MODEL={}", model.model),
            format!("HORCRUX_LLM_API_KEY={}", model.api_key),
            "".to_string(),
        ];
        
        // Add personality
        config_lines.push("# === Agent Personality ===".to_string());
        config_lines.push(format!("HORCRUX_AGENT_NAME={}", personality.name));
        config_lines.push(format!("HORCRUX_PERSONALITY={}", personality.id));
        config_lines.push("".to_string());
        
        // Add integrations
        if integrations.has_any() {
            config_lines.push("# === Messaging Integrations ===".to_string());
            
            if let Some(token) = integrations.telegram_token {
                config_lines.push(format!("TELEGRAM_BOT_TOKEN={}", token));
            }
            if let Some(token) = integrations.discord_token {
                config_lines.push(format!("DISCORD_BOT_TOKEN={}", token));
            }
            if let Some(phone) = integrations.whatsapp_phone {
                config_lines.push(format!("WHATSAPP_PHONE={}", phone));
            }
            if let Some(token) = integrations.slack_token {
                config_lines.push(format!("SLACK_BOT_TOKEN={}", token));
            }
            if let Some(homeserver) = integrations.matrix_homeserver {
                config_lines.push(format!("MATRIX_HOMESERVER={}", homeserver));
            }
            if let Some(token) = integrations.matrix_token {
                config_lines.push(format!("MATRIX_ACCESS_TOKEN={}", token));
            }
            if let Some(port) = integrations.webhook_port {
                config_lines.push(format!("WEBHOOK_PORT={}", port));
            }
            config_lines.push("".to_string());
        }
        
        // Add server config
        if server.has_any() {
            config_lines.push("# === Server Configuration ===".to_string());
            
            if server.api_enabled {
                config_lines.push(format!("API_ENABLED=true"));
                config_lines.push(format!("API_PORT={}", server.api_port));
            }
            if server.webui_enabled {
                config_lines.push(format!("WEBUI_ENABLED=true"));
                config_lines.push(format!("WEBUI_PORT={}", server.webui_port));
            }
            if server.mcp_enabled {
                config_lines.push(format!("MCP_ENABLED=true"));
            }
            config_lines.push("".to_string());
        }
        
        // Add advanced config
        config_lines.push("# === Advanced Settings ===".to_string());
        config_lines.push(format!("MEMORY_LEVEL={}", advanced.memory_level));
        config_lines.push(format!("AUTO_SKILL_CREATION={}", advanced.auto_skill_creation));
        if !advanced.backup_frequency.is_empty() {
            config_lines.push(format!("BACKUP_FREQUENCY={}", advanced.backup_frequency));
        }
        if advanced.multi_agent_enabled {
            config_lines.push(format!("MULTI_AGENT_ENABLED=true"));
        }
        if advanced.scheduled_tasks_enabled {
            config_lines.push(format!("SCHEDULED_TASKS_ENABLED=true"));
        }
        
        std::fs::write(".env", config_lines.join("\n"))?;
        println!("\n💾 Complete configuration saved to .env");
        
        Ok(())
    }
    
    fn prompt_string(&self, prompt: &str) -> Result<String> {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }
}

// Configuration structs
struct ModelConfig {
    base_url: String,
    model: String,
    api_key: String,
}

struct PersonalityConfig {
    id: String,
    name: String,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            id: "voldemort".to_string(),
            name: "Voldemort".to_string(),
        }
    }
}

#[derive(Default)]
struct IntegrationConfig {
    telegram_token: Option<String>,
    discord_token: Option<String>,
    whatsapp_phone: Option<String>,
    slack_token: Option<String>,
    matrix_homeserver: Option<String>,
    matrix_token: Option<String>,
    webhook_port: Option<usize>,
}

impl IntegrationConfig {
    fn has_any(&self) -> bool {
        self.telegram_token.is_some() ||
        self.discord_token.is_some() ||
        self.whatsapp_phone.is_some() ||
        self.slack_token.is_some() ||
        self.matrix_homeserver.is_some() ||
        self.webhook_port.is_some()
    }
    
    fn active_count(&self) -> usize {
        let mut count = 0;
        if self.telegram_token.is_some() { count += 1; }
        if self.discord_token.is_some() { count += 1; }
        if self.whatsapp_phone.is_some() { count += 1; }
        if self.slack_token.is_some() { count += 1; }
        if self.matrix_homeserver.is_some() { count += 1; }
        if self.webhook_port.is_some() { count += 1; }
        count
    }
}

#[derive(Default)]
struct ServerConfig {
    api_enabled: bool,
    api_port: usize,
    webui_enabled: bool,
    webui_port: usize,
    mcp_enabled: bool,
}

impl ServerConfig {
    fn has_any(&self) -> bool {
        self.api_enabled || self.webui_enabled || self.mcp_enabled
    }
}

#[derive(Default)]
struct AdvancedConfig {
    memory_level: String,
    auto_skill_creation: bool,
    backup_frequency: String,
    multi_agent_enabled: bool,
    scheduled_tasks_enabled: bool,
}

enum DeploymentType {
    Local,
    Cloud,
}
