//! horcrux - AI Agent with Knowledge Memory
//! 
//! Distributed intelligence for your tasks — part AI agent, part memory system.
//! Horcrux combines a powerful ReAct-based agent with persistent knowledge storage,
//! allowing you to build up institutional knowledge over time.
//!
//! Features:
//! - ReAct-based AI agent with tool use
//! - Persistent conversation memory
//! - Semantic search over your documents
//! - Dynamic skill creation
//! - Cross-platform, local-first architecture

pub mod agent;
pub mod cache;
pub mod chunk;
pub mod config;
pub mod db;
pub mod embed;
pub mod gateway;
pub mod integrations;
pub mod search;
pub mod skills;
pub mod tools;
pub mod types;

// Re-export commonly used types
pub use types::{Collection, Document, Chunk, SearchResult};
pub use db::Db;
pub use embed::{EmbedClient, EmbedConfig, cosine_similarity};
pub use cache::SearchCache;
pub use agent::{Agent, AgentConfig, LlmClient, LlmConfig};
pub use gateway::{Gateway, parse_agent_response, sanitize_agent_output};
