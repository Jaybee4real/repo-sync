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
    /// When true, scheduled runs are skipped. Manual "Sync now" still works.
    #[serde(default)]
    pub paused: bool,
    /// Days of report history to keep on disk; older reports are pruned after
    /// each sync.
    #[serde(default = "default_keep_reports_days")]
    pub keep_reports_days: u32,
}

fn default_true() -> bool {
    true
}

fn default_keep_reports_days() -> u32 {
    90
}

impl AppConfig {
    /// Reject configs that would put the scheduler or scanner in a bad state.
    pub fn validate(&self) -> Result<(), String> {
        if self.schedule_hour > 23 {
            return Err(format!("schedule_hour must be 0-23, got {}", self.schedule_hour));
        }
        if self.schedule_minute > 59 {
            return Err(format!("schedule_minute must be 0-59, got {}", self.schedule_minute));
        }
        if !self.root.is_dir() {
            return Err(format!("root is not a directory: {}", self.root.display()));
        }
        if self.keep_reports_days == 0 {
            return Err("keep_reports_days must be at least 1".to_string());
        }
        Ok(())
    }
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
            paused: false,
            keep_reports_days: default_keep_reports_days(),
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
        Ok(s) => match serde_json::from_str(&s) {
            Ok(cfg) => cfg,
            Err(_) => {
                // Don't silently destroy a config that failed to parse (it holds
                // the per-repo enable map); park it next to the original.
                let _ = std::fs::rename(&p, p.with_extension("json.bak"));
                AppConfig::default()
            }
        },
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
