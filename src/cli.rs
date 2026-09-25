use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "engram",
    version,
    about = "Fast, portable knowledge base with semantic search",
    long_about = "engram indexes your plain text and markdown files into a local sqlite-vec\n\
                  database and lets you search them by meaning, not just keywords.\n\n\
                  Your files are never modified. The index is a sidecar artifact\n\
                  stored in ~/.engram/index.db — delete it and rebuild anytime.\n\n\
                  No setup required. The index is created automatically on first use."
)]
pub struct Cli {
    /// Path to the index database (overrides ~/.engram/index.db and ENGRAM_DB_PATH)
    #[arg(long, global = true)]
    pub index: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add files or directories to the index
    Add {
        /// Paths to index (files or directories)
        #[arg(required = true)]
        paths: Vec<String>,

        /// Recursively index directories
        #[arg(short, long, default_value_t = true)]
        recursive: bool,

        /// Disable progress bar (plain text output, useful for scripts/logs)
        #[arg(long)]
        no_progress: bool,

        /// Extra file extensions to index this run (e.g. --ext csv --ext yaml)
        #[arg(long = "ext", value_name = "EXT")]
        ext: Vec<String>,
    },

    /// Search the knowledge base by meaning
    Search {
        /// Query string (natural language)
        query: String,

        /// Number of results to return
        #[arg(short, long, default_value_t = 10)]
        limit: usize,

        /// Only show file paths (no snippets)
        #[arg(short = 'p', long)]
        show_path: bool,

        /// Output results as JSON (machine-readable)
        #[arg(long)]
        json: bool,
    },

    /// Remove files from the index
    Remove {
        /// Paths to remove
        #[arg(required = true)]
        paths: Vec<String>,
    },

    /// Rebuild the entire index from scratch
    Rebuild,

    /// Show index statistics
    Status,
}
