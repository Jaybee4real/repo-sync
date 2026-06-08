use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    /// Relative path from config root.
    pub rel_path: String,
    /// Absolute path.
    pub abs_path: PathBuf,
    /// Current branch name, or "DETACHED".
    pub branch: String,
    /// Whether origin remote exists.
    pub has_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PullOutcome {
    UpToDate,
    Updated { commits: u32 },
    Skipped { reason: String },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    pub repo: RepoInfo,
    pub outcome: PullOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncReport {
    pub started_at: String,
    pub finished_at: String,
    pub root: PathBuf,
    pub results: Vec<PullResult>,
}

impl SyncReport {
    pub fn summary(&self) -> (u32, u32, u32, u32) {
        let mut updated = 0;
        let mut up_to_date = 0;
        let mut skipped = 0;
        let mut failed = 0;
        for r in &self.results {
            match &r.outcome {
                PullOutcome::Updated { .. } => updated += 1,
                PullOutcome::UpToDate => up_to_date += 1,
                PullOutcome::Skipped { .. } => skipped += 1,
                PullOutcome::Failed { .. } => failed += 1,
            }
        }
        (updated, up_to_date, skipped, failed)
    }
}

/// Scan `root` recursively (up to 5 levels deep) and return every git repo found.
pub fn scan(root: &Path) -> Vec<RepoInfo> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    for entry in WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_name() == ".git" && entry.file_type().is_dir() {
            let repo = entry.path().parent().unwrap().to_path_buf();
            let rel = repo
                .strip_prefix(root)
                .unwrap_or(&repo)
                .to_string_lossy()
                .into_owned();
            let branch = current_branch(&repo);
            let has_remote = has_origin(&repo);
            out.push(RepoInfo {
                rel_path: rel,
                abs_path: repo,
                branch,
                has_remote,
            });
        }
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

fn current_branch(repo: &Path) -> String {
    Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "DETACHED".to_string())
}

fn has_origin(repo: &Path) -> bool {
    Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn head_sha(repo: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Run `git pull --ff-only` in `repo` and classify the outcome.
pub fn pull(repo: &RepoInfo) -> PullOutcome {
    if !repo.has_remote {
        return PullOutcome::Skipped {
            reason: "no origin remote".to_string(),
        };
    }
    let before = head_sha(&repo.abs_path);
    let out = match Command::new("git")
        .args(["pull", "--ff-only", "--no-rebase"])
        .current_dir(&repo.abs_path)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return PullOutcome::Failed {
                message: format!("failed to spawn git: {}", e),
            }
        }
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = stderr
            .lines()
            .chain(stdout.lines())
            .filter(|l| !l.trim().is_empty())
            .last()
            .unwrap_or("git pull failed")
            .to_string();
        return PullOutcome::Failed { message: msg };
    }

    let after = head_sha(&repo.abs_path);
    match (before, after) {
        (Some(b), Some(a)) if b == a => PullOutcome::UpToDate,
        (Some(b), Some(a)) => {
            let commits = Command::new("git")
                .args(["rev-list", "--count", &format!("{}..{}", b, a)])
                .current_dir(&repo.abs_path)
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok())
                .unwrap_or(0);
            PullOutcome::Updated { commits }
        }
        _ => PullOutcome::UpToDate,
    }
}

/// Pull every repo in the list, filtered by `enabled` (key = `rel_path`).
pub fn pull_all(
    root: &Path,
    repos: &[RepoInfo],
    enabled: &std::collections::HashMap<String, bool>,
) -> SyncReport {
    let started_at = chrono::Local::now().to_rfc3339();
    let mut results = Vec::with_capacity(repos.len());
    for r in repos {
        let on = *enabled.get(&r.rel_path).unwrap_or(&true);
        if !on {
            results.push(PullResult {
                repo: r.clone(),
                outcome: PullOutcome::Skipped {
                    reason: "disabled".to_string(),
                },
            });
            continue;
        }
        let outcome = pull(r);
        results.push(PullResult {
            repo: r.clone(),
            outcome,
        });
    }
    SyncReport {
        started_at,
        finished_at: chrono::Local::now().to_rfc3339(),
        root: root.to_path_buf(),
        results,
    }
}
