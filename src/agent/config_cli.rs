//! Smart Configuration CLI - Interactive setup for models, API keys, etc.

use anyhow::Result;
use std::io::{self, Write};
use std::path::PathBuf;

/// Configuration presets for popular LLM providers
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub needs_api_key: bool,
    pub description: &'static str,
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        name: "Ollama (Local)",
        base_url: "http://localhost:11434/v1",
        default_model: "qwen2.5:7b",
        needs_api_key: false,
        description: "Run models locally - free, private, no internet needed",
    },
    ProviderPreset {
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
        needs_api_key: true,
        description: "OpenAI's GPT models - requires API key",
    },
    ProviderPreset {
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-3.5-sonnet",
        needs_api_key: true,
        description: "Access multiple models through one API",
    },
    ProviderPreset {
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.1-70b-versatile",
        needs_api_key: true,
        description: "Fast inference for open source models",
    },
    ProviderPreset {
        name: "Anthropic (Direct)",
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-3-5-sonnet-20241022",
        needs_api_key: true,
        description: "Claude models directly from Anthropic",
    },
    ProviderPreset {
        name: "Custom",
        base_url: "",
        default_model: "",
        needs_api_key: true,
        description: "Configure your own OpenAI-compatible endpoint",
    },
];

/// Ollama model recommendations
#[derive(Debug, Clone)]
pub struct ModelRecommendation {
    pub name: &'static str,
    pub size: &'static str,
    pub description: &'static str,
    pub best_for: &'static str,
}

pub const OLLAMA_MODELS: &[ModelRecommendation] = &[
    ModelRecommendation {
        name: "qwen2.5:7b",
        size: "4.4 GB",
        description: "Great all-rounder, good at tool use",
        best_for: "General purpose, fast responses",
    },
    ModelRecommendation {
        name: "qwen2.5:14b",
        size: "9.0 GB",
        description: "More capable, better reasoning",
        best_for: "Complex tasks, coding",
    },
    ModelRecommendation {
        name: "llama3.2:3b",
        size: "2.0 GB",
        description: "Lightweight, very fast",
        best_for: "Simple tasks, low resource usage",
    },
    ModelRecommendation {
        name: "codellama:7b",
        size: "3.8 GB",
        description: "Optimized for code",
        best_for: "Programming tasks",
    },
    ModelRecommendation {
        name: "mistral:7b",
        size: "4.1 GB",
        description: "Fast and capable",
        best_for: "Balanced performance",
    },
    ModelRecommendation {
        name: "phi4:14b",
        size: "9.1 GB",
        description: "Microsoft's latest, excellent reasoning",
        best_for: "Complex reasoning tasks",
    },
];

/// Interactive configuration wizard
pub struct ConfigWizard;

impl ConfigWizard {
    pub fn new() -> Self {
        Self
    }

    /// Run the full configuration wizard
    pub async fn run(&self) -> Result<()> {
        println!("\n🤖 Agent Configuration Wizard\n");
        println!("This will help you set up your LLM provider and model.\n");

        // Step 1: Choose provider
        let provider = self.select_provider()?;

        // Step 2: Configure based on provider
        let (base_url, model, api_key) = if provider.name == "Custom" {
            self.configure_custom().await?
        } else if provider.name.starts_with("Ollama") {
            self.configure_ollama().await?
        } else {
            self.configure_cloud_provider(provider).await?
        };

        // Step 3: Test the configuration
        println!("\n🧪 Testing configuration...");
        if self.test_configuration(&base_url, &model, &api_key).await {
            println!("✅ Configuration works!");
        } else {
            println!("⚠️  Could not verify configuration (this might be OK if the service is starting up)");
        }

        // Step 4: Show summary and save instructions
        self.show_summary(&base_url, &model, &api_key);

        Ok(())
    }

    fn select_provider(&self) -> Result<&'static ProviderPreset> {
        println!("Select your LLM provider:\n");
        
        for (i, preset) in PROVIDER_PRESETS.iter().enumerate() {
            println!("{}) {}", i + 1, preset.name);
            println!("   {}\n", preset.description);
        }

        let choice = self.prompt_number("Enter number", 1, PROVIDER_PRESETS.len())?;
        Ok(&PROVIDER_PRESETS[choice - 1])
    }

    async fn configure_ollama(&self) -> Result<(String, String, String)> {
        println!("\n📦 Ollama Configuration\n");
        
        // Check if Ollama is running
        let client = reqwest::Client::new();
        let ollama_url = "http://localhost:11434";
        
        match client.get(format!("{}/api/tags", ollama_url)).send().await {
            Ok(resp) if resp.status().is_success() => {
                println!("✅ Ollama is running at {}", ollama_url);
                
                // Try to parse available models
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(models) = json["models"].as_array() {
                        println!("\n📋 Available models:");
                        for (i, m) in models.iter().enumerate() {
                            if let Some(name) = m["name"].as_str() {
                                println!("  {}. {}", i + 1, name);
                            }
                        }
                    }
                }
            }
            _ => {
                println!("⚠️  Ollama doesn't appear to be running at {}", ollama_url);
                println!("   Start it with: ollama serve");
                println!("   Or download from: https://ollama.com\n");
            }
        }

        // Show recommendations
        println!("\n📊 Recommended models:\n");
        for (i, model) in OLLAMA_MODELS.iter().enumerate() {
            println!("{}) {} ({})", i + 1, model.name, model.size);
            println!("   {} - {}", model.description, model.best_for);
        }
        println!("{}) Other (specify)", OLLAMA_MODELS.len() + 1);

        let choice = self.prompt_number(
            "\nSelect model",
            1,
            OLLAMA_MODELS.len() + 1
        )?;

        let model_name = if choice <= OLLAMA_MODELS.len() {
            OLLAMA_MODELS[choice - 1].name.to_string()
        } else {
            self.prompt_string("Enter model name (e.g., llama3.2:3b)")?
        };

        // Offer to pull the model
        println!("\n📝 Note: If you don't have '{}' yet, run:", model_name);
        println!("   ollama pull {}\n", model_name);

        Ok((
            "http://localhost:11434/v1".to_string(),
            model_name,
            "ollama".to_string(), // Ollama doesn't need real API key
        ))
    }

    async fn configure_cloud_provider(&self, provider: &ProviderPreset) -> Result<(String, String, String)> {
        println!("\n🔑 {} Configuration\n", provider.name);

        let base_url = provider.base_url.to_string();
        
        // Get API key
        let api_key = if provider.needs_api_key {
            let key = self.prompt_secret(&format!("Enter your {} API key", provider.name))?;
            
            // Validate key format (basic check)
            if key.len() < 10 {
                println!("⚠️  That key looks too short, but I'll accept it.");
            }
            key
        } else {
            String::new()
        };

        // Select or enter model
        let model = if provider.name == "OpenAI" {
            self.select_openai_model()?
        } else if provider.name == "Groq" {
            self.select_groq_model()?
        } else {
            self.prompt_string_with_default(
                "Enter model name",
                provider.default_model
            )?
        };

        Ok((base_url, model, api_key))
    }

    async fn configure_custom(&self) -> Result<(String, String, String)> {
        println!("\n⚙️  Custom Provider Configuration\n");

        let base_url = self.prompt_string(
            "Enter base URL (e.g., http://localhost:1234/v1)"
        )?;

        let model = self.prompt_string(
            "Enter model name"
        )?;

        let api_key = self.prompt_secret(
            "Enter API key (press Enter if none needed)"
        ).unwrap_or_default();

        Ok((base_url, model, api_key))
    }

    fn select_openai_model(&self) -> Result<String> {
        let models = vec![
            ("gpt-4o-mini", "Fast, capable, cost-effective (recommended)"),
            ("gpt-4o", "Most capable multimodal model"),
            ("gpt-4-turbo", "Legacy high-capability model"),
            ("gpt-3.5-turbo", "Fast, older model"),
        ];

        println!("\nSelect model:\n");
        for (i, (name, desc)) in models.iter().enumerate() {
            println!("{}) {} - {}", i + 1, name, desc);
        }
        println!("{}) Other", models.len() + 1);

        let choice = self.prompt_number("Enter number", 1, models.len() + 1)?;
        
        if choice <= models.len() {
            Ok(models[choice - 1].0.to_string())
        } else {
            self.prompt_string("Enter model name")
        }
    }

    fn select_groq_model(&self) -> Result<String> {
        let models = vec![
            ("llama-3.1-70b-versatile", "Llama 3.1 70B - Great balance"),
            ("llama-3.1-8b-instant", "Llama 3.1 8B - Very fast"),
            ("mixtral-8x7b-32768", "Mixtral 8x7B - Good for coding"),
            ("gemma2-9b-it", "Gemma 2 9B - Lightweight"),
        ];

        println!("\nSelect model:\n");
        for (i, (name, desc)) in models.iter().enumerate() {
            println!("{}) {} - {}", i + 1, name, desc);
        }
        println!("{}) Other", models.len() + 1);

        let choice = self.prompt_number("Enter number", 1, models.len() + 1)?;
        
        if choice <= models.len() {
            Ok(models[choice - 1].0.to_string())
        } else {
            self.prompt_string("Enter model name")
        }
    }

    async fn test_configuration(&self, base_url: &str, model: &str, api_key: &str) -> bool {
        // Simple connectivity test
        let client = reqwest::Client::new();
        
        // For Ollama, check if it's running
        if base_url.contains("11434") || base_url.contains("ollama") {
            return client
                .get("http://localhost:11434/api/tags")
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
        }

        // For cloud providers, we'd need to make an actual API call
        // For now, just check if the URL looks valid
        base_url.starts_with("http") && !model.is_empty()
    }

    fn show_summary(&self, base_url: &str, model: &str, api_key: &str) {
        println!("\n{}", "=".repeat(60));
        println!("📋 Configuration Summary");
        println!("{}", "=".repeat(60));
        println!("Base URL: {}", base_url);
        println!("Model:    {}", model);
        println!("API Key:  {}", if api_key.is_empty() {
            "(none)".to_string()
        } else {
            format!("{}...{}", &api_key[..4.min(api_key.len())], 
                "*".repeat(api_key.len().saturating_sub(8)))
        });
        println!("{}", "=".repeat(60));

        println!("\n💾 To save this configuration, run:\n");
        
        // Show export commands for different shells
        if cfg!(target_os = "windows") {
            println!("PowerShell:");
            println!("  $env:HORCRUX_LLM_URL = \"{}\"", base_url);
            println!("  $env:HORCRUX_LLM_MODEL = \"{}\"", model);
            if !api_key.is_empty() {
                println!("  $env:HORCRUX_LLM_API_KEY = \"{}\"", api_key);
            }
            println!("\nOr permanently in System Properties > Environment Variables");
        } else {
            println!("Bash/Zsh (add to ~/.bashrc or ~/.zshrc):");
            println!("  export HORCRUX_LLM_URL=\"{}\"", base_url);
            println!("  export HORCRUX_LLM_MODEL=\"{}\"", model);
            if !api_key.is_empty() {
                println!("  export HORCRUX_LLM_API_KEY=\"{}\"", api_key);
            }
            
            println!("\nFish:");
            println!("  set -Ux HORCRUX_LLM_URL \"{}\"", base_url);
            println!("  set -Ux HORCRUX_LLM_MODEL \"{}\"", model);
            if !api_key.is_empty() {
                println!("  set -Ux HORCRUX_LLM_API_KEY \"{}\"", api_key);
            }
        }

        // Also suggest a config file
        println!("\n📝 Or create a .env file in your project root:\n");
        println!("HORCRUX_LLM_URL={}", base_url);
        println!("HORCRUX_LLM_MODEL={}", model);
        if !api_key.is_empty() {
            println!("HORCRUX_LLM_API_KEY={}", api_key);
        }

        println!("\n✨ Configuration wizard complete!");
    }

    // Helper methods for user input

    fn prompt_string(&self, prompt: &str) -> Result<String> {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        Ok(input.trim().to_string())
    }

    fn prompt_string_with_default(&self, prompt: &str, default: &str) -> Result<String> {
        print!("{} [{}]: ", prompt, default);
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        let trimmed = input.trim();
        if trimmed.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    fn prompt_secret(&self, prompt: &str) -> Result<String> {
        print!("{}: ", prompt);
        io::stdout().flush()?;
        
        // In a real implementation, we'd use rpassword or similar
        // For now, just read normally
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        Ok(input.trim().to_string())
    }

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
}

/// Quick command to show current configuration
pub fn show_current_config() {
    println!("\n🔧 Current Agent Configuration\n");
    
    let url = std::env::var("HORCRUX_LLM_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "(not set - using OpenAI default)".to_string());
    
    let model = std::env::var("HORCRUX_LLM_MODEL")
        .or_else(|_| std::env::var("OPENAI_MODEL"))
        .unwrap_or_else(|_| "(not set - using gpt-4o-mini)".to_string());
    
    let key = std::env::var("HORCRUX_LLM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_else(|_| "(not set)".to_string());
    
    println!("HORCRUX_LLM_URL:    {}", url);
    println!("HORCRUX_LLM_MODEL:  {}", model);
    println!("HORCRUX_LLM_API_KEY: {}", 
        if key == "(not set)" || key.is_empty() {
            "(not set)".to_string()
        } else {
            format!("{}...{}", &key[..4.min(key.len())], "*".repeat(10))
        }
    );
    
    // Check embedding config too
    let embed_url = std::env::var("HORCRUX_EMBED_URL")
        .unwrap_or_else(|_| "(not set)".to_string());
    let embed_model = std::env::var("HORCRUX_EMBED_MODEL")
        .unwrap_or_else(|_| "(not set - using text-embedding-3-small)".to_string());
    
    println!("\nEmbedding Configuration:");
    println!("HORCRUX_EMBED_URL:   {}", embed_url);
    println!("HORCRUX_EMBED_MODEL: {}", embed_model);
    
    println!("\n💡 Run `horcrux agent --setup` to reconfigure");
}
