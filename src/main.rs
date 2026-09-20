use anyhow::Result;
use clap::Parser;
use engram::cli::{Cli, Commands};
use engram::index;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli
        .index
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| index::db_path_env().ok());

    match cli.command {
        Commands::Add {
            paths,
            recursive,
            no_progress,
        } => index::add(&paths, recursive, no_progress, db_path.as_deref())?,
        Commands::Search {
            query,
            limit,
            show_path,
            json,
        } => index::search(&query, limit, show_path, json, db_path.as_deref())?,
        Commands::Remove { paths } => index::remove(&paths, db_path.as_deref())?,
        Commands::Rebuild => index::rebuild(db_path.as_deref())?,
        Commands::Status => index::status(db_path.as_deref())?,
    }
    Ok(())
}
