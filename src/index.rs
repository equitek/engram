/// Core indexing logic: file scanning, chunking, embedding, and search.
use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::db::Db;
use crate::embed::{self, Provider};

const SUPPORTED_EXTENSIONS: &[&str] = &["md", "txt", "rst", "org", "adoc"];
const SNIPPET_LEN: usize = 300;
/// Maximum characters per chunk (~1500 tokens, well within nomic-embed-text's 8192 limit)
const CHUNK_SIZE: usize = 6000;
/// Overlap between chunks so context isn't lost at boundaries
const CHUNK_OVERLAP: usize = 200;

// ---------------------------------------------------------------------------
// Public command handlers
// ---------------------------------------------------------------------------

pub fn add(
    paths: &[String],
    recursive: bool,
    no_progress: bool,
    db_override: Option<&Path>,
) -> Result<()> {
    let db_path = resolve_db_path(db_override)?;

    // Auto-initialize on first use
    let is_new = !db_path.exists();
    if is_new {
        std::fs::create_dir_all(db_path.parent().unwrap())?;
    }

    let db = Db::open(&db_path)?;

    if is_new {
        let provider = embed::detect_provider();
        db.init(provider.dims(), provider.name())?;
        println!(
            "✓ Created index at {} (provider: {})",
            db_path.display(),
            provider.name()
        );
    }
    let provider = load_provider(&db)?;

    // Preflight: verify the embedding provider is reachable and the model is
    // available before processing any files. This catches configuration
    // problems (e.g. missing Ollama model) early with a clear error.
    embed::embed("preflight check", &provider).context("Embedding provider is not available")?;

    let files = collect_files(paths, recursive);
    if files.is_empty() {
        println!("No supported files found.");
        return Ok(());
    }

    let bar: Option<ProgressBar> = if no_progress {
        None
    } else {
        let b = ProgressBar::new(files.len() as u64);
        b.set_style(
            ProgressStyle::with_template("{msg:40} [{bar:40}] {pos}/{len}")
                .unwrap()
                .progress_chars("=> "),
        );
        b.set_message("Indexing...");
        Some(b)
    };

    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for path in &files {
        if let Some(b) = &bar {
            b.set_message(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            );
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                if no_progress {
                    eprintln!("error reading {}: {e}", path.display());
                }
                errors += 1;
                if let Some(b) = &bar {
                    b.inc(1);
                }
                continue;
            }
        };

        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        let path_str = path.to_string_lossy().to_string();

        if db.get_hash(&path_str)?.as_deref() == Some(&hash) {
            if no_progress {
                println!("  unchanged  {path_str}");
            }
            skipped += 1;
            if let Some(b) = &bar {
                b.inc(1);
            }
            continue;
        }

        let snippet = make_snippet(&content);
        let doc_id = match db.upsert_document(&path_str, &hash, &snippet) {
            Ok(id) => id,
            Err(e) => {
                if no_progress {
                    eprintln!("  error      {path_str}: {e:#}");
                }
                errors += 1;
                if let Some(b) = &bar {
                    b.inc(1);
                }
                continue;
            }
        };

        let chunks = chunk_text(&content);
        let mut chunk_ok = true;
        for (i, chunk) in chunks.iter().enumerate() {
            if no_progress {
                print!("  indexing   {path_str} ({}/{}) ... ", i + 1, chunks.len());
                let _ = std::io::stdout().flush();
                // std::io::stdout is buffered; use Write trait
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            let embedding = match embed::embed(chunk, &provider) {
                Ok(v) => v,
                Err(e) => {
                    if no_progress {
                        println!("embed error: {e:#}");
                    }
                    chunk_ok = false;
                    break;
                }
            };
            if let Err(e) = db.insert_chunk(doc_id, &embedding) {
                if no_progress {
                    println!("insert error: {e}");
                }
                chunk_ok = false;
                break;
            }
            if no_progress {
                println!("ok");
            }
        }

        if chunk_ok {
            added += 1;
        } else {
            errors += 1;
        }
        if let Some(b) = &bar {
            b.inc(1);
        }
    }

    if let Some(b) = &bar {
        b.finish_and_clear();
    }

    let total = db.document_count()?;
    if errors > 0 {
        println!("✓ Indexed {added} files ({skipped} unchanged, {errors} errors, {total} total)");
    } else {
        println!("✓ Indexed {added} files ({skipped} unchanged, {total} total)");
    }
    Ok(())
}

/// JSON-serializable search result for `--json` output.
#[derive(Serialize)]
pub struct JsonResult {
    pub path: String,
    pub snippet: String,
    pub distance: f32,
}

pub fn search(
    query: &str,
    limit: usize,
    show_path: bool,
    json: bool,
    db_override: Option<&Path>,
) -> Result<()> {
    let db_path = require_db(db_override)?;
    let db = Db::open(&db_path)?;
    let provider = load_provider(&db)?;

    let embedding = embed::embed(query, &provider)?;
    let results = db.search(&embedding, limit)?;

    if json {
        let json_results: Vec<JsonResult> = results
            .iter()
            .map(|r| JsonResult {
                path: r.path.clone(),
                snippet: r.snippet.clone(),
                distance: r.distance,
            })
            .collect();
        let json_output = serde_json::to_string_pretty(&json_results)?;
        println!("{json_output}");
        return Ok(());
    }

    if results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    for (i, r) in results.iter().enumerate() {
        println!("{:2}. {} (dist: {:.3})", i + 1, r.path, r.distance);
        if !show_path {
            println!("    {}\n", r.snippet.replace('\n', " "));
        }
    }
    Ok(())
}

pub fn remove(paths: &[String], db_override: Option<&Path>) -> Result<()> {
    let db_path = require_db(db_override)?;
    let db = Db::open(&db_path)?;

    for path in paths {
        if db.remove_document(path)? {
            println!("✓ Removed {path}");
        } else {
            println!("  Not found: {path}");
        }
    }
    Ok(())
}

pub fn rebuild(db_override: Option<&Path>) -> Result<()> {
    let db_path = require_db(db_override)?;
    let db = Db::open(&db_path)?;
    let paths = db.all_paths()?;
    drop(db);

    // Delete and recreate the database
    std::fs::remove_file(&db_path)?;
    let provider = embed::detect_provider();
    let db = Db::open(&db_path)?;
    db.init(provider.dims(), provider.name())?;
    drop(db);

    let path_strings: Vec<String> = paths;
    add(&path_strings, false, false, db_override)
}

pub fn status(db_override: Option<&Path>) -> Result<()> {
    let db_path = require_db(db_override)?;
    let db = Db::open(&db_path)?;

    let count = db.document_count()?;
    let provider = db.get_meta("provider")?.unwrap_or_else(|| "unknown".into());
    let dims = db.get_meta("dims")?.unwrap_or_else(|| "?".into());
    let size = std::fs::metadata(&db_path)?.len();

    println!("engram index: {}", db_path.display());
    println!("  Documents : {count}");
    println!("  Provider  : {provider}");
    println!("  Dimensions: {dims}");
    println!("  Index size: {:.1} MB", size as f64 / 1_048_576.0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the DB path from the `ENGRAM_DB_PATH` env var, if set.
pub fn db_path_env() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ENGRAM_DB_PATH") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".engram").join("index.db"))
}

/// Resolve the DB path: `--index` override takes precedence, then `ENGRAM_DB_PATH`,
/// then the default `~/.engram/index.db`.
fn resolve_db_path(db_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = db_override {
        return Ok(p.to_path_buf());
    }
    db_path_env()
}

/// Like `resolve_db_path`, but also verifies the index exists (bails if not).
fn require_db(db_override: Option<&Path>) -> Result<PathBuf> {
    let path = resolve_db_path(db_override)?;
    if !path.exists() {
        bail!("engram not initialized. Run `engram init` first.");
    }
    Ok(path)
}

fn load_provider(db: &Db) -> Result<Provider> {
    let name = db.get_meta("provider")?.unwrap_or_default();
    let _dims: usize = db
        .get_meta("dims")?
        .unwrap_or_default()
        .parse()
        .unwrap_or(1536);

    Ok(match name.as_str() {
        "openai/text-embedding-3-small" => Provider::OpenAiSmall,
        "ollama/nomic-embed-text" => {
            let base_url = std::env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            Provider::OllamaNomic { base_url }
        }
        _ => {
            // Try to auto-detect and warn if it differs
            let detected = embed::detect_provider();
            eprintln!(
                "warning: unknown provider '{name}', using detected: {}",
                detected.name()
            );
            detected
        }
    })
}

fn collect_files(paths: &[String], recursive: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path_str in paths {
        let path = Path::new(path_str);
        if path.is_file() {
            if is_supported(path) {
                files.push(path.to_path_buf());
            }
        } else if path.is_dir() {
            let walker = WalkDir::new(path).max_depth(if recursive { usize::MAX } else { 1 });
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() && is_supported(entry.path()) {
                    files.push(entry.into_path());
                }
            }
        }
    }
    files
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e))
        .unwrap_or(false)
}

/// Round `idx` down to the nearest valid UTF-8 char boundary.
fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Split text into overlapping chunks for embedding.
/// Tries to split on paragraph boundaries where possible.
/// All slicing uses char-boundary-safe indices.
fn chunk_text(content: &str) -> Vec<String> {
    let content = content.trim();
    if content.len() <= CHUNK_SIZE {
        return vec![content.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < content.len() {
        let end = floor_char_boundary(content, start + CHUNK_SIZE);

        // Try to find a paragraph break near the end to split cleanly
        let split_at = if end < content.len() {
            content[start..end]
                .rfind("\n\n")
                .map(|i| floor_char_boundary(content, start + i + 2))
                .unwrap_or(end)
        } else {
            end
        };

        // Guard: split_at must be > start to make forward progress
        if split_at <= start {
            chunks.push(content[start..].to_string());
            break;
        }

        chunks.push(content[start..split_at].to_string());

        // Advance with overlap
        let next = floor_char_boundary(content, split_at.saturating_sub(CHUNK_OVERLAP));
        if next <= start {
            break;
        }
        start = next;
    }

    chunks
}

fn make_snippet(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= SNIPPET_LEN {
        trimmed.to_string()
    } else {
        let boundary = floor_char_boundary(trimmed, SNIPPET_LEN);
        format!("{}…", &trimmed[..boundary])
    }
}
