# 🧙 Horcrux - AI Agent with Knowledge Memory

> Distributed intelligence for your tasks — part AI agent, part memory system

Horcrux combines a powerful ReAct-based AI agent with persistent knowledge storage, allowing you to build up institutional knowledge over time. Create skills on the fly, search your documents, and automate workflows.

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🤖 **Smart Agent** | ReAct-based reasoning with automatic skill creation |
| 🧠 **Knowledge Base** | Semantic search over your documents (BM25 + Vector) |
| 🛠️ **Skills System** | Built-in library + create your own on the fly |
| 💾 **Persistent Memory** | SQLite-backed conversation history |
| 🌐 **Multi-Platform** | Windows, Linux, macOS (x64 + ARM64) |
| ⚡ **Blazing Fast** | Rust-powered, sub-100ms queries |
| 🔒 **Privacy First** | Local-first, works offline with Ollama |
| 📱 **Telegram Bot** | Chat with your agent via Telegram |

## 🚀 Quick Start

### Installation

Download the latest binary for your platform from [Releases](https://github.com/yourusername/horcrux/releases):

```bash
# Windows
horcrux-windows-x64.exe

# Linux
chmod +x horcrux-linux-x64
./horcrux-linux-x64

# macOS (Intel)
chmod +x horcrux-macos-x64
./horcrux-macos-x64

# macOS (Apple Silicon)
chmod +x horcrux-macos-arm64
./horcrux-macos-arm64
```

### First Time Setup

Run the interactive setup wizard:

```bash
horcrux setup
```

This will guide you through:
1. **Choose deployment**: Local (Ollama) or Cloud API
2. **Select model**: We'll recommend the best for your use case
3. **Enter API key**: If using cloud providers
4. **Save configuration**: Automatically creates `.env` file

### Supported Models

**Local (via Ollama) - FREE:**
- Llama 3.1 8B ⭐ (Recommended - best balance)
- Qwen 2.5 14B (Powerful, needs 16GB RAM)
- Mistral 7B (Fast responses)
- Llama 3.2 3B (Lightweight, 4GB RAM)

**Cloud APIs:**
- Kimi (Moonshot) ⭐ (Best tool use, cheap)
- OpenAI GPT-4o (Industry standard)
- Anthropic Claude (Best reasoning)
- Groq (Ultra-fast inference)
- OpenRouter (100+ models)
- DeepSeek, Together AI, Fireworks, Cohere

## 📖 Usage

### Interactive Agent Mode

```bash
horcrux agent
```

Example conversation:
```
🤖 Horcrux Agent - Interactive Mode

💬 You: get me the top 5 hacker news stories

🤖 Agent: Here are the top 5 stories:
1. Artemis II crew take "spectacular" image of Earth
   https://www.bbc.com/news/articles/...
2. Show HN: Travel Hacking Toolkit
   https://github.com/borski/...
...

💬 You: save this as a skill

🤖 Agent: ✅ Created skill 'hackernews_top' for future use!

💬 You: use hackernews_top

🤖 Agent: (instantly returns results without thinking)
```

### One-shot Commands

```bash
horcrux agent "search my notes for project ideas"
```

### Knowledge Management

```bash
# Add documents to your knowledge base
horcrux collection add ~/Documents/notes

# Index everything
horcrux update

# Search (hybrid BM25 + semantic)
horcrux search "rust async patterns"
```

### Telegram Bot

```bash
# Set your bot token
export TELEGRAM_BOT_TOKEN="your-token"

# Run as Telegram bot
horcrux agent --telegram
```

## 🛠️ Built-in Skills

Horcrux comes with pre-installed skills:

| Skill | Description |
|-------|-------------|
| `hackernews_top` | Fetch top HN stories |
| `weather_check` | Current weather by city |
| `git_status` | Pretty git status |
| `system_info` | OS, memory, disk usage |
| `port_scan` | Check open ports |
| `crypto_price` | BTC, ETH prices |
| `password_gen` | Strong password generator |
| `file_backup` | Timestamped file backup |
| `dir_size` | Directory size analyzer |
| `json_format` | Pretty-print JSON |
| `timestamp_convert` | Unix timestamp → date |
| `base64_convert` | Encode/decode Base64 |
| `url_shorten` | Shorten URLs |
| `qr_generate` | Generate QR codes |

**Using skills:**
```
💬 You: use hackernews_top
💬 You: run skill weather_check with city: "London"
💬 You: create a new skill that...
```

## 🧠 Smart Skill Creation

The agent automatically suggests creating skills when:
- You perform multi-step API calls
- You run complex command sequences
- You mention doing something regularly

Skills are saved as reusable scripts and appear instantly in future sessions.

## ⚙️ Configuration

### Environment Variables

Create `.env` file or set directly:

```bash
# LLM Configuration
HORCRUX_LLM_URL=https://api.moonshot.ai/v1
HORCRUX_LLM_MODEL=moonshot-v1-8k
HORCRUX_LLM_API_KEY=sk-your-key

# Embedding (for semantic search)
HORCRUX_EMBED_URL=http://localhost:11434/v1
HORCRUX_EMBED_MODEL=nomic-embed-text

# Telegram
TELEGRAM_BOT_TOKEN=your-bot-token
```

### Multi-Agent Mode

Enable multiple specialized agents:

```bash
# In your .env
HORCRUX_MULTI_AGENT=true
HORCRUX_AGENTS=researcher,coder,writer
```

Agents can spawn sub-agents for complex tasks:
```
💬 You: research and write a blog post about Rust async

🤖 Research Agent: (gathers information)
🤖 Writer Agent: (drafts the post)
🤖 Coder Agent: (creates examples)
🤖 Main Agent: (combines everything)
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────┐
│              HORCRUX AGENT                  │
│  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
│  │  ReAct  │  │  Tools  │  │   Memory    │  │
│  │  Loop   │◄─┤ Registry├─►│  (SQLite)   │  │
│  └────┬────┘  └────┬────┘  └─────────────┘  │
│       │            │                        │
│       └────────────┘                        │
│              │                              │
│       ┌──────┴──────┐                       │
│       ▼             ▼                       │
│  ┌─────────┐  ┌─────────┐                   │
│  │  LLM   │  │ Skills  │                    │
│  │(Kimi/) │  │ Library │                    │
│  │(Ollama)│  │(Built-in│                    │     
│  └─────────┘  │+ User)  │                   │
│               └─────────┘                   │
└─────────────────────────────────────────────┘
```

## 🔧 Building from Source

```bash
# Clone
git clone https://github.com/yourusername/horcrux.git
cd horcrux

# Build
cargo build --release

# Binary will be at:
# target/release/horcrux.exe (Windows)
# target/release/horcrux    (Linux/Mac)
```

### Cross-compilation

```bash
# Build for all platforms
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

## 📊 Performance

| Metric | Horcrux | Python Agents |
|--------|---------|---------------|
| Startup | ~50ms | ~500ms-2s |
| Memory | ~50MB | ~200-500MB |
| Queries | ~10ms | ~50-100ms |
| Binary | ~15MB | ~100MB+ |

## 🤝 Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing`)
5. Open a Pull Request

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- Inspired by OpenClaw, Nous Hermes, and Claude Code
- Built with Rust, SQLite, and love
- Special thanks to the Ollama and Kimi teams

---

**Made with 🦀 in Rust** | [Issues](https://github.com/yourusername/horcrux/issues) | [Discussions](https://github.com/yourusername/horcrux/discussions)
