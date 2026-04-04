# Publishing Guide for Horcrux

This guide covers how to publish Horcrux to [crates.io](https://crates.io/) as a Rust crate.

## Overview

Horcrux is an AI agent framework with:
- ReAct-based reasoning with tool use
- Knowledge memory with semantic search
- Multi-platform messaging bots
- REST API and Web UI
- 15+ built-in skills

## Step 1: Check Crate Name Availability

```bash
# Check if 'horcrux' is available on crates.io
cargo search horcrux
```

If taken, consider alternatives:
- `horcrux-agent`
- `horcrux-ai`
- `horcrux-mind`

## Step 2: Prepare Cargo.toml

Ensure your `Cargo.toml` has all required metadata:

```toml
[package]
name = "horcrux"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
edition = "2021"
description = "AI Agent with Knowledge Memory - Multi-platform bots, skills, and semantic search"
license = "MIT"
repository = "https://github.com/YOUR_USERNAME/horcrux"
homepage = "https://github.com/YOUR_USERNAME/horcrux"
documentation = "https://docs.rs/horcrux"
readme = "README.md"
keywords = ["ai", "agent", "llm", "memory", "semantic-search", "telegram", "discord", "bot"]
categories = ["command-line-utilities", "text-processing", "web-programming"]
exclude = [
    "/target/*",
    "/*.sh",
    "/.github/*",
    "/docs/*",
    "*.db",
    "*.db-*",
    ".env",
]

[[bin]]
name = "horcrux"
path = "src/main.rs"

[dependencies]
# ... your dependencies ...
```

## Step 3: Create Required Files

### LICENSE File

Create `LICENSE` in project root (MIT example):

```
MIT License

Copyright (c) 2026 [Your Name]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Ensure README.md is Ready

Your README should include:
- Clear description
- Installation instructions (`cargo install horcrux`)
- Usage examples
- Feature list
- Configuration guide

## Step 4: Create crates.io Account

1. Go to [crates.io](https://crates.io/)
2. Sign in with GitHub
3. Go to Account Settings → API Tokens
4. Create a new token named `cargo publish`
5. Copy the token

## Step 5: Login with Cargo

```bash
cargo login
# Paste your API token when prompted
```

## Step 6: Verify Package

```bash
# Dry run - checks everything without publishing
cargo publish --dry-run

# Check for warnings
cargo clippy -- -D warnings

# Run tests
cargo test

# Build in release mode
cargo build --release

# Check what's being included in the package
cargo package --list
```

### Common Issues

**Binary files too large:**
```bash
# Make sure to exclude in Cargo.toml:
exclude = ["target/*", "*.db", ".env"]
```

**Missing documentation:**
```bash
# Add doc comments to public APIs
# Test docs locally
cargo doc --no-deps
cargo test --doc
```

## Step 7: Publish

### First Release

```bash
# 1. Ensure everything is committed
git status
git add .
git commit -m "Prepare for v0.1.0 crates.io release"

# 2. Create git tag
git tag -a v0.1.0 -m "First crates.io release"
git push origin v0.1.0

# 3. Publish to crates.io
cargo publish
```

### Subsequent Releases

1. Update version in `Cargo.toml`:
   ```toml
   version = "0.1.1"
   ```

2. Update `CHANGELOG.md` (if you have one)

3. Commit and tag:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "Bump version to 0.1.1"
   git tag -a v0.1.1 -m "Version 0.1.1"
   git push origin v0.1.1
   ```

4. Publish:
   ```bash
   cargo publish
   ```

## Step 8: Verify Publication

1. Check crates.io: `https://crates.io/crates/horcrux`
2. Check docs.rs: `https://docs.rs/horcrux`
3. Test installation:
   ```bash
   cargo install horcrux
   horcrux --version
   ```

## Post-Publishing Checklist

- [ ] Add badges to README:
  ```markdown
  [![Crates.io](https://img.shields.io/crates/v/horcrux)](https://crates.io/crates/horcrux)
  [![Docs.rs](https://docs.rs/horcrux/badge.svg)](https://docs.rs/horcrux)
  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  ```
- [ ] Update README with `cargo install` instructions
- [ ] Create GitHub Release with binaries (see SETUP.md)
- [ ] Announce on social media / forums
- [ ] Submit to [awesome-rust](https://github.com/rust-unofficial/awesome-rust)
- [ ] Add to [lib.rs](https://lib.rs/)

## Alternative Distribution Methods

### GitHub Releases (Recommended for End Users)

Most users prefer pre-built binaries. Use GitHub Actions to build releases:

```bash
# See .github/workflows/release.yml
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
# Binaries built automatically
```

### cargo-binstall

Users can install without compiling:
```bash
cargo install cargo-binstall
cargo binstall horcrux
```

### Homebrew (macOS/Linux)

Create a Homebrew formula for easy installation.

## Troubleshooting

### "crate with this name already exists"

The name is taken. Options:
1. Choose a different name (e.g., `horcrux-agent`)
2. Contact the owner of the existing crate
3. Use a different registry

### "failed to verify package tarball"

```bash
# Check what's being included
cargo package --list

# Add exclusions to Cargo.toml
[package]
exclude = ["target/*", "*.log", "tests/fixtures/*", "*.db", ".env"]
```

### "documentation tests failed"

```bash
# Test docs locally
cargo test --doc

# Fix doc examples to be complete and runnable
```

### "missing license"

Add a `LICENSE` or `LICENSE-MIT` file to the project root.

## Yanking a Release (Emergency)

If you need to remove a version:

```bash
cargo yank --version 0.1.0
```

To undo:
```bash
cargo yank --undo --version 0.1.0
```

## Resources

- [The Cargo Book - Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [crates.io policies](https://crates.io/policies)
- [SemVer](https://semver.org/) for versioning
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
