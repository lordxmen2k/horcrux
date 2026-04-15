//! Document Reader Tool - Extract text from PDFs (text or scanned) and images via OCR

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

pub struct DocumentReaderTool;

impl DocumentReaderTool {
    pub fn new() -> Self {
        Self
    }

    /// Extract text from PDF - tries text extraction first, then OCR if needed
    fn extract_pdf_text(&self, pdf_path: &str) -> anyhow::Result<String> {
        // First try text extraction
        if let Ok(text) = self.extract_pdf_text_native(pdf_path) {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        
        // If no text or failed, use OCR
        self.extract_pdf_with_ocr(pdf_path)
    }

    /// Extract text from text-based PDF
    fn extract_pdf_text_native(&self, pdf_path: &str) -> anyhow::Result<String> {
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "import pdfplumber; \
                 pdf = pdfplumber.open('{}'); \
                 text = '\\n'.join([page.extract_text() or '' for page in pdf.pages]); \
                 pdf.close(); \
                 print(text)",
                pdf_path
            ))
            .output()?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!("PDF text extraction failed"))
        }
    }

    /// Convert PDF to images and OCR them
    fn extract_pdf_with_ocr(&self, pdf_path: &str) -> anyhow::Result<String> {
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "from pdf2image import convert_from_path; \
                 import pytesseract; \
                 import sys; \
                 pages = convert_from_path('{}', dpi=200, first_page=1, last_page=5); \
                 text = []; \
                 for i, page in enumerate(pages): \
                     page_text = pytesseract.image_to_string(page); \
                     if page_text.strip(): \
                         text.append(f'--- Page {{i+1}} ---\\n{{page_text}}'); \
                 print('\\n\\n'.join(text))",
                pdf_path
            ))
            .output()?;
        
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if text.trim().is_empty() {
                Err(anyhow::anyhow!("OCR found no text in PDF"))
            } else {
                Ok(text)
            }
        } else {
            Err(anyhow::anyhow!("PDF OCR failed: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }

    /// OCR an image file
    fn ocr_image(&self, image_path: &str) -> anyhow::Result<String> {
        let output = std::process::Command::new("python")
            .arg("-c")
            .arg(format!(
                "from PIL import Image; \
                 import pytesseract; \
                 img = Image.open('{}'); \
                 text = pytesseract.image_to_string(img); \
                 print(text)",
                image_path
            ))
            .output()?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!("OCR failed: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }

    /// Search for text in PDF using extraction or OCR
    fn search_pdf(&self, pdf_path: &str, query: &str) -> anyhow::Result<bool> {
        let text = self.extract_pdf_text(pdf_path)?;
        Ok(text.to_lowercase().contains(&query.to_lowercase()))
    }
}

#[async_trait]
impl Tool for DocumentReaderTool {
    fn name(&self) -> &str {
        "document_reader"
    }

    fn description(&self) -> &str {
        "Extract text from PDFs (text or scanned) and images via OCR. \
         Use this to read document contents or search inside documents. \
         Handles both text-based PDFs and image-based/scanned PDFs."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to PDF or image file"
                },
                "operation": {
                    "type": "string",
                    "enum": ["extract", "search"],
                    "description": "extract=read all text, search=check if query exists"
                },
                "query": {
                    "type": "string",
                    "description": "Text to search for (required for search operation)"
                }
            },
            "required": ["path", "operation"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let path_str = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let operation = args["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing operation"))?;

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
            return Ok(ToolResult::error(format!("File not found: {}", path_str)));
        }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match operation {
            "extract" => {
                let result = match ext.as_str() {
                    "pdf" => self.extract_pdf_text(&path_str),
                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" => {
                        self.ocr_image(&path_str)
                    }
                    _ => Err(anyhow::anyhow!("Unsupported file type: {}", ext))
                };

                match result {
                    Ok(text) => {
                        let preview = if text.len() > 2000 {
                            format!("{}...\n\n[Truncated - {} total characters]", &text[..2000], text.len())
                        } else {
                            text
                        };
                        Ok(ToolResult::success(preview))
                    }
                    Err(e) => Ok(ToolResult::error(format!("Failed to extract text: {}", e)))
                }
            }
            "search" => {
                let query = args["query"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing query for search operation"))?;

                let found = match ext.as_str() {
                    "pdf" => self.search_pdf(&path_str, query).unwrap_or(false),
                    "txt" | "md" | "csv" => {
                        // Simple text file search
                        match std::fs::read_to_string(&path_str) {
                            Ok(content) => content.to_lowercase().contains(&query.to_lowercase()),
                            Err(_) => false
                        }
                    }
                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" => {
                        // OCR the image and search
                        match self.ocr_image(&path_str) {
                            Ok(text) => text.to_lowercase().contains(&query.to_lowercase()),
                            Err(_) => false
                        }
                    }
                    _ => false
                };

                if found {
                    Ok(ToolResult::success(format!(
                        "✅ Found '{}' in {}", query, path_str
                    )))
                } else {
                    Ok(ToolResult::success(format!(
                        "❌ '{}' not found in {}", query, path_str
                    )))
                }
            }
            _ => Ok(ToolResult::error(format!("Unknown operation: {}", operation)))
        }
    }
}
