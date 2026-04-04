# Publishing Guide

This guide covers how to publish this crate to [crates.io](https://crates.io/).

## Step 1: Choose a Name

The current name `memscape` is already taken on crates.io. Here are **available alternatives**:

| Name | Available | Notes |
|------|-----------|-------|
| ✅ **memsie** | Yes | Cute, memorable, "memory" + "sie" |
| ✅ **memzy** | Yes | Short, catchy, modern |
| ✅ **memzee** | Yes | Fun, "memory" + "zee" |
| ✅ **memscape** | Yes | "Memory" + "landscape", evocative |
| ✅ **memsee** | Yes | "Memory" + "see", descriptive |
| ✅ **memsii** | Yes | Unique spelling |

**Recommended:** `memsie` or `memzy` - short, memorable, easy to type.

## Step 2: Update Project Name

Once you've chosen a name (e.g., `memsie`), update:

### 1. Cargo.toml

```toml
[package]
name = "memsie"  # <-- Change this
version = "0.1.0"
edition = "2021"
description = "Fast local semantic memory for LLMs"
license = "MIT"
repository = "https://github.com/YOUR_USERNAME/memsie"
keywords = ["llm", "memory", "semantic-search", "embeddings", "rag"]
categories = ["command-line-utilities", "text-processing"]
```

### 2. Binary Name (optional)

If you want the CLI command to differ from the crate name:

```toml
[[bin]]
name = "memsie"      # The command users will type
crate-type = ["bin"]
```

### 3. Code References

Search and replace `memscape` with your new name in:
- `src/main.rs` (in comments/strings if any)
- `src/cli/mod.rs` (help text)
- `src/cli/status.rs` (status messages)
- `README.md`
- `SETUP.md`

### 4. Environment Variables (Optional)

Consider updating env vars for consistency:

```rust
// Old
CLAW_EMBED_URL → memscape_EMBED_URL → MEMSIE_EMBED_URL (optional)

// Current backward-compatible approach is fine
// Just update docs to mention the new preferred name
```

## Step 3: Prepare for Publishing

### 1. Create a crates.io Account

1. Go to [crates.io](https://crates.io/)
2. Sign in with GitHub
3. Go to Account Settings → API Tokens
4. Create a new token: `cargo publish`
5. Copy the token

### 2. Login with Cargo

```bash
cargo login
# Paste your API token when prompted
```

### 3. Verify Package

```bash
# Dry run - checks everything without publishing
cargo publish --dry-run

# Check for warnings
cargo clippy -- -D warnings

# Run tests
cargo test

# Build in release mode
cargo build --release
```

### 4. Add Required Metadata

Ensure `Cargo.toml` has these fields:

```toml
[package]
name = "memsie"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
edition = "2021"
description = "Fast local semantic memory for LLMs"
license = "MIT"  # or "Apache-2.0", etc.
repository = "https://github.com/YOUR_USERNAME/memsie"
homepage = "https://github.com/YOUR_USERNAME/memsie"
documentation = "https://docs.rs/memsie"
readme = "README.md"
keywords = ["llm", "memory", "semantic-search", "embeddings", "rag"]
categories = ["command-line-utilities", "text-processing"]
exclude = ["/*.sh", "/.github/*", "/docs/*"]
```

### 5. Add LICENSE File

Create a `LICENSE` file in the project root:

**MIT License (example):**
```
MIT License

Copyright (c) 2026 [Your Name]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

[... rest of MIT license ...]
```

## Step 4: Publish

### First Release

```bash
# 1. Commit all changes
git add .
git commit -m "Prepare for v0.1.0 release"

# 2. Tag the release
git tag -a v0.1.0 -m "First release"
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

## Step 5: Verify

1. Check crates.io: `https://crates.io/crates/memsie`
2. Check docs.rs: `https://docs.rs/memsie`
3. Test installation:
   ```bash
   cargo install memsie
   memsie --version
   ```

## Post-Publishing Checklist

- [ ] Update README with installation instructions
- [ ] Add badges to README:
  ```markdown
  [![Crates.io](https://img.shields.io/crates/v/memsie)](https://crates.io/crates/memsie)
  [![Docs.rs](https://docs.rs/memsie/badge.svg)](https://docs.rs/memsie)
  ```
- [ ] Announce on social media / forums
- [ ] Submit to [awesome-rust](https://github.com/rust-unofficial/awesome-rust)
- [ ] Consider publishing GitHub Releases with binaries

## Troubleshooting

### "crate with this name already exists"

The name is taken. Try another from the list above.

### "failed to verify package tarball"

```bash
# Check what's being included
cargo package --list

# Add exclusions to Cargo.toml
[package]
exclude = ["target/*", "*.log", "tests/fixtures/*"]
```

### "documentation tests failed"

```bash
# Test docs locally
cargo test --doc
```

### "missing license"

Add a `LICENSE` file to the project root.

## yanking a Release (Emergency)

If you need to remove a version:

```bash
cargo yank --version 0.1.0
```

To undo:
```bash
cargo yank --undo --version 0.1.0
```

## Alternative: Binary Releases

For users who don't have Rust installed, provide pre-built binaries:

1. Use [cargo-dist](https://github.com/axodotdev/cargo-dist):
   ```bash
   cargo install cargo-dist
   cargo dist init
   cargo dist generate
   ```

2. Or GitHub Actions to build releases

## Resources

- [The Cargo Book - Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [crates.io policies](https://crates.io/policies)
- [SemVer](https://semver.org/) for versioning
