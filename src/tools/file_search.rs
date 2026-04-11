//! File Search Tool - Cross-platform file finder with filtering
//!
//! Supports: name patterns, extension filters, date/size filters, content search
//! Cross-platform: Works on Windows, macOS, and Linux

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct FileSearchTool;

impl FileSearchTool {
    pub fn new() -> Self {
        Self
    }
    
    /// Expand paths like ~/Documents to absolute paths
    fn expand_path(&self, path: &str) -> PathBuf {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..])
            } else {
                PathBuf::from(path)
            }
        } else {
            PathBuf::from(path)
        }
    }
    
    /// Check if file matches the criteria
    fn matches_criteria(
        &self,
        entry: &walkdir::DirEntry,
        name_pattern: Option<&str>,
        extension: Option<&str>,
        min_size: Option<u64>,
        max_size: Option<u64>,
        modified_within_days: Option<u64>,
    ) -> anyhow::Result<bool> {
        let metadata = entry.metadata()?;
        
        // Only check files (not directories)
        if !metadata.is_file() {
            return Ok(false);
        }
        
        let file_name = entry.file_name().to_string_lossy();
        
        // Check name pattern (case-insensitive)
        if let Some(pattern) = name_pattern {
            let pattern_lower = pattern.to_lowercase();
            let name_lower = file_name.to_lowercase();
            
            // Support wildcards: * matches any sequence, ? matches single char
            if !self.wildcard_match(&name_lower, &pattern_lower) {
                return Ok(false);
            }
        }
        
        // Check extension (case-insensitive)
        if let Some(ext) = extension {
            let file_ext = Path::new(&*file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if file_ext != ext.to_lowercase() {
                return Ok(false);
            }
        }
        
        // Check size
        let size = metadata.len();
        if let Some(min) = min_size {
            if size < min {
                return Ok(false);
            }
        }
        if let Some(max) = max_size {
            if size > max {
                return Ok(false);
            }
        }
        
        // Check modification time
        if let Some(days) = modified_within_days {
            if let Ok(modified) = metadata.modified() {
                let modified_time = std::time::SystemTime::from(modified);
                let now = std::time::SystemTime::now();
                let duration = now.duration_since(modified_time)?;
                let days_since = duration.as_secs() / 86400;
                if days_since > days {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }
    
    /// Simple wildcard matching (* = any sequence, ? = single char)
    fn wildcard_match(&self, text: &str, pattern: &str) -> bool {
        let mut text_chars = text.chars().peekable();
        let mut pattern_chars = pattern.chars().peekable();
        
        while let Some(p) = pattern_chars.next() {
            match p {
                '*' => {
                    // Skip any sequence in text
                    let next_p = pattern_chars.peek().copied();
                    if next_p.is_none() {
                        return true; // * at end matches everything
                    }
                    // Find the next non-star character in pattern
                    while pattern_chars.peek() == Some(&'*') {
                        pattern_chars.next();
                    }
                    let next_p = pattern_chars.peek().copied();
                    if next_p.is_none() {
                        return true;
                    }
                    // Try to match the rest
                    let remaining_pattern: String = pattern_chars.clone().collect();
                    let remaining_text: String = text_chars.clone().collect();
                    // Try matching at each position
                    for i in 0..=remaining_text.len() {
                        if self.wildcard_match(&remaining_text[i..], &remaining_pattern) {
                            return true;
                        }
                    }
                    return false;
                }
                '?' => {
                    // Match any single character
                    if text_chars.next().is_none() {
                        return false;
                    }
                }
                c => {
                    // Match exact character
                    match text_chars.next() {
                        Some(tc) if tc.to_lowercase().next() == Some(c.to_lowercase().next().unwrap_or(c)) => {}
                        _ => return false,
                    }
                }
            }
        }
        
        text_chars.next().is_none()
    }
    
    /// Format file size for human reading
    fn format_size(&self, size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_idx = 0;
        
        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }
        
        if unit_idx == 0 {
            format!("{} {}", size as u64, UNITS[unit_idx])
        } else {
            format!("{:.1} {}", size, UNITS[unit_idx])
        }
    }
    
    /// Format timestamp
    fn format_time(&self, time: std::time::SystemTime) -> String {
        let datetime: chrono::DateTime<chrono::Local> = time.into();
        datetime.format("%Y-%m-%d %H:%M").to_string()
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }
    
    fn description(&self) -> &str {
        "Search for files by name pattern, extension, size, or modification date. \
         Cross-platform: Works on Windows, macOS, and Linux. \
         Supports wildcards (* and ?) in filename patterns. \
         Use this when you need to find files without using shell commands."
    }
    
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to search in. Use ~/ for home directory. Default: current directory"
                },
                "name_pattern": {
                    "type": "string",
                    "description": "Filename pattern with wildcards. * matches any sequence, ? matches single char. Examples: '*.pdf', 'img_*', 'report?.txt'"
                },
                "extension": {
                    "type": "string",
                    "description": "File extension to filter by (without dot). Examples: 'pdf', 'txt', 'py'"
                },
                "min_size": {
                    "type": "integer",
                    "description": "Minimum file size in bytes"
                },
                "max_size": {
                    "type": "integer",
                    "description": "Maximum file size in bytes"
                },
                "modified_within_days": {
                    "type": "integer",
                    "description": "Only show files modified within this many days"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Search subdirectories recursively. Default: true",
                    "default": true
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return. Default: 100",
                    "default": 100
                }
            },
            "required": []
        })
    }
    
    async fn execute(&self, args: Value) -> Result<ToolResult, anyhow::Error> {
        let path = args["path"].as_str().unwrap_or(".");
        let name_pattern = args["name_pattern"].as_str();
        let extension = args["extension"].as_str();
        let min_size = args["min_size"].as_u64();
        let max_size = args["max_size"].as_u64();
        let modified_within_days = args["modified_within_days"].as_u64();
        let recursive = args["recursive"].as_bool().unwrap_or(true);
        let max_results = args["max_results"].as_u64().unwrap_or(100) as usize;
        
        // Expand path
        let search_path = self.expand_path(path);
        
        // Verify path exists
        if !search_path.exists() {
            return Ok(ToolResult::error(format!(
                "Path does not exist: {}\n\n💡 Hint: Use ~/ for home directory (e.g., ~/Documents)",
                search_path.display()
            )));
        }
        
        if !search_path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path is not a directory: {}",
                search_path.display()
            )));
        }
        
        let mut results = Vec::new();
        let mut searched_count = 0;
        
        // Build walker
        let walker = if recursive {
            WalkDir::new(&search_path)
        } else {
            WalkDir::new(&search_path).max_depth(1)
        };
        
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Warning: Error accessing path: {}", e);
                    continue;
                }
            };
            
            searched_count += 1;
            
            match self.matches_criteria(
                &entry,
                name_pattern,
                extension,
                min_size,
                max_size,
                modified_within_days,
            ) {
                Ok(true) => {
                    if let Ok(metadata) = entry.metadata() {
                        let size = self.format_size(metadata.len());
                        let modified = metadata.modified()
                            .map(|t| self.format_time(t))
                            .unwrap_or_else(|_| "Unknown".to_string());
                        
                        results.push(format!(
                            "📄 {} ({}, modified: {})",
                            entry.path().display(),
                            size,
                            modified
                        ));
                        
                        if results.len() >= max_results {
                            break;
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    eprintln!("Warning: Error checking file {}: {}", entry.path().display(), e);
                }
            }
        }
        
        // Build output
        let mut output = format!(
            "🔍 File Search Results\n\
            📂 Directory: {}\n\
            🔎 Searched: {} items\n\
            ✅ Found: {} matches\n\n",
            search_path.display(),
            searched_count,
            results.len()
        );
        
        // Add search criteria summary
        let mut criteria = Vec::new();
        if let Some(p) = name_pattern {
            criteria.push(format!("name like '{}'", p));
        }
        if let Some(e) = extension {
            criteria.push(format!("extension: .{}", e));
        }
        if min_size.is_some() || max_size.is_some() {
            criteria.push(format!("size: {}-{} bytes", 
                min_size.map(|s| s.to_string()).unwrap_or_else(|| "0".to_string()),
                max_size.map(|s| s.to_string()).unwrap_or_else(|| "∞".to_string())
            ));
        }
        if let Some(d) = modified_within_days {
            criteria.push(format!("modified within {} days", d));
        }
        
        if !criteria.is_empty() {
            output.push_str(&format!("📝 Criteria: {}\n\n", criteria.join(", ")));
        }
        
        if results.is_empty() {
            output.push_str("❌ No files found matching your criteria.\n");
            output.push_str("\n💡 Try:\n");
            output.push_str("   - Using * wildcard for broader matches (e.g., '*.pdf')\n");
            output.push_str("   - Checking if the directory path is correct\n");
            output.push_str("   - Removing some filters to broaden the search\n");
        } else {
            output.push_str(&results.join("\n"));
            if results.len() >= max_results {
                output.push_str(&format!("\n\n⚠️ Showing first {} results. Use max_results parameter to see more.", max_results));
            }
        }
        
        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match() {
        let tool = FileSearchTool::new();
        
        assert!(tool.wildcard_match("test.pdf", "*.pdf"));
        assert!(tool.wildcard_match("test.pdf", "test.*"));
        assert!(tool.wildcard_match("test.pdf", "*.txt"));
        assert!(tool.wildcard_match("IMG_001.jpg", "IMG_*.jpg"));
        assert!(tool.wildcard_match("report1.txt", "report?.txt"));
        assert!(tool.wildcard_match("report10.txt", "report*.txt"));
        assert!(!tool.wildcard_match("test.pdf", "*.txt"));
        assert!(!tool.wildcard_match("report10.txt", "report?.txt"));
    }

    #[test]
    fn test_format_size() {
        let tool = FileSearchTool::new();
        
        assert_eq!(tool.format_size(500), "500 B");
        assert_eq!(tool.format_size(1536), "1.5 KB");
        assert_eq!(tool.format_size(1048576), "1.0 MB");
    }
}
