use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Per-repo pull configuration. Lets a backend engineer running many services
/// point each repo at the branch it should track every morning and the branch
/// they want to land back on afterwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSettings {
    /// Whether this repo is pulled at all.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Explicit list of branches to fast-forward. Empty = every local branch
    /// that has an upstream (the original behaviour).
    #[serde(default)]
    pub branches: Vec<String>,
    /// Check this branch out before pulling (created as a tracking branch off
    /// origin if it doesn't exist locally). None = don't switch.
    #[serde(default)]
    pub target_branch: Option<String>,
    /// Check this branch out after pulling — the branch the engineer wants to be
    /// on when they sit down. None = return to whatever was checked out at start.
    #[serde(default)]
    pub fallback_branch: Option<String>,
}

impl Default for RepoSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            branches: Vec::new(),
            target_branch: None,
            fallback_branch: None,
        }
    }
}

/// What to do with a repo that has uncommitted tracked changes at pull time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DirtyPolicy {
    /// Auto-stash, pull, then restore (default — never loses work).
    Stash,
    /// Leave repos with local changes untouched and record them as skipped.
    Skip,
}

impl Default for DirtyPolicy {
    fn default() -> Self {
        DirtyPolicy::Stash
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Absolute path to the root folder containing repo folders.
    pub root: PathBuf,
    /// Hour of day (0-23) to run the daily sync, local time.
    pub schedule_hour: u32,
    /// Minute (0-59).
    pub schedule_minute: u32,
    /// Legacy per-repo enable flag, keyed by repo path relative to `root`.
    /// Superseded by `repo_settings`; kept so old configs migrate cleanly.
    #[serde(default)]
    pub repo_enabled: HashMap<String, bool>,
    /// Per-repo settings (enable + branch targets), keyed by repo path relative
    /// to `root`. Missing entries fall back to `repo_enabled`, then to defaults.
    #[serde(default)]
    pub repo_settings: HashMap<String, RepoSettings>,
    /// How to treat a repo with uncommitted tracked changes at pull time.
    #[serde(default)]
    pub on_dirty: DirtyPolicy,
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

    /// Effective settings for a repo, resolving the new per-repo map first, then
    /// the legacy `repo_enabled` flag, then defaults (enabled, pull all branches).
    pub fn settings_for(&self, rel_path: &str) -> RepoSettings {
        if let Some(s) = self.repo_settings.get(rel_path) {
            return s.clone();
        }
        let enabled = *self.repo_enabled.get(rel_path).unwrap_or(&true);
        RepoSettings {
            enabled,
            ..RepoSettings::default()
        }
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
            repo_settings: HashMap::new(),
            on_dirty: DirtyPolicy::Stash,
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
        Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
            Ok(mut cfg) => {
                // Migrate legacy `repo_enabled` entries into `repo_settings` so
                // the richer per-repo model is populated on first run after upgrade.
                for (rel, &on) in &cfg.repo_enabled {
                    cfg.repo_settings.entry(rel.clone()).or_insert(RepoSettings {
                        enabled: on,
                        ..RepoSettings::default()
                    });
                }
                cfg
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_for_prefers_new_map_then_legacy_then_default() {
        let mut cfg = AppConfig::default();

        // Nothing configured -> enabled, pull all branches, no branch switching.
        let d = cfg.settings_for("svc/api");
        assert!(d.enabled && d.branches.is_empty() && d.target_branch.is_none());

        // Legacy repo_enabled=false is honored when no new settings exist.
        cfg.repo_enabled.insert("svc/api".into(), false);
        assert!(!cfg.settings_for("svc/api").enabled);

        // New per-repo settings win over the legacy flag.
        cfg.repo_settings.insert(
            "svc/api".into(),
            RepoSettings {
                enabled: true,
                branches: vec!["develop".into()],
                target_branch: Some("develop".into()),
                fallback_branch: Some("my-feature".into()),
            },
        );
        let s = cfg.settings_for("svc/api");
        assert!(s.enabled);
        assert_eq!(s.branches, vec!["develop".to_string()]);
        assert_eq!(s.target_branch.as_deref(), Some("develop"));
        assert_eq!(s.fallback_branch.as_deref(), Some("my-feature"));
    }
}
