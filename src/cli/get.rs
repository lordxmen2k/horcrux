use crate::db::Db;
use anyhow::Result;
use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Document path or #docid
    pub target: String,
    /// Maximum lines to return
    #[arg(short = 'l', long)]
    pub lines: Option<usize>,
    /// Start from line number
    #[arg(long)]
    pub from: Option<usize>,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: GetArgs, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;

    let doc = if args.target.starts_with('#') {
        let docid = args.target.trim_start_matches('#');
        db.get_document(docid)?
    } else {
        db.find_document_by_path(&args.target)?
    };

    match doc {
        None => {
            eprintln!("Document not found: {}", args.target);
            std::process::exit(1);
        }
        Some(d) => {
            let lines: Vec<&str> = d.body.lines().collect();
            let from = args.from.unwrap_or(0);
            let to = args.lines
                .map(|n| (from + n).min(lines.len()))
                .unwrap_or(lines.len());
            let body = lines[from..to].join("\n");

            if args.json {
                let out = serde_json::json!({
                    "path": d.path,
                    "docid": format!("#{}", d.docid),
                    "title": d.title,
                    "body": body,
                    "line_count": lines.len(),
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("# {} ({})\n", d.title, d.path);
                println!("{}", body);
            }
        }
    }

    Ok(())
}