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
cd /g/Python\ Projects/miniclaw_agent

# Build release binary
cargo build --release

# Binary location:
# Windows: target/release/horcrux.exe
# Linux/Mac: target/release/horcrux
```

### Test the Build

```bash
# Run the setup wizard
./target/release/horcrux.exe setup

# Or run agent directly
./target/release/horcrux.exe agent
```

---

## 🐙 Part 2: GitHub Setup

### Step 1: Initialize Git Repository

```bash
# Navigate to project directory
cd /g/Python\ Projects/miniclaw_agent

# Initialize git (if not already done)
git init

# Add all files
git add .

# First commit
git commit -m "Initial commit: Horcrux AI Agent"
```

### Step 2: Create GitHub Repository

1. Go to https://github.com/new
2. Repository name: `horcrux` (or your preferred name)
3. Description: "AI Agent with Knowledge Memory - built in Rust"
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

# Step 2: Create a version tag
git tag -a v0.1.0 -m "First release - Horcrux AI Agent"

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
   ## What's New
   - AI Agent with tool use
   - Knowledge base with semantic search
   - 15+ built-in skills
   - Telegram bot support
   - Cross-platform: Windows, Linux, macOS
   
   ## Downloads
   - Windows: `horcrux-windows-x64.exe`
   - Linux: `horcrux-linux-x64`
   - macOS Intel: `horcrux-macos-x64`
   - macOS ARM: `horcrux-macos-arm64`
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
./target/release/horcrux.exe agent

# Commit changes
git add .
git commit -m "Add feature: description here"

# Push to GitHub
git push origin main
```

### Creating a New Release

```bash
# After committing all changes

# Update version tag
git tag -a v0.2.0 -m "Add multi-agent support and new skills"

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
2. Make sure you're pushing a tag (not just committing)
3. Go to **Actions** tab on GitHub to see build status

---

## ✅ Pre-Release Checklist

Before creating a release:

- [ ] Code compiles without errors
- [ ] Tests pass (`cargo test`)
- [ ] README.md is up to date
- [ ] CHANGELOG.md added (optional)
- [ ] Version number updated in `Cargo.toml`
- [ ] All changes committed
- [ ] GitHub Actions workflow file present

---

## 🎯 Next Steps

After your first release:

1. **Share the release** - Post on social media, forums
2. **Get feedback** - Create GitHub Issues for bugs/features
3. **Iterate** - Make improvements based on feedback
4. **Build community** - Add CONTRIBUTING.md, Code of Conduct

---

**Questions?** Open an issue on GitHub or check the main README.md

**Happy shipping! 🚀**
