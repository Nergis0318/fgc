use crate::cli::{LfsBackend, Strategy};
use crate::error::{FgcError, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default_strategy: Option<String>,
    #[serde(default)]
    pub lfs_jobs: Option<u32>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub tui: Option<bool>,
    #[serde(default)]
    pub lfs_backend: Option<String>,
    #[serde(default)]
    pub aria2c_connections: Option<u32>,
}

impl Config {
    pub fn load() -> Self {
        match config_path() {
            Some(path) if path.exists() => Self::load_from(&path).unwrap_or_default(),
            _ => Self::default(),
        }
    }

    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| FgcError::new(format!("Failed to read config {}: {e}", path.display())))?;
        toml::from_str(&content)
            .map_err(|e| FgcError::new(format!("Failed to parse config {}: {e}", path.display())))
    }

    pub fn default_strategy(&self) -> Option<Strategy> {
        self.default_strategy
            .as_ref()
            .and_then(|s| parse_strategy(s))
    }

    pub fn lfs_backend(&self) -> Option<LfsBackend> {
        self.lfs_backend.as_ref().and_then(|s| parse_lfs_backend(s))
    }

    pub fn expand_reference(&self) -> Option<String> {
        self.reference.as_ref().map(|p| expand_tilde(p))
    }
}

pub fn config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("FGC_CONFIG") {
        return Some(PathBuf::from(path));
    }

    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok()?;

    Some(base.join("fgc").join("config.toml"))
}

pub fn parse_strategy(s: &str) -> Option<Strategy> {
    match s.to_lowercase().as_str() {
        "auto" => Some(Strategy::Auto),
        "blobless" => Some(Strategy::Blobless),
        "sparse" => Some(Strategy::Sparse),
        "shallow" => Some(Strategy::Shallow),
        "full" => Some(Strategy::Full),
        _ => None,
    }
}

pub fn parse_lfs_backend(s: &str) -> Option<LfsBackend> {
    match s.to_lowercase().as_str() {
        "auto" => Some(LfsBackend::Auto),
        "git" => Some(LfsBackend::Git),
        "aria2c" => Some(LfsBackend::Aria2c),
        _ => None,
    }
}

pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_path() {
        std::env::set_var("HOME", "/home/test");
        assert_eq!(expand_tilde("~/cache/fgc"), "/home/test/cache/fgc");
        std::env::remove_var("HOME");
    }

    #[test]
    fn parses_strategy_names() {
        assert_eq!(parse_strategy("blobless"), Some(Strategy::Blobless));
        assert_eq!(parse_strategy("AUTO"), Some(Strategy::Auto));
        assert_eq!(parse_strategy("invalid"), None);
    }

    #[test]
    fn parses_lfs_backend() {
        assert_eq!(parse_lfs_backend("aria2c"), Some(LfsBackend::Aria2c));
        assert_eq!(parse_lfs_backend("git"), Some(LfsBackend::Git));
    }
}
