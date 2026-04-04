use crate::chunk::extract_snippet;
use crate::db::{bytes_to_floats, Db};
use crate::embed::{cosine_similarity, EmbedClient};
use crate::types::SearchResult;
use anyhow::Result;
use std::collections::HashMap;

/// Escape special characters for FTS5 MATCH queries.
/// Wraps the term in double quotes if it contains special characters.
fn escape_fts5(term: &str) -> String {
    // FTS5 special characters that need escaping: " * ( ) - (at word boundary)
    // The hyphen - is treated as NOT operator in FTS5, so we need to quote it
    if term.chars().any(|c| matches!(c, '"' | '*' | '(' | ')')) || term.contains('-') {
        // Escape internal double quotes by doubling them
        let escaped = term.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        term.to_string()
    }
}

pub struct SearchEngine<'a> {
    db: &'a Db,
}

impl<'a> SearchEngine<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// BM25 full-text search (fast, no embeddings needed)
    pub fn search_bm25(&self, query: &str, limit: usize, collection: Option<&str>) -> Result<Vec<SearchResult>> {
        // Escape special FTS5 characters in each term
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| escape_fts5(t))
            .collect();

        // Build FTS5 match expression — phrase search with fallback to OR
        let fts_query = if terms.len() > 1 {
            let escaped_query = escape_fts5(query);
            format!("\"{}\" OR {}", escaped_query, terms.join(" OR "))
        } else if terms.len() == 1 {
            terms[0].clone()
        } else {
            String::new()
        };

        let sql = if collection.is_some() {
            "SELECT d.docid, d.path, d.collection, d.title, d.body,
                    bm25(documents_fts) AS score
             FROM documents_fts
             JOIN documents d ON d.rowid = documents_fts.rowid
             WHERE documents_fts MATCH ?1
               AND d.collection = ?2
             ORDER BY score
             LIMIT ?3"
        } else {
            "SELECT d.docid, d.path, d.collection, d.title, d.body,
                    bm25(documents_fts) AS score
             FROM documents_fts
             JOIN documents d ON d.rowid = documents_fts.rowid
             WHERE documents_fts MATCH ?1
             ORDER BY score
             LIMIT ?2"
        };

        // Convert to str refs for snippet extraction
        let term_refs: Vec<&str> = terms.iter().map(|s| s.as_str()).collect();
        
        let results = if let Some(col) = collection {
            self.query_bm25_with_collection(sql, &fts_query, col, limit, &term_refs)?
        } else {
            self.query_bm25(sql, &fts_query, limit, &term_refs)?
        };

        Ok(results)
    }

    fn query_bm25(&self, sql: &str, fts_query: &str, limit: usize, terms: &[&str]) -> Result<Vec<SearchResult>> {
        let mut stmt = self.db.conn.prepare(sql)?;
        let rows = stmt.query_map(
            rusqlite::params![fts_query, limit as i64],
            |row| self.row_to_result(row),
        )?;
        self.collect_with_snippets(rows, terms)
    }

    fn query_bm25_with_collection(
        &self, sql: &str, fts_query: &str, collection: &str, limit: usize, terms: &[&str]
    ) -> Result<Vec<SearchResult>> {
        let mut stmt = self.db.conn.prepare(sql)?;
        let rows = stmt.query_map(
            rusqlite::params![fts_query, collection, limit as i64],
            |row| self.row_to_result(row),
        )?;
        self.collect_with_snippets(rows, terms)
    }

    fn row_to_result(&self, row: &rusqlite::Row) -> rusqlite::Result<(String, String, String, String, String, f64)> {
        Ok((
            row.get(0)?, // docid
            row.get(1)?, // path
            row.get(2)?, // collection
            row.get(3)?, // title
            row.get(4)?, // body
            row.get(5)?, // bm25 score (negative — lower is better in SQLite FTS5)
        ))
    }

    fn collect_with_snippets(
        &self,
        rows: impl Iterator<Item = rusqlite::Result<(String, String, String, String, String, f64)>>,
        terms: &[&str],
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        for row in rows {
            let (docid, path, collection, title, body, bm25_score) = row?;
            // Convert BM25 score (negative, lower=better) to 0..1 range
            let score = 1.0 / (1.0 + bm25_score.abs() as f32);
            let context = self.db.get_context_for_document(&path, &collection).ok().flatten();
            results.push(SearchResult {
                docid,
                path,
                title,
                score,
                snippet: extract_snippet(&body, terms, 700),
                context,
                line: None,
            });
        }
        Ok(results)
    }

    /// Vector semantic search - memory efficient with streaming
    pub fn search_vector(
        &self,
        query_embedding: &[f32],
        limit: usize,
        collection: Option<&str>,
        model: &str,
    ) -> Result<Vec<SearchResult>> {
        // Stream embeddings and keep only top N candidates
        // This avoids loading all embeddings into memory at once
        
        // Get total count for early exit
        let total_embedded = self.db.embedded_chunk_count()?;
        if total_embedded == 0 {
            return Ok(vec![]);
        }

        // Max candidates to consider (limit × multiplier for collection filtering)
        let candidate_limit = limit * 3;

        // Track top candidates: (score, docid)
        let mut candidates: Vec<(f32, String)> = Vec::with_capacity(candidate_limit + 1);
        let mut seen_docs: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Stream embeddings one at a time
        let mut stmt = self.db.conn.prepare(
            "SELECT docid, embedding FROM chunks
             WHERE embed_model = ?1 AND embedding IS NOT NULL
             ORDER BY docid"
        )?;
        let mut rows = stmt.query([model])?;

        while let Some(row) = rows.next()? {
            let docid: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            
            // Skip empty embeddings (corrupted data)
            if bytes.is_empty() {
                continue;
            }
            
            let emb = bytes_to_floats(&bytes);
            let sim = cosine_similarity(query_embedding, &emb);

            // Deduplicate by docid - skip if already seen
            if !seen_docs.insert(docid.clone()) {
                continue;
            }

            // Maintain top N candidates using partial sort approach
            if candidates.len() < candidate_limit {
                candidates.push((sim, docid));
            } else {
                // Find and replace the lowest score if this one is better
                if let Some((min_idx, _)) = candidates
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                {
                    if sim > candidates[min_idx].0 {
                        candidates[min_idx] = (sim, docid);
                    }
                }
            }
        }

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        // Sort by score descending
        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Fetch documents and apply collection filter
        let mut results = Vec::new();
        for (score, docid) in candidates {
            if results.len() >= limit { break; }

            let doc = match self.db.get_document(&docid)? {
                Some(d) => d,
                None => continue,
            };

            if let Some(col) = collection {
                if doc.collection != col { continue; }
            }

            let context = self.db.get_context_for_document(&doc.path, &doc.collection).ok().flatten();
            results.push(SearchResult {
                docid,
                path: doc.path,
                title: doc.title,
                score,
                snippet: doc.body.chars().take(700).collect::<String>() + "…",
                context,
                line: None,
            });
        }

        Ok(results)
    }

    /// Hybrid search: BM25 + vector merged via Reciprocal Rank Fusion
    pub fn search_hybrid(
        &self,
        query: &str,
        query_embedding: &[f32],
        limit: usize,
        collection: Option<&str>,
        model: &str,
    ) -> Result<Vec<SearchResult>> {
        let candidate_limit = limit * 4; // cast a wide net before merging

        let bm25_results = self.search_bm25(query, candidate_limit, collection)?;
        let vec_results = self.search_vector(query_embedding, candidate_limit, collection, model)?;

        let k = 60.0_f32;
        let mut scores: HashMap<String, f32> = HashMap::new();
        let mut result_by_docid: HashMap<String, SearchResult> = HashMap::new();

        for (rank, r) in bm25_results.iter().enumerate() {
            *scores.entry(r.docid.clone()).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
        for (rank, r) in vec_results.iter().enumerate() {
            *scores.entry(r.docid.clone()).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }

        // Build lookup then immediately drop the source Vecs
        for r in bm25_results.into_iter().chain(vec_results.into_iter()) {
            result_by_docid.entry(r.docid.clone()).or_insert(r);
        }
        // ↑ bm25_results and vec_results are now dropped (moved into chain)

        // Sort by RRF score and take top N
        let mut fused: Vec<(String, f32)> = scores.into_iter().collect();
        fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let results = fused
            .into_iter()
            .take(limit)
            .filter_map(|(docid, rrf_score)| {
                result_by_docid.remove(&docid).map(|mut r| {
                    r.score = rrf_score;
                    r
                })
            })
            .collect();

        Ok(results)
    }
}

/// Run the appropriate search mode and return QMD-compatible JSON output.
pub fn run_search(
    db: &Db,
    query: &str,
    mode: &str, // "search" | "vsearch" | "query"
    limit: usize,
    min_score: f32,
    collection: Option<&str>,
    embed_client: Option<&EmbedClient>,
    model: &str,
) -> Result<Vec<SearchResult>> {
    let engine = SearchEngine::new(db);

    let mut results = match mode {
        "vsearch" => {
            let client = embed_client.ok_or_else(|| anyhow::anyhow!(
                "Vector search requires embedding config (set HOARD_EMBED_URL)"
            ))?;
            let query_emb = client.embed_one(query)?;
            engine.search_vector(&query_emb, limit, collection, model)?
        }
        "query" => {
            // Hybrid: fall back to BM25-only if no embeddings configured
            if let Some(client) = embed_client {
                let query_emb = client.embed_one(query)?;
                engine.search_hybrid(query, &query_emb, limit, collection, model)?
            } else {
                engine.search_bm25(query, limit, collection)?
            }
        }
        _ => {
            // "search" — BM25 only
            engine.search_bm25(query, limit, collection)?
        }
    };

    results.retain(|r| r.score >= min_score);
    Ok(results)
}