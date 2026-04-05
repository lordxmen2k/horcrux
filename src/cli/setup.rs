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
/// Available models for each provider
#[derive(Debug, Clone)]
pub struct ModelOption {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub context: &'static str,
}

/// Provider with available models
#[derive(Debug, Clone)]
pub struct ProviderWithModels {
    pub config: ProviderConfig,
    pub models: &'static [ModelOption],
}

// OpenAI Models
pub const OPENAI_MODELS: &[ModelOption] = &[
    ModelOption { id: "gpt-4o", name: "GPT-4o", description: "Most capable multimodal model", context: "128K" },
    ModelOption { id: "gpt-4o-mini", name: "GPT-4o Mini", description: "Fast and affordable", context: "128K" },
    ModelOption { id: "gpt-4-turbo", name: "GPT-4 Turbo", description: "High capability, lower price", context: "128K" },
    ModelOption { id: "gpt-3.5-turbo", name: "GPT-3.5 Turbo", description: "Fast and cost-effective", context: "16K" },
];

// Anthropic Models
pub const ANTHROPIC_MODELS: &[ModelOption] = &[
    ModelOption { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet", description: "⭐ Best balance of intelligence and speed", context: "200K" },
    ModelOption { id: "claude-3-opus-20240229", name: "Claude 3 Opus", description: "Most powerful for complex tasks", context: "200K" },
    ModelOption { id: "claude-3-haiku-20240307", name: "Claude 3 Haiku", description: "Fastest responses", context: "200K" },
];

// Kimi Models
pub const KIMI_MODELS: &[ModelOption] = &[
    ModelOption { id: "moonshot-v1-8k", name: "Moonshot v1 8K", description: "Fast responses, basic tasks", context: "8K" },
    ModelOption { id: "moonshot-v1-32k", name: "Moonshot v1 32K", description: "⭐ RECOMMENDED - Good balance", context: "32K" },
    ModelOption { id: "moonshot-v1-128k", name: "Moonshot v1 128K", description: "Long context, complex documents", context: "128K" },
];

// Groq Models
pub const GROQ_MODELS: &[ModelOption] = &[
    ModelOption { id: "llama-3.1-70b-versatile", name: "Llama 3.1 70B", description: "⭐ Powerful, fast inference", context: "128K" },
    ModelOption { id: "llama-3.1-8b-instant", name: "Llama 3.1 8B", description: "Very fast, efficient", context: "128K" },
    ModelOption { id: "mixtral-8x7b-32768", name: "Mixtral 8x7B", description: "Good reasoning", context: "32K" },
    ModelOption { id: "gemma2-9b-it", name: "Gemma 2 9B", description: "Lightweight, fast", context: "8K" },
];

// DeepSeek Models
pub const DEEPSEEK_MODELS: &[ModelOption] = &[
    ModelOption { id: "deepseek-chat", name: "DeepSeek Chat", description: "⭐ General purpose, excellent reasoning", context: "64K" },
    ModelOption { id: "deepseek-coder", name: "DeepSeek Coder", description: "Optimized for coding tasks", context: "64K" },
];

// OpenRouter Models (subset of popular ones)
pub const OPENROUTER_MODELS: &[ModelOption] = &[
    ModelOption { id: "anthropic/claude-3.5-sonnet", name: "Claude 3.5 Sonnet", description: "⭐ Best overall via OpenRouter", context: "200K" },
    ModelOption { id: "openai/gpt-4o", name: "GPT-4o", description: "OpenAI's best", context: "128K" },
    ModelOption { id: "meta-llama/llama-3.1-70b-instruct", name: "Llama 3.1 70B", description: "Open source champion", context: "128K" },
    ModelOption { id: "google/gemini-1.5-pro", name: "Gemini 1.5 Pro", description: "Massive context", context: "1M" },
];

// Together AI Models
pub const TOGETHER_MODELS: &[ModelOption] = &[
    ModelOption { id: "meta-llama/Llama-3.1-70B-Instruct-Turbo", name: "Llama 3.1 70B", description: "⭐ Fast and capable", context: "128K" },
    ModelOption { id: "meta-llama/Llama-3.1-8B-Instruct-Turbo", name: "Llama 3.1 8B", description: "Efficient", context: "128K" },
    ModelOption { id: "mistralai/Mixtral-8x22B-Instruct-v0.1", name: "Mixtral 8x22B", description: "Powerful MoE", context: "64K" },
];

// Fireworks Models
pub const FIREWORKS_MODELS: &[ModelOption] = &[
    ModelOption { id: "accounts/fireworks/models/llama-v3p1-70b-instruct", name: "Llama 3.1 70B", description: "Fast inference", context: "128K" },
    ModelOption { id: "accounts/fireworks/models/llama-v3p1-8b-instruct", name: "Llama 3.1 8B", description: "Efficient", context: "128K" },
    ModelOption { id: "accounts/fireworks/models/mixtral-8x22b-instruct", name: "Mixtral 8x22B", description: "Strong reasoning", context: "64K" },
];

// Cohere Models
pub const COHERE_MODELS: &[ModelOption] = &[
    ModelOption { id: "command-r-plus", name: "Command R+", description: "⭐ Best for RAG and tool use", context: "128K" },
    ModelOption { id: "command-r", name: "Command R", description: "Good balance", context: "128K" },
    ModelOption { id: "command", name: "Command", description: "General purpose", context: "4K" },
];

// AI21 Models
pub const AI21_MODELS: &[ModelOption] = &[
    ModelOption { id: "jamba-1.5-large", name: "Jamba 1.5 Large", description: "⭐ Long context specialist", context: "256K" },
    ModelOption { id: "jamba-1.5-mini", name: "Jamba 1.5 Mini", description: "Efficient", context: "256K" },
];

// Azure Models
pub const AZURE_MODELS: &[ModelOption] = &[
    ModelOption { id: "gpt-4", name: "GPT-4", description: "High capability", context: "8K" },
    ModelOption { id: "gpt-4-32k", name: "GPT-4 32K", description: "Extended context", context: "32K" },
    ModelOption { id: "gpt-35-turbo", name: "GPT-3.5 Turbo", description: "Cost-effective", context: "4K" },
];

pub const CLOUD_PROVIDERS: &[ProviderConfig] = &[
    // TIER 1: Recommended / Best Experience
    ProviderConfig {
        name: "Kimi (Moonshot) - RECOMMENDED",
        base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-32k",
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
        default_model: "gpt-4o",
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

    pub async fn run(&self, section: Option<&str>) -> Result<()> {
        // Handle quick section-specific setup
        if let Some(sec) = section {
            return self.run_section_setup(sec).await;
        }
        
        // Full setup wizard
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
    
    /// Run section-specific quick setup
    async fn run_section_setup(&self, section: &str) -> Result<()> {
        use horcrux::config::Config;
        
        match section {
            "llm" => self.quick_setup_llm().await,
            "images" => self.quick_setup_images().await,
            "telegram" => self.quick_setup_telegram().await,
            "show" => {
                let config = Config::load().unwrap_or_default();
                println!("\n📋 Current Configuration\n");
                println!("[llm]");
                println!("  provider = {}", config.llm.provider.as_deref().unwrap_or("not set"));
                println!("  model    = {}", config.llm.model.as_deref().unwrap_or("not set"));
                println!("  api_key  = {}", mask_key(config.llm.api_key.as_deref().unwrap_or("")));
                println!("\n[images]");
                println!("  provider = {}", config.images.provider.as_deref().unwrap_or("not set"));
                println!("  api_key  = {}", mask_key(config.images.api_key.as_deref().unwrap_or("")));
                println!("\n[telegram]");
                println!("  bot_token = {}", mask_key(config.telegram.bot_token.as_deref().unwrap_or("")));
                println!();
                Ok(())
            }
            _ => {
                println!("Unknown section '{}'. Try: llm, images, telegram, show", section);
                Ok(())
            }
        }
    }
    
    /// Quick LLM setup using config.toml
    async fn quick_setup_llm(&self) -> Result<()> {
        use horcrux::config::Config;
        let mut config = Config::load().unwrap_or_default();
        
        println!("\n🤖 Quick LLM Setup\n");
        
        // Provider
        println!("Select provider:");
        println!("  1. Moonshot (Kimi)");
        println!("  2. OpenAI");
        println!("  3. Anthropic");
        println!("  4. Ollama (local)");
        
        let current = config.llm.provider.as_deref().unwrap_or("");
        print!("Choice (current: {}): ", current);
        std::io::stdout().flush()?;
        let mut choice = String::new();
        std::io::stdin().read_line(&mut choice)?;
        
        if !choice.trim().is_empty() {
            config.llm.provider = Some(match choice.trim() {
                "1" => "kimi".to_string(),
                "2" => "openai".to_string(),
                "3" => "anthropic".to_string(),
                "4" => "ollama".to_string(),
                _ => choice.trim().to_string(),
            });
            
            // Set defaults
            match config.llm.provider.as_deref() {
                Some("kimi") => {
                    config.llm.base_url = Some("https://api.moonshot.cn/v1".to_string());
                    config.llm.model = Some("moonshot-v1-8k".to_string());
                }
                Some("openai") => {
                    config.llm.base_url = Some("https://api.openai.com/v1".to_string());
                    config.llm.model = Some("gpt-4o-mini".to_string());
                }
                Some("anthropic") => {
                    config.llm.base_url = Some("https://api.anthropic.com/v1".to_string());
                    config.llm.model = Some("claude-3-5-sonnet".to_string());
                }
                Some("ollama") => {
                    config.llm.base_url = Some("http://localhost:11434/v1".to_string());
                    config.llm.model = Some("qwen2.5:7b".to_string());
                }
                _ => {}
            }
        }
        
        // API Key
        let masked = mask_key(config.llm.api_key.as_deref().unwrap_or(""));
        print!("API Key (current: {}): ", masked);
        std::io::stdout().flush()?;
        let mut key = String::new();
        std::io::stdin().read_line(&mut key)?;
        if !key.trim().is_empty() {
            config.llm.api_key = Some(key.trim().to_string());
        }
        
        config.save()?;
        println!("\n✅ LLM configuration saved to ~/.horcrux/config.toml");
        Ok(())
    }
    
    /// Quick image provider setup
    async fn quick_setup_images(&self) -> Result<()> {
        use horcrux::config::{Config, IMAGE_PROVIDER_PRESETS};
        let mut config = Config::load().unwrap_or_default();
        
        println!("\n🖼️  Image Search Setup\n");
        
        for (i, preset) in IMAGE_PROVIDER_PRESETS.iter().enumerate() {
            println!("  {}. {}", i + 1, preset.key_name);
            println!("     {}", preset.description);
            println!("     Rate: {}\n", preset.rate_limit);
        }
        
        let current = config.images.provider.as_deref().unwrap_or("");
        print!("Provider (current: {}): ", current);
        std::io::stdout().flush()?;
        let mut choice = String::new();
        std::io::stdin().read_line(&mut choice)?;
        
        if !choice.trim().is_empty() {
            config.images.provider = Some(match choice.trim() {
                "1" => "unsplash".to_string(),
                "2" => "pixabay".to_string(),
                "3" => "pexels".to_string(),
                _ => choice.trim().to_string(),
            });
        }
        
        let masked = mask_key(config.images.api_key.as_deref().unwrap_or(""));
        print!("API Key (current: {}): ", masked);
        std::io::stdout().flush()?;
        let mut key = String::new();
        std::io::stdin().read_line(&mut key)?;
        if !key.trim().is_empty() {
            config.images.api_key = Some(key.trim().to_string());
        }
        
        config.save()?;
        println!("\n✅ Image provider saved to ~/.horcrux/config.toml");
        Ok(())
    }
    
    /// Quick Telegram setup
    async fn quick_setup_telegram(&self) -> Result<()> {
        use horcrux::config::Config;
        let mut config = Config::load().unwrap_or_default();
        
        println!("\n📱 Telegram Setup\n");
        println!("Get a bot token from @BotFather on Telegram\n");
        
        let masked = mask_key(config.telegram.bot_token.as_deref().unwrap_or(""));
        print!("Bot Token (current: {}): ", masked);
        std::io::stdout().flush()?;
        let mut token = String::new();
        std::io::stdin().read_line(&mut token)?;
        
        if !token.trim().is_empty() {
            config.telegram.bot_token = Some(token.trim().to_string());
            config.telegram.enabled = true;
        }
        
        config.save()?;
        println!("\n✅ Telegram configuration saved");
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
        println!("\n📦 Local Model Setup (Ollama)\n");
        
        // Check if Ollama is installed
        println!("Step 1: Checking if Ollama is installed...\n");
        
        let ollama_installed = self.check_ollama_installed();
        
        if !ollama_installed {
            println!("❌ Ollama is not installed.\n");
            println!("📋 Install from: https://ollama.com/download\n");
            return Err(anyhow::anyhow!("Ollama not installed"));
        }
        
        println!("✅ Ollama is installed!\n");
        
        // Fetch available models from Ollama
        println!("📋 Checking available models...\n");
        let available_models = self.get_local_ollama_models().await;
        
        let model = {
            // Show dropdown of available models
            println!("Popular Ollama models:\n");
            
            // Show recommended models with download status
            let recommended = vec![
                ("qwen2.5:7b", "⭐ RECOMMENDED - Best balance of speed and capability"),
                ("llama3.1:8b", "Very popular, good tool use"),
                ("mistral:7b", "Fast and capable"),
                ("qwen2.5:14b", "More powerful, needs 16GB RAM"),
                ("deepseek-r1:8b", "Excellent reasoning"),
                ("llama3.2:3b", "Lightweight, 4GB RAM"),
            ];
            
            // Build display list
            let mut display_list: Vec<(String, String)> = Vec::new();
            for (name, desc) in &recommended {
                let is_downloaded = self.check_model_exists(name);
                let status = if is_downloaded { "✅" } else { "📥" };
                let display = format!("{} {} - {}", status, name, desc);
                display_list.push((name.to_string(), display));
            }
            
            // Show the list
            for (i, (name, display)) in display_list.iter().enumerate() {
                println!("{}) {}", i + 1, display);
            }
            println!("{}) 📝 Custom model (type your own)", display_list.len() + 1);
            
            let choice = self.prompt_number(
                "Select model",
                1,
                display_list.len() + 1
            )?;
            
            if choice == display_list.len() + 1 {
                // Custom model
                print!("Enter model name (e.g., codellama:7b): ");
                std::io::stdout().flush()?;
                let mut custom_model = String::new();
                std::io::stdin().read_line(&mut custom_model)?;
                custom_model.trim().to_string()
            } else {
                display_list[choice - 1].0.clone()
            }
        };
        
        // Check if model needs to be downloaded
        if !self.check_model_exists(&model) {
            println!("\n📥 Model '{}' not found locally.", model);
            println!("   Run: ollama pull {}", model);
            println!("   Then restart horcrux.\n");
        }
        
        Ok(("http://localhost:11434/v1".to_string(), model, "ollama".to_string()))
    }
    
    async fn setup_cloud_model(&self) -> Result<(String, String, String)> {
        println!("\n☁️  Cloud API Setup\n");
        
        // Step 1: Select provider
        println!("Step 1: Select a cloud provider\n");
        
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
        
        let selected_provider = &CLOUD_PROVIDERS[provider_choice - 1];
        println!("\n✅ Provider: {}\n", selected_provider.name);
        
        // Step 2: Select model (NEW - right after provider!)
        println!("Step 2: Select a model\n");
        let models = self.get_models_for_provider(selected_provider.name);
        
        let model_id = if !models.is_empty() {
            println!("Available models:\n");
            
            for (i, model) in models.iter().enumerate() {
                let star = if model.description.contains("⭐") { "⭐ " } else { "" };
                println!("{}) {}{} ({} context)", 
                    i + 1, 
                    star,
                    model.name, 
                    model.context
                );
                println!("   {}\n", model.description);
            }
            println!("{}) 📝 Custom model (type your own)", models.len() + 1);
            
            let model_choice = self.prompt_number(
                "Select model",
                1,
                models.len() + 1
            )?;
            
            if model_choice == models.len() + 1 {
                // Custom model
                print!("Enter model ID (e.g., gpt-4-turbo-preview): ");
                std::io::stdout().flush()?;
                let mut custom_model = String::new();
                std::io::stdin().read_line(&mut custom_model)?;
                custom_model.trim().to_string()
            } else {
                models[model_choice - 1].id.to_string()
            }
        } else {
            // No predefined models - ask manually
            print!("Enter model ID (e.g., gpt-4): ");
            std::io::stdout().flush()?;
            let mut model = String::new();
            std::io::stdin().read_line(&mut model)?;
            model.trim().to_string()
        };
        
        println!("\n✅ Selected model: {}\n", model_id);
        
        // Step 3: Get API key
        println!("Step 3: API Key\n");
        let api_key = if selected_provider.needs_api_key {
            self.show_provider_instructions(selected_provider.name);
            self.prompt_secret("Enter your API key")?
        } else {
            String::new()
        };

        // Test configuration
        println!("\n🧪 Testing configuration...");
        let test_passed = self.test_cloud_config_with_model(selected_provider, &model_id, &api_key).await;
        
        if test_passed {
            println!("✅ Configuration works!");
        } else {
            println!("⚠️  Could not verify (this might be OK if the API is working)");
        }

        println!("\n✅ Cloud configuration complete!");
        println!("   Provider: {}", selected_provider.name);
        println!("   Model: {}\n", model_id);
        
        let base_url = selected_provider.base_url.to_string();
        Ok((base_url, model_id, api_key))
    }
    
    /// Get available models for a provider
    fn get_models_for_provider(&self, provider_name: &str) -> &'static [ModelOption] {
        match provider_name {
            "OpenAI (GPT-4o) - Popular" => OPENAI_MODELS,
            "Anthropic (Claude) - Premium" => ANTHROPIC_MODELS,
            "Kimi (Moonshot) - RECOMMENDED" => KIMI_MODELS,
            "Groq - Ultra Fast" => GROQ_MODELS,
            "DeepSeek - China Optimized" => DEEPSEEK_MODELS,
            "OpenRouter - Universal" => OPENROUTER_MODELS,
            "Together AI - Open Source" => TOGETHER_MODELS,
            "Fireworks AI - Fast" => FIREWORKS_MODELS,
            "Cohere - Command R+" => COHERE_MODELS,
            "AI21 Labs - Jurassic" => AI21_MODELS,
            "Azure OpenAI - Enterprise" => AZURE_MODELS,
            _ => &[],
        }
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
    
    /// Get list of locally installed Ollama models
    async fn get_local_ollama_models(&self) -> Vec<String> {
        match tokio::process::Command::new("ollama")
            .arg("list")
            .output()
            .await
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.lines()
                    .skip(1) // Skip header line
                    .filter_map(|line| {
                        // Parse lines like: "qwen2.5:7b    845db89...    4.4 GB"
                        line.split_whitespace().next().map(|s| s.to_string())
                    })
                    .collect()
            }
            Err(_) => Vec::new(),
        }
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
        self.test_cloud_config_with_model(provider, provider.default_model, api_key).await
    }
    
    async fn test_cloud_config_with_model(&self, provider: &ProviderConfig, model: &str, api_key: &str) -> bool {
        // Simple connectivity test
        let client = reqwest::Client::new();
        
        if provider.base_url.contains("localhost") {
            return true; // Skip for local
        }
        
        // Try to list models or do a simple completion test
        match client
            .get(format!("{}/models", provider.base_url.trim_end_matches("/v1")))
            .bearer_auth(api_key)
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => {
                // Fallback: try a simple chat completion
                let request = serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "Hi"}],
                    "max_tokens": 5
                });
                
                match client
                    .post(format!("{}/chat/completions", provider.base_url.trim_end_matches("/v1")))
                    .bearer_auth(api_key)
                    .json(&request)
                    .send()
                    .await
                {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                }
            }
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


// Helper function to mask API keys for display
fn mask_key(key: &str) -> String {
    if key.is_empty() {
        return "not set".to_string();
    }
    if key.len() < 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len()-4..])
}

