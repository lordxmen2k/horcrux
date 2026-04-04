use crate::db::Db;
use crate::search::run_search;
use crate::embed::{EmbedClient, EmbedConfig};
use crate::types::SearchResult;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

#[derive(PartialEq)]
pub enum Focus {
    Search,
    Results,
    Preview,
}

pub struct App {
    // Input state
    pub input: String,
    pub input_mode: InputMode,
    pub focus: Focus,
    pub show_preview: bool,
    
    // Search state
    pub search_query: String,
    pub last_search: Option<Instant>,
    pub results: Vec<SearchResult>,
    pub selected_index: usize,
    pub is_searching: bool,
    
    // Preview state
    pub preview_content: Option<String>,
    pub preview_scroll: usize,
    
    // App state
    pub db_path: PathBuf,
    pub collection: Option<String>,
    pub message: Option<(String, Instant)>,
    pub show_help: bool,
    pub should_quit: bool,
    
    // Embedding
    embed_client: Option<EmbedClient>,
    embed_model: String,
    
    // Database (opened once)
    db: Db,
}

impl App {
    pub fn new(db_path: PathBuf, collection: Option<String>) -> Self {
        let config = EmbedConfig::from_env();
        let embed_client = if std::env::var("HORCRUX_EMBED_URL").is_ok()
            || std::env::var("HOARD_EMBED_URL").is_ok() // backward compat
            || std::env::var("OLLAMA_HOST").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
        {
            Some(EmbedClient::new(config.clone()))
        } else {
            None
        };

        let db = Db::open(&db_path).expect("Failed to open DB");
        
        Self {
            input: String::new(),
            input_mode: InputMode::Editing,
            focus: Focus::Search,
            show_preview: true,
            search_query: String::new(),
            last_search: None,
            results: Vec::new(),
            selected_index: 0,
            is_searching: false,
            preview_content: None,
            preview_scroll: 0,
            db_path,
            collection,
            message: None,
            show_help: false,
            should_quit: false,
            embed_client,
            embed_model: config.model,
            db,
        }
    }

    pub fn run_search(&mut self) -> Result<()> {
        if self.input.is_empty() {
            self.results.clear();
            return Ok(());
        }

        self.is_searching = true;
        self.search_query = self.input.clone();
        
        let results = run_search(
            &self.db,
            &self.search_query,
            "query", // hybrid mode
            20,      // limit
            0.0,     // min_score
            self.collection.as_deref(),
            self.embed_client.as_ref(),
            &self.embed_model,
        )?;

        self.results = results;
        self.selected_index = 0;
        self.is_searching = false;
        self.last_search = Some(Instant::now());

        // Load preview for first result
        self.load_preview()?;

        Ok(())
    }

    pub fn load_preview(&mut self) -> Result<()> {
        if let Some(result) = self.results.get(self.selected_index) {
            if let Some(doc) = self.db.get_document(&result.docid)? {
                // Show full document content with match highlighted
                let mut content = format!("# {}\n\n", doc.title);
                content.push_str(&doc.body);
                self.preview_content = Some(content);
                self.preview_scroll = 0;
            } else {
                self.preview_content = None;
            }
        } else {
            self.preview_content = None;
        }
        Ok(())
    }

    pub fn next_result(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.results.len() - 1);
            let _ = self.load_preview();
        }
    }

    pub fn previous_result(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            let _ = self.load_preview();
        }
    }

    pub fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(5);
    }

    pub fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(5);
    }

    pub fn open_selected(&self) -> Result<()> {
        if let Some(result) = self.results.get(self.selected_index) {
            if let Some(doc) = self.db.get_document(&result.docid)? {
                if let Ok(col) = self.db.get_collection(&doc.collection) {
                    if let Some(c) = col {
                        let full_path = std::path::Path::new(&c.path).join(&doc.path);
                        self.open_file(&full_path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn open_file(&self, path: &std::path::Path) -> Result<()> {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
            if cfg!(windows) { "notepad".into() } else { "vim".into() }
        });

        std::process::Command::new(&editor)
            .arg(path)
            .spawn()?
            .wait()?;

        Ok(())
    }

    pub fn clear_message(&mut self) {
        if let Some((_, when)) = self.message {
            if when.elapsed().as_secs() > 3 {
                self.message = None;
            }
        }
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some((msg, Instant::now()));
    }

    pub fn on_tick(&mut self) {
        self.clear_message();
        
        // Auto-search after typing stops (300ms debounce)
        if let Some(last) = self.last_search {
            if self.search_query != self.input && last.elapsed().as_millis() > 300 {
                self.last_search = None; // ← reset so it doesn't re-fire every tick
                if let Err(e) = self.run_search() {
                    self.set_message(format!("Search error: {}", e));
                }
            }
        } else if !self.input.is_empty() && self.results.is_empty() && !self.is_searching {
            if let Err(e) = self.run_search() {
                self.set_message(format!("Search error: {}", e));
            }
        }
    }

    pub fn toggle_preview(&mut self) {
        self.show_preview = !self.show_preview;
        self.focus = if self.show_preview && self.focus == Focus::Results {
            Focus::Preview
        } else {
            Focus::Results
        };
    }
}
