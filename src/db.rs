use crate::types::{Chunk, Collection, Document, PathContext};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite at {}", path.display()))?;

        // Performance tuning - safe for our single-writer use case
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -32000;  -- 32MB page cache
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 67108864; -- 64MB instead of 256MB",
        )?;

        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch("
            CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);

            CREATE TABLE IF NOT EXISTS collections (
                name        TEXT PRIMARY KEY,
                path        TEXT NOT NULL,
                pattern     TEXT NOT NULL DEFAULT '**/*.md',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS documents (
                docid       TEXT PRIMARY KEY,   -- 6-char hex
                path        TEXT NOT NULL,       -- collection-relative
                collection  TEXT NOT NULL,
                title       TEXT NOT NULL,
                body        TEXT NOT NULL,
                hash        TEXT NOT NULL,       -- SHA-256 for change detection
                updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (collection) REFERENCES collections(name) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
            CREATE INDEX IF NOT EXISTS idx_documents_path ON documents(path);

            -- FTS5 for BM25 keyword search
            CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
                docid UNINDEXED,
                title,
                body,
                content='documents',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS documents_ai AFTER INSERT ON documents BEGIN
                INSERT INTO documents_fts(rowid, docid, title, body)
                VALUES (new.rowid, new.docid, new.title, new.body);
            END;

            CREATE TRIGGER IF NOT EXISTS documents_au AFTER UPDATE ON documents BEGIN
                INSERT INTO documents_fts(documents_fts, rowid, docid, title, body)
                VALUES ('delete', old.rowid, old.docid, old.title, old.body);
                INSERT INTO documents_fts(rowid, docid, title, body)
                VALUES (new.rowid, new.docid, new.title, new.body);
            END;

            CREATE TRIGGER IF NOT EXISTS documents_ad AFTER DELETE ON documents BEGIN
                INSERT INTO documents_fts(documents_fts, rowid, docid, title, body)
                VALUES ('delete', old.rowid, old.docid, old.title, old.body);
            END;

            -- Embedding chunks
            CREATE TABLE IF NOT EXISTS chunks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                docid       TEXT NOT NULL,
                seq         INTEGER NOT NULL,
                text        TEXT NOT NULL,
                pos         INTEGER NOT NULL DEFAULT 0,
                embedding   BLOB,               -- NULL until embed runs
                embed_model TEXT,               -- model name used
                updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(docid, seq),
                FOREIGN KEY (docid) REFERENCES documents(docid) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_docid ON chunks(docid);
            CREATE INDEX IF NOT EXISTS idx_chunks_no_embed ON chunks(docid) WHERE embedding IS NULL;

            -- Path contexts (metadata for search quality)
            CREATE TABLE IF NOT EXISTS path_contexts (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                collection  TEXT,
                path        TEXT NOT NULL,
                context     TEXT NOT NULL,
                UNIQUE(collection, path)
            );

            -- In-memory cache for embedding results (persisted across restarts)
            CREATE TABLE IF NOT EXISTS embed_cache (
                text_hash   TEXT PRIMARY KEY,
                model       TEXT NOT NULL,
                embedding   BLOB NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Agent conversation history
            CREATE TABLE IF NOT EXISTS conversations (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                role        TEXT NOT NULL,  -- system, user, assistant, tool
                content     TEXT NOT NULL,
                tool_calls  TEXT,           -- JSON array of tool calls (for assistant)
                tool_name   TEXT,           -- Tool name (for tool role)
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_conversations_session ON conversations(session_id, created_at);

            -- FTS5 for conversation search (session_search tool)
            CREATE VIRTUAL TABLE IF NOT EXISTS conversations_fts USING fts5(
                session_id UNINDEXED,
                role UNINDEXED,
                content,
                content='conversations',
                content_rowid='rowid'
            );

            -- Triggers to keep conversations FTS in sync
            CREATE TRIGGER IF NOT EXISTS conversations_ai AFTER INSERT ON conversations BEGIN
                INSERT INTO conversations_fts(rowid, session_id, role, content)
                VALUES (new.rowid, new.session_id, new.role, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS conversations_au AFTER UPDATE ON conversations BEGIN
                INSERT INTO conversations_fts(conversations_fts, rowid, session_id, role, content)
                VALUES ('delete', old.rowid, old.session_id, old.role, old.content);
                INSERT INTO conversations_fts(rowid, session_id, role, content)
                VALUES (new.rowid, new.session_id, new.role, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS conversations_ad AFTER DELETE ON conversations BEGIN
                INSERT INTO conversations_fts(conversations_fts, rowid, session_id, role, content)
                VALUES ('delete', old.rowid, old.session_id, old.role, old.content);
            END;
        ")?;

        Ok(())
    }

    // ── Collections ──────────────────────────────────────────────────────────

    pub fn add_collection(&self, col: &Collection) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO collections (name, path, pattern) VALUES (?1, ?2, ?3)",
            params![col.name, col.path, col.pattern],
        )?;
        Ok(())
    }

    pub fn list_collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, path, pattern FROM collections ORDER BY name"
        )?;
        let cols = stmt.query_map([], |row| {
            Ok(Collection {
                name: row.get(0)?,
                path: row.get(1)?,
                pattern: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cols)
    }

    pub fn remove_collection(&self, name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM collections WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn get_collection(&self, name: &str) -> Result<Option<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, path, pattern FROM collections WHERE name = ?1"
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(Collection {
                name: row.get(0)?,
                path: row.get(1)?,
                pattern: row.get(2)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    // ── Documents ────────────────────────────────────────────────────────────

    pub fn upsert_document(&self, doc: &Document) -> Result<bool> {
        // Returns true if document was changed (new or updated)
        let existing_hash: Option<String> = self.conn.query_row(
            "SELECT hash FROM documents WHERE docid = ?1",
            params![doc.docid],
            |row| row.get(0),
        ).ok();

        if existing_hash.as_deref() == Some(&doc.hash) {
            return Ok(false); // unchanged
        }

        self.conn.execute(
            "INSERT INTO documents (docid, path, collection, title, body, hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(docid) DO UPDATE SET
               path = excluded.path,
               title = excluded.title,
               body = excluded.body,
               hash = excluded.hash,
               updated_at = excluded.updated_at",
            params![
                doc.docid, doc.path, doc.collection,
                doc.title, doc.body, doc.hash,
                Utc::now().to_rfc3339()
            ],
        )?;

        // Delete old chunks so they get re-embedded
        self.conn.execute("DELETE FROM chunks WHERE docid = ?1", params![doc.docid])?;

        Ok(true)
    }

    pub fn remove_missing_documents(&self, collection: &str, present_docids: &[String]) -> Result<usize> {
        if present_docids.is_empty() {
            let n = self.conn.execute(
                "DELETE FROM documents WHERE collection = ?1",
                params![collection],
            )?;
            return Ok(n);
        }

        // SQLite doesn't support arrays directly; use a temp approach
        let placeholders: String = present_docids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "DELETE FROM documents WHERE collection = ?1 AND docid NOT IN ({})",
            placeholders
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(collection.to_string())];
        for id in present_docids {
            bound.push(Box::new(id.clone()));
        }

        let params: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
        let n = stmt.execute(params.as_slice())?;
        Ok(n)
    }

    pub fn get_document(&self, docid: &str) -> Result<Option<Document>> {
        let mut stmt = self.conn.prepare(
            "SELECT docid, path, collection, title, body, hash, updated_at
             FROM documents WHERE docid = ?1"
        )?;
        let mut rows = stmt.query_map(params![docid], |row| {
            Ok(Document {
                docid: row.get(0)?,
                path: row.get(1)?,
                collection: row.get(2)?,
                title: row.get(3)?,
                body: row.get(4)?,
                hash: row.get(5)?,
                updated_at: row.get::<_, String>(6)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn find_document_by_path(&self, path: &str) -> Result<Option<Document>> {
        let mut stmt = self.conn.prepare(
            "SELECT docid, path, collection, title, body, hash, updated_at
             FROM documents WHERE path = ?1 LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![path], |row| {
            Ok(Document {
                docid: row.get(0)?,
                path: row.get(1)?,
                collection: row.get(2)?,
                title: row.get(3)?,
                body: row.get(4)?,
                hash: row.get(5)?,
                updated_at: row.get::<_, String>(6)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn document_count(&self) -> Result<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))?)
    }

    // ── Chunks / Embeddings ──────────────────────────────────────────────────

    pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO chunks (docid, seq, text, pos) VALUES (?1, ?2, ?3, ?4)"
        )?;
        for chunk in chunks {
            stmt.execute(params![chunk.docid, chunk.seq, chunk.text, chunk.pos])?;
        }
        Ok(())
    }

    pub fn chunks_needing_embedding(&self, model: &str, limit: usize) -> Result<Vec<(i64, String, String)>> {
        // Returns (id, docid, text) for chunks without an embedding for this model
        let mut stmt = self.conn.prepare(
            "SELECT id, docid, text FROM chunks
             WHERE embedding IS NULL OR embed_model != ?1
             LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![model, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn save_embedding(&self, chunk_id: i64, model: &str, embedding: &[f32]) -> Result<()> {
        let bytes = floats_to_bytes(embedding);
        self.conn.execute(
            "UPDATE chunks SET embedding = ?1, embed_model = ?2, updated_at = datetime('now')
             WHERE id = ?3",
            params![bytes, model, chunk_id],
        )?;
        Ok(())
    }

    pub fn all_embeddings(&self, model: &str) -> Result<Vec<(String, Vec<f32>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT docid, embedding FROM chunks
             WHERE embed_model = ?1 AND embedding IS NOT NULL
             ORDER BY docid"
        )?;
        let mut results = Vec::new();
        let mut rows = stmt.query(params![model])?;
        while let Some(row) = rows.next()? {
            let docid: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            results.push((docid, bytes_to_floats(&bytes)));
            // bytes is dropped here, not held alongside the float vec
        }
        Ok(results)
    }

    pub fn chunk_count(&self) -> Result<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?)
    }

    pub fn embedded_chunk_count(&self) -> Result<usize> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE embedding IS NOT NULL", [], |r| r.get(0)
        )?)
    }

    // ── Embedding cache ──────────────────────────────────────────────────────

    pub fn get_cached_embedding(&self, text_hash: &str, model: &str) -> Result<Option<Vec<f32>>> {
        let result: Option<Vec<u8>> = self.conn.query_row(
            "SELECT embedding FROM embed_cache WHERE text_hash = ?1 AND model = ?2",
            params![text_hash, model],
            |row| row.get(0),
        ).ok();
        Ok(result.map(|b| bytes_to_floats(&b)))
    }

    pub fn set_cached_embedding(&self, text_hash: &str, model: &str, embedding: &[f32]) -> Result<()> {
        let bytes = floats_to_bytes(embedding);
        self.conn.execute(
            "INSERT OR REPLACE INTO embed_cache (text_hash, model, embedding) VALUES (?1, ?2, ?3)",
            params![text_hash, model, bytes],
        )?;
        Ok(())
    }

    // ── Path contexts ────────────────────────────────────────────────────────

    pub fn add_context(&self, ctx: &PathContext) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO path_contexts (collection, path, context) VALUES (?1, ?2, ?3)",
            params![ctx.collection, ctx.path, ctx.context],
        )?;
        Ok(())
    }

    pub fn get_context_for_document(&self, path: &str, collection: &str) -> Result<Option<String>> {
        // Try most specific match first (exact path), then prefix matches
        let ctx: Option<String> = self.conn.query_row(
            "SELECT context FROM path_contexts
             WHERE (collection = ?1 OR collection IS NULL)
             AND (?2 LIKE path || '%' OR path = ?2)
             ORDER BY LENGTH(path) DESC
             LIMIT 1",
            params![collection, path],
            |row| row.get(0),
        ).ok();
        Ok(ctx)
    }

    pub fn list_contexts(&self) -> Result<Vec<PathContext>> {
        let mut stmt = self.conn.prepare(
            "SELECT collection, path, context FROM path_contexts ORDER BY collection, path"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PathContext {
                collection: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                path: row.get(1)?,
                context: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn remove_context(&self, collection: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM path_contexts WHERE collection = ?1 AND path = ?2",
            params![collection, path],
        )?;
        Ok(())
    }

    // ── Conversation History ───────────────────────────────────────────────────

    pub fn add_conversation_message(&self, session_id: &str, role: &str, content: &str, tool_calls: Option<&str>, tool_name: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO conversations (session_id, role, content, tool_calls, tool_name, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![session_id, role, content, tool_calls, tool_name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_conversation_history(&self, session_id: &str, limit: usize) -> Result<Vec<ConversationMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, tool_calls, tool_name, created_at 
             FROM conversations 
             WHERE session_id = ?1 
             ORDER BY created_at ASC 
             LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(ConversationMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls: row.get(4)?,
                tool_name: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<ConversationSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, MIN(created_at) as started_at, MAX(created_at) as last_active,
                    COUNT(*) as message_count
             FROM conversations 
             GROUP BY session_id 
             ORDER BY last_active DESC 
             LIMIT ?1"
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(ConversationSession {
                session_id: row.get(0)?,
                started_at: row.get(1)?,
                last_active: row.get(2)?,
                message_count: row.get::<_, i64>(3)? as usize,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn clear_session(&self, session_id: &str) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM conversations WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(n)
    }

    pub fn delete_old_conversations(&self, days: i64) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM conversations WHERE created_at < datetime('now', ?1)",
            params![format!("-{} days", days)],
        )?;
        Ok(n)
    }

    // ── Session Search (FTS5) ──────────────────────────────────────────────────

    pub fn search_conversations(&self, query: &str, session_id: Option<&str>, limit: usize) -> Result<Vec<ConversationSearchResult>> {
        let sql = if let Some(sid) = session_id {
            // Search within specific session
            "SELECT c.id, c.session_id, c.role, c.content, c.created_at, 
                    rank
             FROM conversations_fts fts
             JOIN conversations c ON c.rowid = fts.rowid
             WHERE conversations_fts MATCH ?1 AND fts.session_id = ?2
             ORDER BY rank
             LIMIT ?3"
                .to_string()
        } else {
            // Search across all sessions
            "SELECT c.id, c.session_id, c.role, c.content, c.created_at, 
                    rank
             FROM conversations_fts fts
             JOIN conversations c ON c.rowid = fts.rowid
             WHERE conversations_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&sql)?;
        
        let rows = if let Some(sid) = session_id {
            stmt.query_map(params![query, sid, limit as i64], |row| {
                Ok(ConversationSearchResult {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                    rank: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![query, limit as i64], |row| {
                Ok(ConversationSearchResult {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                    rank: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        
        Ok(rows)
    }
}

// ── Conversation Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub tool_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ConversationSession {
    pub session_id: String,
    pub started_at: String,
    pub last_active: String,
    pub message_count: usize,
}

#[derive(Debug, Clone)]
pub struct ConversationSearchResult {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub rank: f64,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}