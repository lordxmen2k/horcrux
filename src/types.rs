use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult {
            path: "test/path.md".into(),
            docid: "abc123".into(),
            score: 0.95,
            snippet: "test content".into(),
            title: "Test Title".into(),
            context: Some("context".into()),
            line: Some(42),
        };

        assert_eq!(result.path, "test/path.md");
        assert_eq!(result.score, 0.95);
        assert!(result.context.is_some());
        assert!(result.line.is_some());
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            path: "test.md".into(),
            docid: "abc".into(),
            score: 0.9,
            snippet: "snippet".into(),
            title: "Title".into(),
            context: None,
            line: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test.md"));
        assert!(json.contains("0.9"));
        
        // Deserialize back
        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, result.path);
        assert_eq!(deserialized.score, result.score);
    }

    #[test]
    fn test_collection_creation() {
        let col = Collection {
            name: "notes".into(),
            path: "/home/user/notes".into(),
            pattern: "**/*.md".into(),
        };

        assert_eq!(col.name, "notes");
        assert_eq!(col.pattern, "**/*.md");
    }

    #[test]
    fn test_collection_serialization() {
        let col = Collection {
            name: "test".into(),
            path: "/tmp/test".into(),
            pattern: "*.txt".into(),
        };

        let json = serde_json::to_string(&col).unwrap();
        let deserialized: Collection = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.name, col.name);
        assert_eq!(deserialized.path, col.path);
    }

    #[test]
    fn test_document_creation() {
        let now = Utc::now();
        let doc = Document {
            docid: "abc123".into(),
            path: "test.md".into(),
            collection: "notes".into(),
            title: "Test".into(),
            body: "Body content".into(),
            hash: "sha256hash".into(),
            updated_at: now,
        };

        assert_eq!(doc.docid, "abc123");
        assert_eq!(doc.title, "Test");
    }

    #[test]
    fn test_chunk_creation() {
        let chunk = Chunk {
            docid: "abc123".into(),
            seq: 0,
            text: "chunk text".into(),
            pos: 100,
            embedding: Some(vec![0.1, 0.2, 0.3]),
        };

        assert_eq!(chunk.seq, 0);
        assert_eq!(chunk.pos, 100);
        assert!(chunk.embedding.is_some());
    }

    #[test]
    fn test_chunk_without_embedding() {
        let chunk = Chunk {
            docid: "abc123".into(),
            seq: 1,
            text: "chunk text".into(),
            pos: 200,
            embedding: None,
        };

        assert!(chunk.embedding.is_none());
    }

    #[test]
    fn test_path_context_creation() {
        let ctx = PathContext {
            collection: "notes".into(),
            path: "subdir".into(),
            context: "Meeting notes".into(),
        };

        assert_eq!(ctx.context, "Meeting notes");
    }
}

// ── QMD-compatible JSON output format ────────────────────────────────────────
// hoard speaks QMD protocol for compatibility with OpenClaw and other tools
// See: https://github.com/tobi/qmd for the original QMD spec

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResult {
    /// Collection-relative path, e.g. "memory/2026-03-27.md"
    pub path: String,
    /// 6-char hash docid, e.g. "#a1b2c3"
    pub docid: String,
    /// Relevance score 0.0–1.0
    pub score: f32,
    /// Text snippet around the match
    pub snippet: String,
    /// Document title (first heading or filename)
    pub title: String,
    /// Path context metadata (from `qmd context add`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Line number where the snippet starts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchOutput {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub backend: String,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentResult {
    pub path: String,
    pub docid: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub line_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusOutput {
    pub backend: String,
    pub db_path: String,
    pub collections: Vec<CollectionStatus>,
    pub total_documents: usize,
    pub total_chunks: usize,
    pub embedded_chunks: usize,
    pub index_size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionStatus {
    pub name: String,
    pub path: String,
    pub doc_count: usize,
    pub last_modified: Option<DateTime<Utc>>,
}

// ── Internal DB types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Document {
    pub docid: String,   // 6-char hex hash
    pub path: String,    // collection-relative path
    pub collection: String,
    pub title: String,
    pub body: String,
    pub hash: String,    // SHA-256 of body (for change detection)
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub docid: String,
    pub seq: usize,
    pub text: String,
    pub pos: usize,      // byte offset in original document
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub path: String,
    pub pattern: String, // glob, default "**/*.md"
}

#[derive(Debug, Clone)]
pub struct PathContext {
    pub collection: String,
    pub path: String,    // virtual path, e.g. "qmd://notes" or relative
    pub context: String,
}