//! Integration tests for memscape

use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;

// Helper to create a temporary database path
fn temp_db_path() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    (temp_dir, db_path)
}

// Note: These tests require the memscape library to be built with test support
// They test the core functionality end-to-end

#[test]
fn test_temp_dir_cleanup() {
    // Simple test to verify our temp directory helper works
    let (temp_dir, db_path) = temp_db_path();
    assert!(temp_dir.path().exists());
    assert!(db_path.to_str().unwrap().contains("test.db"));
    // temp_dir is automatically cleaned up when dropped
}

#[cfg(feature = "integration-tests")]
mod db_tests {
    use super::*;
    use memscape::db::Db;
    use memscape::types::{Collection, Document, Chunk};
    use chrono::Utc;

    #[test]
    fn test_database_creation() {
        let (_temp, db_path) = temp_db_path();
        
        let db = Db::open(&db_path).expect("Failed to open database");
        
        // Verify database file was created
        assert!(db_path.exists());
    }

    #[test]
    fn test_collection_crud() {
        let (_temp, db_path) = temp_db_path();
        let db = Db::open(&db_path).unwrap();
        
        // Create
        let collection = Collection {
            name: "test".into(),
            path: "/tmp/test".into(),
            pattern: "**/*.md".into(),
        };
        
        db.add_collection(&collection).expect("Failed to add collection");
        
        // Read
        let collections = db.list_collections().expect("Failed to list collections");
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "test");
        
        // Get specific
        let found = db.get_collection("test").expect("Failed to get collection");
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "/tmp/test");
        
        // Delete
        db.remove_collection("test").expect("Failed to remove collection");
        let collections = db.list_collections().unwrap();
        assert_eq!(collections.len(), 0);
    }

    #[test]
    fn test_document_crud() {
        let (_temp, db_path) = temp_db_path();
        let db = Db::open(&db_path).unwrap();
        
        // Add a collection first
        let collection = Collection {
            name: "docs".into(),
            path: "/tmp/docs".into(),
            pattern: "*.md".into(),
        };
        db.add_collection(&collection).unwrap();
        
        // Create document
        let doc = Document {
            docid: "abc123".into(),
            path: "test.md".into(),
            collection: "docs".into(),
            title: "Test Document".into(),
            body: "This is a test document.".into(),
            hash: "sha256hash".into(),
            updated_at: Utc::now(),
        };
        
        // Insert
        let changed = db.upsert_document(&doc).expect("Failed to upsert document");
        assert!(changed, "New document should be marked as changed");
        
        // Read by docid
        let found = db.get_document("abc123").expect("Failed to get document");
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Test Document");
        
        // Read by path
        let found = db.find_document_by_path("test.md").expect("Failed to find document");
        assert!(found.is_some());
        
        // Count
        let count = db.document_count().expect("Failed to count documents");
        assert_eq!(count, 1);
        
        // Update (same hash - should not change)
        let changed = db.upsert_document(&doc).expect("Failed to upsert document");
        assert!(!changed, "Same document should not be marked as changed");
    }

    #[test]
    fn test_chunk_operations() {
        let (_temp, db_path) = temp_db_path();
        let db = Db::open(&db_path).unwrap();
        
        // Add document first
        let collection = Collection {
            name: "docs".into(),
            path: "/tmp/docs".into(),
            pattern: "*.md".into(),
        };
        db.add_collection(&collection).unwrap();
        
        let doc = Document {
            docid: "doc1".into(),
            path: "test.md".into(),
            collection: "docs".into(),
            title: "Test".into(),
            body: "Content".into(),
            hash: "hash".into(),
            updated_at: Utc::now(),
        };
        db.upsert_document(&doc).unwrap();
        
        // Insert chunks
        let chunks = vec![
            Chunk {
                docid: "doc1".into(),
                seq: 0,
                text: "First chunk".into(),
                pos: 0,
                embedding: None,
            },
            Chunk {
                docid: "doc1".into(),
                seq: 1,
                text: "Second chunk".into(),
                pos: 100,
                embedding: None,
            },
        ];
        
        db.insert_chunks(&chunks).expect("Failed to insert chunks");
        
        // Count chunks
        let count = db.chunk_count().expect("Failed to count chunks");
        assert_eq!(count, 2);
        
        let embedded = db.embedded_chunk_count().expect("Failed to count embedded chunks");
        assert_eq!(embedded, 0);
    }
}

#[cfg(feature = "integration-tests")]
mod search_tests {
    use super::*;
    use memscape::db::Db;
    use memscape::search::run_search;
    use memscape::types::{Collection, Document};
    use chrono::Utc;

    #[test]
    fn test_bm25_search() {
        let (_temp, db_path) = temp_db_path();
        let db = Db::open(&db_path).unwrap();
        
        // Setup collection and document
        let collection = Collection {
            name: "docs".into(),
            path: "/tmp/docs".into(),
            pattern: "*.md".into(),
        };
        db.add_collection(&collection).unwrap();
        
        let doc = Document {
            docid: "doc1".into(),
            path: "rust.md".into(),
            collection: "docs".into(),
            title: "Rust Programming".into(),
            body: "Rust is a systems programming language with memory safety.".into(),
            hash: "hash1".into(),
            updated_at: Utc::now(),
        };
        db.upsert_document(&doc).unwrap();
        
        // Search
        let results = run_search(&db, "Rust programming", "search", 10, 0.0, None, None, "model").unwrap();
        
        assert!(!results.is_empty(), "Should find results for 'Rust programming'");
        assert_eq!(results[0].title, "Rust Programming");
    }

    #[test]
    fn test_search_no_results() {
        let (_temp, db_path) = temp_db_path();
        let db = Db::open(&db_path).unwrap();
        
        let collection = Collection {
            name: "docs".into(),
            path: "/tmp/docs".into(),
            pattern: "*.md".into(),
        };
        db.add_collection(&collection).unwrap();
        
        let doc = Document {
            docid: "doc1".into(),
            path: "test.md".into(),
            collection: "docs".into(),
            title: "Test".into(),
            body: "Content".into(),
            hash: "hash".into(),
            updated_at: Utc::now(),
        };
        db.upsert_document(&doc).unwrap();
        
        // Search for something that doesn't exist
        let results = run_search(&db, "nonexistent xyz123", "search", 10, 0.0, None, None, "model").unwrap();
        
        assert!(results.is_empty(), "Should return empty for non-matching query");
    }
}

#[cfg(feature = "integration-tests")]
mod chunk_tests {
    use super::*;
    use memscape::chunk::{chunk_markdown, extract_title, extract_snippet};

    #[test]
    fn test_chunking_integration() {
        let markdown = r#"# Title

This is paragraph one with some content.

## Section 2

This is paragraph two with more content.

### Subsection

Final paragraph here.
"#;

        let chunks = chunk_markdown(markdown);
        
        // Should have at least one chunk
        assert!(!chunks.is_empty());
        
        // First chunk should contain the title
        assert!(chunks[0].text.contains("Title"));
    }

    #[test]
    fn test_title_extraction_integration() {
        let md1 = "# Main Title\n\nContent";
        let md2 = "## Subtitle\n\nContent";
        let md3 = "Just content without heading";
        
        assert_eq!(extract_title(md1, "file.md"), "Main Title");
        assert_eq!(extract_title(md2, "file.md"), "Subtitle");
        assert_eq!(extract_title(md3, "/path/2024-01-01.md"), "2024-01-01");
    }

    #[test]
    fn test_snippet_extraction_integration() {
        let text = "The quick brown fox jumps over the lazy dog. "
            .repeat(20);
        
        let terms = &["fox", "dog"];
        let snippet = extract_snippet(&text, terms, 100);
        
        assert!(snippet.to_lowercase().contains("fox"));
        assert!(snippet.len() <= 150);
    }
}

// Main test entry point - runs when integration-tests feature is enabled
#[cfg(not(feature = "integration-tests"))]
#[test]
fn test_placeholder() {
    // Placeholder test that always passes when integration tests are disabled
    // This ensures the test file is valid even without the feature flag
    println!("Integration tests require --features integration-tests flag");
}
