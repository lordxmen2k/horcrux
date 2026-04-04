use crate::db::Db;
use crate::embed::{EmbedClient, EmbedConfig};
use crate::search::run_search;
use crate::types::{SearchOutput, StatusOutput, CollectionStatus};
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{info, error};

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub tx: broadcast::Sender<ServerEvent>,
    pub embed_client: Option<Arc<EmbedClient>>,
    pub embed_model: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    IndexUpdated { collection: String, docs_added: usize },
    EmbeddingProgress { done: usize, total: usize },
    SearchQuery { query: String, results: usize },
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub min_score: f32,
}

#[derive(Deserialize)]
pub struct AddCollectionRequest {
    pub name: Option<String>,
    pub path: String,
    #[serde(default = "default_pattern")]
    pub pattern: String,
}

fn default_mode() -> String { "query".into() }
fn default_limit() -> usize { 10 }
fn default_pattern() -> String { "**/*.md".into() }

pub async fn run_server(db_path: PathBuf, host: String, port: u16) -> Result<()> {
    let (tx, _rx) = broadcast::channel(100);
    
    let config = EmbedConfig::from_env();
    let embed_client = Some(Arc::new(EmbedClient::new(config.clone())));
    let state = AppState { 
        db_path, 
        tx, 
        embed_client,
        embed_model: config.model,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/status", get(status))
        .route("/search", post(search))
        .route("/query", post(query))
        .route("/collections", get(list_collections).post(add_collection))
        .route("/update", post(trigger_update))
        .route("/embed", post(trigger_embed))
        .route("/sse", get(sse_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    
    info!("🌐 hoard server listening on http://{}", addr);
    println!("🌐 hoard server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn status(
    State(state): State<AppState>,
) -> Result<Json<StatusOutput>, StatusCode> {
    let db = Db::open(&state.db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let collections = db.list_collections()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|c| CollectionStatus {
            name: c.name,
            path: c.path,
            doc_count: 0, // Would need to query per collection
            last_modified: None,
        })
        .collect();

    let doc_count = db.document_count().unwrap_or(0);
    let chunk_count = db.chunk_count().unwrap_or(0);
    let embedded_count = db.embedded_chunk_count().unwrap_or(0);
    
    let db_size = std::fs::metadata(&state.db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(Json(StatusOutput {
        backend: "horcrux".into(),
        db_path: state.db_path.to_string_lossy().to_string(),
        collections,
        total_documents: doc_count,
        total_chunks: chunk_count,
        embedded_chunks: embedded_count,
        index_size_bytes: db_size,
    }))
}

async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchOutput>, StatusCode> {
    let db = Db::open(&state.db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let embed_client = if req.mode == "search" {
        None
    } else {
        state.embed_client.as_deref()
    };

    let results = run_search(
        &db,
        &req.query,
        &req.mode,
        req.limit,
        req.min_score,
        req.collection.as_deref(),
        embed_client,
        &state.embed_model,
    )
    .map_err(|e| {
        error!("Search error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Notify SSE listeners
    let _ = state.tx.send(ServerEvent::SearchQuery {
        query: req.query.clone(),
        results: results.len(),
    });

    Ok(Json(SearchOutput {
        results,
        query: req.query,
        backend: "horcrux".into(),
        total: 0,
    }))
}

async fn query(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchOutput>, StatusCode> {
    let mut req = req;
    req.mode = "query".into();
    search(State(state), Json(req)).await
}

async fn list_collections(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::types::Collection>>, StatusCode> {
    let db = Db::open(&state.db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let collections = db.list_collections().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(collections))
}

async fn add_collection(
    State(state): State<AppState>,
    Json(req): Json<AddCollectionRequest>,
) -> Result<Json<crate::types::Collection>, StatusCode> {
    let db = Db::open(&state.db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let name = req.name.unwrap_or_else(|| {
        std::path::Path::new(&req.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string()
    });

    let collection = crate::types::Collection {
        name: name.clone(),
        path: req.path.clone(),
        pattern: req.pattern,
    };

    db.add_collection(&collection).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let _ = state.tx.send(ServerEvent::IndexUpdated {
        collection: name,
        docs_added: 0,
    });
    
    Ok(Json(collection))
}

async fn trigger_update(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.tx.send(ServerEvent::IndexUpdated {
        collection: "all".into(),
        docs_added: 0,
    });
    (StatusCode::ACCEPTED, Json(serde_json::json!({ "status": "updating" })))
}

async fn trigger_embed(State(state): State<AppState>) -> impl IntoResponse {
    let _ = state.tx.send(ServerEvent::EmbeddingProgress { done: 0, total: 100 });
    (StatusCode::ACCEPTED, Json(serde_json::json!({ "status": "embedding" })))
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    
    let stream = async_stream::stream! {
        let mut rx = rx;
        while let Ok(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap();
            yield Ok(Event::default().data(json));
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
