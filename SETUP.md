# Setup Guide - From Zero to Release

This guide walks you through building Horcrux, setting up GitHub, and creating cross-platform releases.

---

## 📦 Part 1: Build the Project

### Prerequisites

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### Build Locally

```bash
# Navigate to project
cd horcrux

# Build release binary (15MB, single file, no dependencies)
cargo build --release

# Binary location:
# Windows: target/release/horcrux.exe
# Linux/Mac: target/release/horcrux
```

### Test the Build

```bash
# Run the comprehensive setup wizard
./target/release/horcrux setup

# Or run agent directly
./target/release/horcrux agent

# Test knowledge search
./target/release/horcrux search "test query"
```

---

## 🐙 Part 2: GitHub Setup

### Step 1: Initialize Git Repository

```bash
# Navigate to project directory
cd horcrux

# Initialize git (if not already done)
git init

# Create .gitignore (IMPORTANT!)
cat > .gitignore << 'EOF'
# Rust build artifacts
/target/
**/*.rs.bk
Cargo.lock

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# Environment variables (contains secrets!)
.env
.env.local
.env.*.local

# Database files
*.db
*.db-shm
*.db-wal
*.sqlite
*.sqlite3

# Compiled binaries
*.exe
*.dll
*.so
*.dylib
horcrux

# Logs
*.log
logs/

# OS files
.DS_Store
Thumbs.db
EOF

# Verify .env is ignored (CRITICAL - contains secrets!)
git check-ignore -v .env
# Should output: .gitignore:14:.env .env (or similar)

# Add all files
git add .

# Check that .env is not staged
git status
# Should NOT show .env in "Changes to be committed"

# First commit
git commit -m "Initial commit: Horcrux AI Agent with Knowledge Memory"
```

### Step 2: Create GitHub Repository

1. Go to https://github.com/new
2. Repository name: `horcrux` (or your preferred name)
3. Description: "AI Agent with Knowledge Memory - Multi-platform bots, skills, and semantic search - built in Rust"
4. Choose: **Public** or **Private**
5. **DO NOT** initialize with README (we already have one)
6. Click **Create repository**

### Step 3: Push to GitHub

```bash
# Add remote (replace YOUR_USERNAME with your GitHub username)
git remote add origin https://github.com/YOUR_USERNAME/horcrux.git

# Push code
git branch -M main
git push -u origin main
```

### Step 4: Verify Push

```bash
# Check status
git status

# View remote
git remote -v
```

---

## 🚀 Part 3: Create a Release

The GitHub Actions workflow (`.github/workflows/release.yml`) automatically builds for all platforms when you create a tag.

### Method 1: Using Git Commands (Recommended)

```bash
# Step 1: Make sure all changes are committed
git status
git add .
git commit -m "Ready for release v0.1.0"

# Step 2: Create a version tag (single line)
git tag -a v0.1.0 -m "First release - Horcrux AI Agent"

# OR use a file for multi-line message:
cat > /tmp/tagmsg.txt << 'EOF'
First release - Horcrux AI Agent

Features:
- ReAct-based AI agent with tool use
- Knowledge base with BM25 + semantic search
- 15+ built-in skills with auto-creation
- Multi-platform messaging bots
- REST API and Web UI
- Cross-platform: Windows, Linux, macOS
EOF
git tag -a v0.1.0 -F /tmp/tagmsg.txt

# Step 3: Push the tag (this triggers the build)
git push origin v0.1.0

# Done! GitHub Actions will now build for:
# - Windows (x64)
# - Linux (x64)
# - macOS Intel (x64)
# - macOS Apple Silicon (ARM64)
```

### Method 2: Using GitHub Web Interface

1. Go to your repository on GitHub
2. Click **Releases** → **Create a new release**
3. Click **Choose a tag** → Type `v0.1.0` → Click **Create new tag**
4. Release title: `v0.1.0 - Initial Release`
5. Description:
   ```markdown
   ## 🧙 Horcrux v0.1.0

   AI Agent with Knowledge Memory

   ### Features
   - 🤖 ReAct-based agent with automatic skill creation
   - 🧠 Semantic search over documents (BM25 + Vector)
   - 🛠️ 15+ built-in skills
   - 📱 Multi-platform bots: Telegram, Discord, Slack, WhatsApp, Matrix
   - 🌐 REST API + Web UI
   - 🔌 MCP Server for Claude Desktop
   - 🔄 Scheduled tasks support
   - 👥 Multi-agent mode
   - ⚡ 15MB single binary, no dependencies
   - 🌐 Cross-platform: Windows, Linux, macOS (x64 + ARM64)

   ### Downloads
   | Platform | Binary |
   |----------|--------|
   | Windows x64 | `horcrux-windows-x64.exe` |
   | Linux x64 | `horcrux-linux-x64` |
   | macOS Intel | `horcrux-macos-x64` |
   | macOS ARM64 | `horcrux-macos-arm64` |

   ### Quick Start
   ```bash
   # Download, then run setup
   ./horcrux setup

   # Start the agent
   ./horcrux agent
   ```
   ```
6. Click **Publish release**

7. The GitHub Actions workflow will automatically:
   - Build for all 4 platforms
   - Upload binaries to the release
   - Takes ~5-10 minutes

### Step 4: Download Your Binaries

1. Go to **Releases** page on GitHub
2. Click on your release (e.g., `v0.1.0`)
3. Download binaries from **Assets** section:
   - `horcrux-windows-x64.exe`
   - `horcrux-linux-x64`
   - `horcrux-macos-x64`
   - `horcrux-macos-arm64`

---

## 🔄 Part 4: Making Updates

### Regular Development Workflow

```bash
# Make changes to code
# ... edit files ...

# Test locally
cargo build --release
./target/release/horcrux agent

# Run tests
cargo test

# Commit changes
git add .
git commit -m "Add feature: description here"

# Push to GitHub
git push origin main
```

### Creating a New Release

```bash
# Update version in Cargo.toml
# version = "0.2.0"

# After committing all changes
git add .
git commit -m "Bump version to 0.2.0"

# Create new tag
git tag -a v0.2.0 -m "Version 0.2.0 - Add feature X"

# Push tag (triggers new builds)
git push origin v0.2.0
```

---

## 📋 Quick Reference

### Common Git Commands

```bash
# Check status
git status

# View commit history
git log --oneline

# Pull latest changes
git pull origin main

# Create and switch to new branch
git checkout -b feature/my-feature

# Switch back to main
git checkout main

# Merge branch
git merge feature/my-feature

# Delete branch
git branch -d feature/my-feature
```

### Version Numbering

Use [Semantic Versioning](https://semver.org/):
- `v0.1.0` - Initial release
- `v0.1.1` - Bug fixes
- `v0.2.0` - New features (backward compatible)
- `v1.0.0` - Stable release

---

## 🛠️ Troubleshooting

### Build Fails

```bash
# Clean and rebuild
cargo clean
cargo build --release

# Update dependencies
cargo update
cargo build --release

# Windows: Kill running process if file locked
taskkill /F /IM horcrux.exe 2>nul
cargo build --release
```

### Git Push Rejected

```bash
# Pull latest changes first
git pull origin main

# Then push
git push origin main
```

### Release Not Triggering

1. Check that `.github/workflows/release.yml` exists
2. Make sure you're pushing a tag (not just committing):
   ```bash
   git tag -a v0.1.0 -m "Release message"
   git push origin v0.1.0
   ```
3. Go to **Actions** tab on GitHub to see build status
4. Check that Actions are enabled in repository settings

---

## ✅ Pre-Release Checklist

Before creating a release:

- [ ] Code compiles without errors (`cargo build --release`)
- [ ] Tests pass (`cargo test`)
- [ ] README.md is up to date
- [ ] All documentation reviewed (SETUP.md, TESTING.md, etc.)
- [ ] `.gitignore` includes `.env`, `target/`, `*.db` (CRITICAL!)
- [ ] Version number updated in `Cargo.toml`
- [ ] All changes committed
- [ ] GitHub Actions workflow file present
- [ ] `.env` file NOT committed (verify with `git status`)

---

## 🎯 Next Steps

After your first release:

1. **Share the release** - Post on social media, Reddit r/rust, Hacker News
2. **Get feedback** - Create GitHub Issues for bugs/features
3. **Iterate** - Make improvements based on feedback
4. **Build community** - Add CONTRIBUTING.md, Code of Conduct
5. **Consider crates.io** - Publish as Rust crate (see PUBLISH.md)

---

**Questions?** Open an issue on GitHub or check the main README.md

**Happy shipping! 🚀**
