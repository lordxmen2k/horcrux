# Testing Guide for memscape

This document describes the test suite for memscape and how to run it.

## Test Structure

The test suite is organized as follows:

### Unit Tests (in `src/`)

| Module | Test Coverage |
|--------|--------------|
| `src/chunk.rs` | Chunking logic, title extraction, snippet extraction, break point scoring |
| `src/embed.rs` | Text hashing, cosine similarity, embed config |
| `src/cache.rs` | Cache operations, eviction, invalidation, thread safety |
| `src/types.rs` | Type creation, serialization/deserialization |

### Integration Tests (in `tests/`)

| File | Coverage |
|------|----------|
| `tests/integration_tests.rs` | Database CRUD, search functionality, end-to-end workflows |

## Running Tests

### Run All Tests

```bash
cargo test
```

### Run Only Unit Tests

```bash
cargo test --lib
```

### Run Only Integration Tests

```bash
cargo test --test integration_tests
```

### Run with Integration Test Features

```bash
cargo test --features integration-tests
```

### Run with Output

```bash
cargo test -- --nocapture
```

### Run Specific Test

```bash
cargo test test_name
```

### Run Tests for Specific Module

```bash
cargo test chunk::tests
cargo test embed::tests
cargo test cache::tests
```

## Test Coverage Summary

### chunk.rs Tests
- ✅ `test_short_text_no_chunking` - Short texts stay as one chunk
- ✅ `test_title_extraction` - Extract titles from markdown headings
- ✅ `test_chunk_markdown_long_text` - Long texts are split properly
- ✅ `test_chunk_respects_headings` - Split at heading boundaries
- ✅ `test_extract_snippet` - Snippet extraction with query terms
- ✅ `test_break_score_headings` - Heading break point scoring
- ✅ `test_break_score_code_fence` - Code fence detection
- ✅ `test_break_score_horizontal_rules` - HR line detection
- ✅ `test_break_score_empty` - Empty line handling
- ✅ `test_break_score_list_items` - List item detection
- ✅ `test_snap_char_boundary` - UTF-8 character boundary handling

### embed.rs Tests
- ✅ `test_text_hash_deterministic` - Hash consistency
- ✅ `test_text_hash_different_inputs` - Hash uniqueness
- ✅ `test_cosine_similarity_identical` - Similarity of identical vectors
- ✅ `test_cosine_similarity_opposite` - Similarity of opposite vectors
- ✅ `test_cosine_similarity_orthogonal` - Similarity of orthogonal vectors
- ✅ `test_cosine_similarity_empty` - Empty vector handling
- ✅ `test_cosine_similarity_different_lengths` - Length mismatch handling
- ✅ `test_cosine_similarity_zero_vector` - Zero vector handling
- ✅ `test_cosine_similarity_typical` - Typical use case
- ✅ `test_embed_config_defaults` - Default config values
- ✅ `test_embed_config_from_env` - Environment variable reading

### cache.rs Tests
- ✅ `test_cache_key_generation` - Cache key formatting
- ✅ `test_cache_basic_operations` - Get/set operations
- ✅ `test_cache_update_existing` - Overwriting entries
- ✅ `test_cache_eviction` - LRU eviction when full
- ✅ `test_cache_invalidation` - Clear all entries
- ✅ `test_cache_stats` - Hit/miss statistics
- ✅ `test_cache_thread_safety` - Concurrent access

### types.rs Tests
- ✅ `test_search_result_creation` - SearchResult construction
- ✅ `test_search_result_serialization` - JSON serialization
- ✅ `test_collection_creation` - Collection construction
- ✅ `test_collection_serialization` - JSON serialization
- ✅ `test_document_creation` - Document construction
- ✅ `test_chunk_creation` - Chunk with/without embeddings
- ✅ `test_path_context_creation` - PathContext construction

### Integration Tests
- ✅ `test_database_creation` - Database file creation
- ✅ `test_collection_crud` - Collection create/read/update/delete
- ✅ `test_document_crud` - Document operations
- ✅ `test_chunk_operations` - Chunk insertion and counting
- ✅ `test_bm25_search` - Full-text search
- ✅ `test_search_no_results` - Empty result handling
- ✅ `test_chunking_integration` - End-to-end chunking
- ✅ `test_title_extraction_integration` - Title extraction
- ✅ `test_snippet_extraction_integration` - Snippet generation

## Continuous Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        override: true
    
    - name: Run unit tests
      run: cargo test --lib
    
    - name: Run integration tests
      run: cargo test --features integration-tests
```

## Adding New Tests

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        // Arrange
        let input = "test";
        
        // Act
        let result = my_function(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Test Example

```rust
#[test]
fn test_feature_integration() {
    let (_temp, db_path) = temp_db_path();
    let db = Db::open(&db_path).unwrap();
    
    // Test your feature end-to-end
    let result = db.my_feature().unwrap();
    assert!(result.is_ok());
}
```

## Known Issues

### Windows File Locking

On Windows, you may encounter file locking errors during compilation:
```
error: failed to remove ...: The process cannot access the file
```

**Solution:** Wait a few seconds and try again, or close any processes that might be holding the files open.

### SQLite Concurrency

SQLite WAL mode is enabled, which allows concurrent reads but the tests currently run serially. If you add parallel test execution, ensure proper connection handling.

## Test Data

Tests use temporary directories created with the `tempfile` crate. These are automatically cleaned up after each test.

```rust
fn temp_db_path() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    (temp_dir, db_path)
}
```

## Code Coverage

To generate code coverage reports:

```bash
# Install cargo-tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

## Benchmarks

To run benchmarks (if added):

```bash
cargo bench
```
