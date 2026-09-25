/// User configuration loaded from `~/.engram/config.toml`.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// Parsed contents of `~/.engram/config.toml`.
///
/// All fields are optional; missing keys fall back to built-in defaults.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Additional file extensions to index beyond the built-in defaults.
    ///
    /// Extensions are matched case-insensitively and without a leading dot,
    /// e.g. `extensions = ["csv", "yaml"]`.
    pub extensions: Option<Vec<String>>,
}

impl Config {
    /// Load config from the default path (`~/.engram/config.toml`).
    ///
    /// Returns `Config::default()` if the file does not exist.
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load() -> Result<Config> {
        let path = config_path();
        if !path.exists() {
            return Ok(Config::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Config = toml::from_str(&contents)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }

    /// Return the configured extra extensions (lowercased, dot-stripped).
    pub fn extra_extensions(&self) -> Vec<String> {
        self.extensions
            .as_ref()
            .map(|exts| {
                exts.iter()
                    .map(|e| e.trim_start_matches('.').to_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Resolve the config file path: `~/.engram/config.toml`.
fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".engram").join("config.toml")
}
