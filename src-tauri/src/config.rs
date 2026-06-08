use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Absolute path to the root folder containing repo folders.
    pub root: PathBuf,
    /// Hour of day (0-23) to run the daily sync, local time.
    pub schedule_hour: u32,
    /// Minute (0-59).
    pub schedule_minute: u32,
    /// Per-repo enable flag, keyed by repo path relative to `root`.
    /// Default true if missing.
    #[serde(default)]
    pub repo_enabled: HashMap<String, bool>,
    /// ISO timestamp of the last successful run, if any.
    #[serde(default)]
    pub last_run: Option<String>,
    /// Show a notification when the sync finishes.
    #[serde(default = "default_true")]
    pub notify_on_finish: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        let root = dirs::document_dir()
            .map(|d| d.join("Programming-Codes"))
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            root,
            schedule_hour: 8,
            schedule_minute: 0,
            repo_enabled: HashMap::new(),
            last_run: None,
            notify_on_finish: true,
        }
    }
}

/// Where the config JSON lives. Uses the OS app-config dir under our identifier.
pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("repo-sync").join("config.json")
}

pub fn load() -> AppConfig {
    let p = config_path();
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save(cfg: &AppConfig) -> anyhow::Result<()> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}
