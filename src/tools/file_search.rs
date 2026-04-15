//! File Search Tool - Search inside documents (PDF, TXT, etc.)

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

pub struct FileSearchTool;

impl FileSearchTool {
    pub fn new() -> Self {
        Self
    }

    /// Search for text inside PDF files
    fn search_pdf(&self, pdf_path: &str, query: &str) -> anyhow::Result<bool> {
        let query_lower = query.to_lowercase();
        
        // Try pdfplumber first (better text extraction)
        if let Ok(found) = self.search_pdf_with_pdfplumber(pdf_path, &query_lower) {
            return Ok(found);
        }
        
        // Fallback to PyPDF2
        self.search_pdf_with_pypdf2(pdf_path, &query_lower)
    }

    fn search_pdf_with_pdfplumber(&self, pdf_path: &str, query_lower: &str) -> anyhow::Result<bool> {
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "import pdfplumber; import sys; \
                 pdf = pdfplumber.open('{}'); \
                 text = ''.join([page.extract_text() or '' for page in pdf.pages]); \
                 pdf.close(); \
                 sys.exit(0 if '{}' in text.lower() else 1)",
                pdf_path, query_lower
            ))
            .output()?;
        
        Ok(output.status.success())
    }

    fn search_pdf_with_pypdf2(&self, pdf_path: &str, query_lower: &str) -> anyhow::Result<bool> {
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "import PyPDF2; import sys; \
                 reader = PyPDF2.PdfReader('{}'); \
                 text = ''.join([page.extract_text() or '' for page in reader.pages]); \
                 sys.exit(0 if '{}' in text.lower() else 1)",
                pdf_path, query_lower
            ))
            .output()?;
        
        Ok(output.status.success())
    }

    /// Search for text in TXT files
    fn search_txt(&self, file_path: &str, query: &str) -> anyhow::Result<bool> {
        let content = std::fs::read_to_string(file_path)?;
        Ok(content.to_lowercase().contains(&query.to_lowercase()))
    }

    /// Search for text in Word documents (.docx)
    fn search_docx(&self, file_path: &str, query: &str) -> anyhow::Result<bool> {
        let query_lower = query.to_lowercase();
        
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "import docx2txt; import sys; \
                 text = docx2txt.process('{}'); \
                 sys.exit(0 if '{}' in text.lower() else 1)",
                file_path, query_lower
            ))
            .output()?;
        
        Ok(output.status.success())
    }

    /// Search for text using generic shell tools (for .doc, .rtf, etc.)
    fn search_with_shell(&self, file_path: &str, query: &str) -> anyhow::Result<bool> {
        let query_lower = query.to_lowercase();
        
        // Try strings command first (extracts text from binary files)
        if let Ok(output) = std::process::Command::new("strings")
            .arg(file_path)
            .output() 
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.to_lowercase().contains(&query_lower) {
                return Ok(true);
            }
        }
        
        // Fallback to cat and grep
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("cat '{}' | grep -i '{}'", file_path, query_lower))
            .output()?;
        
        Ok(output.status.success())
    }

    /// Search directory for files containing query (non-recursive for simplicity)
    async fn search_directory(&self, dir_path: &str, query: &str) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        if let Ok(entries) = tokio::fs::read_dir(dir_path).await {
            let mut entries = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let path_str = path.to_string_lossy().to_string();
                
                if path.is_file() {
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    
                    let found = match ext.as_str() {
                        "pdf" => self.search_pdf(&path_str, &query_lower).unwrap_or(false),
                        "txt" | "md" | "csv" | "json" | "xml" | "html" | "log" => {
                            self.search_txt(&path_str, &query_lower).unwrap_or(false)
                        }
                        "docx" => self.search_docx(&path_str, &query_lower).unwrap_or(false),
                        "doc" | "rtf" | "odt" | "pages" | "epub" => {
                            self.search_with_shell(&path_str, &query_lower).unwrap_or(false)
                        }
                        _ => {
                            self.search_with_shell(&path_str, &query_lower).unwrap_or(false)
                        }
                    };
                    
                    if found {
                        results.push((path_str, true));
                    }
                }
                // Note: Not recursing into subdirectories to avoid async recursion issues
            }
        }
        
        results
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Search for text inside files (PDF, TXT). \
         Use this when user asks for documents containing specific words/phrases. \
         Searches file CONTENTS, not filenames."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory or file path to search"
                },
                "query": {
                    "type": "string",
                    "description": "Text to search for inside files"
                }
            },
            "required": ["path", "query"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path_str = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        // Expand ~ to home directory
        let path_str = if path_str.starts_with("~/") {
            dirs::home_dir()
                .map(|home| home.join(&path_str[2..]).to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.to_string())
        } else {
            path_str.to_string()
        };

        let path = Path::new(&path_str);
        
        if !path.exists() {
            return Ok(ToolResult::error(format!("Path not found: {}", path.display())));
        }

        let results = if path.is_file() {
            // Single file search
            let path_str = path.to_string_lossy().to_string();
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            
            let found = match ext.as_str() {
                "pdf" => self.search_pdf(&path_str, query).unwrap_or(false),
                "txt" | "md" | "csv" | "json" | "xml" | "html" | "log" => {
                    self.search_txt(&path_str, query).unwrap_or(false)
                }
                "docx" => self.search_docx(&path_str, query).unwrap_or(false),
                _ => self.search_with_shell(&path_str, query).unwrap_or(false),
            };
            
            vec![(path_str, found)]
        } else {
            // Directory search
            self.search_directory(&path.to_string_lossy(), query).await
        };

        if results.is_empty() {
            Ok(ToolResult::success(format!(
                "No files found containing '{}' in {}. Note: PDFs may be scanned images that cannot be searched.",
                query, path.display()
            )))
        } else {
            let matching: Vec<String> = results
                .iter()
                .filter(|(_, found)| *found)
                .map(|(path, _)| path.clone())
                .collect();
            
            if matching.is_empty() {
                Ok(ToolResult::success(format!(
                    "Searched {} files. None contain '{}'",
                    results.len(), query
                )))
            } else {
                Ok(ToolResult::success(format!(
                    "Found {} file(s) containing '{}':\n{}",
                    matching.len(),
                    query,
                    matching.join("\n")
                )))
            }
        }
    }
}
