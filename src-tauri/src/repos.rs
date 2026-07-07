use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    /// Relative path from config root.
    pub rel_path: String,
    /// Absolute path.
    pub abs_path: PathBuf,
    /// Branch that was checked out when the sync started, or "DETACHED".
    pub branch: String,
    /// Whether origin remote exists.
    pub has_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BranchStatus {
    /// Already at upstream.
    UpToDate,
    /// Fast-forwarded by `commits` commits.
    FastForwarded { commits: u32 },
    /// Local and remote have both moved — cannot fast-forward. Needs attention.
    Diverged { ahead: u32, behind: u32 },
    /// Branch has no configured upstream, so nothing to pull.
    NoUpstream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    pub branch: String,
    pub status: BranchStatus,
}

/// Result of syncing a single repo across all its branches.
///
/// All of the v0.2 fields carry `#[serde(default)]` so that reports written by
/// the older v0.1 schema (which only had `repo` + `outcome`) still deserialize
/// — they degrade to an empty branch list rather than failing the whole load
/// and poisoning hydration / the Reports dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    pub repo: RepoInfo,
    /// The branch we started (and ended) on.
    #[serde(default)]
    pub original_branch: String,
    /// Per-branch outcomes.
    #[serde(default)]
    pub branches: Vec<BranchResult>,
    /// True if the repo had uncommitted changes that we stashed.
    #[serde(default)]
    pub had_local_changes: bool,
    /// True if restoring stashed changes left conflict markers in the tree.
    #[serde(default)]
    pub stash_conflict: bool,
    /// Set if the whole repo was skipped (e.g. no remote, or disabled).
    #[serde(default)]
    pub skipped: Option<String>,
    /// Set on a hard failure (e.g. `git fetch` failed).
    #[serde(default)]
    pub error: Option<String>,
}

impl PullResult {
    /// A repo "needs attention" if restoring stashed changes conflicted, or any
    /// branch diverged and could not be fast-forwarded.
    pub fn has_conflict(&self) -> bool {
        self.stash_conflict
            || self
                .branches
                .iter()
                .any(|b| matches!(b.status, BranchStatus::Diverged { .. }))
    }

    pub fn updated_count(&self) -> u32 {
        self.branches
            .iter()
            .filter(|b| matches!(b.status, BranchStatus::FastForwarded { .. }))
            .count() as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncReport {
    pub started_at: String,
    pub finished_at: String,
    pub root: PathBuf,
    pub results: Vec<PullResult>,
}

impl SyncReport {
    /// (repos_with_updates, repos_clean, repos_skipped, repos_failed, repos_conflict)
    pub fn summary(&self) -> (u32, u32, u32, u32, u32) {
        let mut updated = 0;
        let mut clean = 0;
        let mut skipped = 0;
        let mut failed = 0;
        let mut conflict = 0;
        for r in &self.results {
            if r.error.is_some() {
                failed += 1;
            } else if r.skipped.is_some() {
                skipped += 1;
            } else if r.has_conflict() {
                conflict += 1;
            } else if r.updated_count() > 0 {
                updated += 1;
            } else {
                clean += 1;
            }
        }
        (updated, clean, skipped, failed, conflict)
    }

    pub fn conflict_count(&self) -> u32 {
        self.results.iter().filter(|r| r.has_conflict()).count() as u32
    }

    /// True if this looks like a pre-v0.2 report (written before per-branch
    /// data existed). Every v0.2 result records an `original_branch`; a legacy
    /// report deserializes with that field empty for all entries. Used to
    /// trigger a fresh sync after a version upgrade so the dashboard isn't
    /// stuck showing schema-less data.
    pub fn is_legacy(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.original_branch.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Small git helpers
// ---------------------------------------------------------------------------

/// Hard cap on any single git invocation. A fetch hung on a dead network or a
/// credential prompt would otherwise stall the entire sync run forever.
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Run a git command in `repo`, returning (success, stdout, stderr).
///
/// Hardening, since this runs unattended against every repo in the folder:
/// - `core.fsmonitor=` — a repo's own .git/config can point fsmonitor at an
///   arbitrary binary that git would then execute; force it off.
/// - `GIT_TERMINAL_PROMPT=0` + ssh BatchMode — never wait on an interactive
///   credential prompt that can't be answered from a tray app.
/// - A hard timeout, after which the child is killed and the call fails.
fn git(repo: &Path, args: &[&str]) -> (bool, String, String) {
    use std::io::Read;
    use std::process::Stdio;
    use wait_timeout::ChildExt;

    let mut full_args: Vec<&str> = vec!["-c", "core.fsmonitor="];
    full_args.extend_from_slice(args);

    let spawned = Command::new("git")
        .args(&full_args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return (false, String::new(), e.to_string()),
    };

    // Drain the pipes on separate threads so a chatty command can't deadlock
    // against a full pipe buffer while we wait on it.
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        buf
    });

    match child.wait_timeout(GIT_TIMEOUT) {
        Ok(Some(status)) => {
            let out = stdout_reader.join().unwrap_or_default();
            let err = stderr_reader.join().unwrap_or_default();
            (status.success(), out.trim().to_string(), err.trim().to_string())
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            (
                false,
                String::new(),
                format!("git {} timed out after {}s", args.first().unwrap_or(&""), GIT_TIMEOUT.as_secs()),
            )
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            (false, String::new(), e.to_string())
        }
    }
}

fn current_branch(repo: &Path) -> String {
    let (ok, out, _) = git(repo, &["symbolic-ref", "--short", "HEAD"]);
    if ok && !out.is_empty() {
        out
    } else {
        "DETACHED".to_string()
    }
}

fn has_origin(repo: &Path) -> bool {
    git(repo, &["remote", "get-url", "origin"]).0
}

/// True if there are tracked, uncommitted changes (staged or unstaged).
/// Untracked files are ignored — they don't block a fast-forward.
fn has_tracked_changes(repo: &Path) -> bool {
    let (_, out, _) = git(repo, &["status", "--porcelain", "--untracked-files=no"]);
    !out.trim().is_empty()
}

fn stash_count(repo: &Path) -> u32 {
    let (ok, out, _) = git(repo, &["stash", "list"]);
    if !ok {
        return 0;
    }
    out.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

/// (ahead, behind) of `branch` relative to `upstream`.
fn ahead_behind(repo: &Path, branch: &str, upstream: &str) -> Option<(u32, u32)> {
    let spec = format!("{}...{}", branch, upstream);
    let (ok, out, _) = git(repo, &["rev-list", "--left-right", "--count", &spec]);
    if !ok {
        return None;
    }
    let mut parts = out.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// (branch, upstream-or-empty) for every local branch.
fn local_branches(repo: &Path) -> Vec<(String, String)> {
    let (ok, out, _) = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(upstream:short)",
            "refs/heads",
        ],
    );
    if !ok {
        return Vec::new();
    }
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(2, '|');
            let b = it.next()?.to_string();
            let up = it.next().unwrap_or("").to_string();
            if b.is_empty() {
                None
            } else {
                Some((b, up))
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The pull engine
// ---------------------------------------------------------------------------

/// Fetch and fast-forward every branch of a single repo.
///
/// Flow:
/// 1. `git fetch --all --prune`
/// 2. Stash tracked changes if the working tree is dirty.
/// 3. For each local branch with an upstream:
///    - up to date  -> record UpToDate
///    - can ff      -> advance it (merge --ff-only for the current branch,
///                     `branch -f` for the others) and record FastForwarded
///    - diverged    -> record Diverged (never auto-merged)
/// 4. Restore stashed changes with `git stash apply`. If that conflicts, leave
///    the conflict markers in the tree and keep the stash (flagged for the user).
pub fn pull(repo: &RepoInfo) -> PullResult {
    let path = &repo.abs_path;
    let original_branch = current_branch(path);

    let mut result = PullResult {
        repo: repo.clone(),
        original_branch: original_branch.clone(),
        branches: Vec::new(),
        had_local_changes: false,
        stash_conflict: false,
        skipped: None,
        error: None,
    };

    if !repo.has_remote {
        result.skipped = Some("no origin remote".to_string());
        return result;
    }

    // 1. Fetch everything up front.
    let (fok, _, ferr) = git(path, &["fetch", "--all", "--prune"]);
    if !fok {
        let msg = ferr.lines().last().unwrap_or("git fetch failed").to_string();
        result.error = Some(msg);
        return result;
    }

    // 2. Stash if dirty.
    let dirty = has_tracked_changes(path);
    let mut did_stash = false;
    if dirty {
        let before = stash_count(path);
        let (sok, _, _) = git(
            path,
            &["stash", "push", "-m", "repo-sync: auto-stash before update"],
        );
        did_stash = sok && stash_count(path) > before;
        result.had_local_changes = did_stash;
    }

    // 3. Update each branch.
    for (branch, upstream) in local_branches(path) {
        if upstream.is_empty() {
            result.branches.push(BranchResult {
                branch,
                status: BranchStatus::NoUpstream,
            });
            continue;
        }
        // Upstream ref must actually exist (it may have been pruned).
        if !git(path, &["rev-parse", "--verify", "--quiet", &upstream]).0 {
            result.branches.push(BranchResult {
                branch,
                status: BranchStatus::NoUpstream,
            });
            continue;
        }

        let (ahead, behind) = match ahead_behind(path, &branch, &upstream) {
            Some(v) => v,
            None => {
                result.branches.push(BranchResult {
                    branch,
                    status: BranchStatus::NoUpstream,
                });
                continue;
            }
        };

        if behind == 0 {
            result.branches.push(BranchResult {
                branch,
                status: BranchStatus::UpToDate,
            });
            continue;
        }
        if ahead > 0 {
            // Both sides moved — cannot fast-forward.
            result.branches.push(BranchResult {
                branch,
                status: BranchStatus::Diverged { ahead, behind },
            });
            continue;
        }

        // Pure fast-forward (ahead == 0, behind > 0).
        let advanced = if branch == original_branch {
            git(path, &["merge", "--ff-only", &upstream]).0
        } else {
            // Safe: we verified this is a fast-forward.
            git(path, &["branch", "-f", &branch, &upstream]).0
        };

        result.branches.push(BranchResult {
            branch,
            status: if advanced {
                BranchStatus::FastForwarded { commits: behind }
            } else {
                BranchStatus::Diverged { ahead, behind }
            },
        });
    }

    // 4. Restore stashed changes.
    if did_stash {
        let (aok, _, _) = git(path, &["stash", "apply"]);
        if aok {
            // Clean restore — drop the now-redundant stash.
            let _ = git(path, &["stash", "drop"]);
        } else {
            // Conflict (or other failure): keep the stash and flag it. The tree
            // now contains conflict markers for the user to resolve.
            result.stash_conflict = true;
        }
    }

    result
}

/// Pull every repo in the list, honoring the per-repo `enabled` map.
/// `progress` is called before each repo starts, with (index, total, rel_path).
pub fn pull_all(
    root: &Path,
    repos: &[RepoInfo],
    enabled: &HashMap<String, bool>,
    mut progress: impl FnMut(usize, usize, &str),
) -> SyncReport {
    let started_at = chrono::Local::now().to_rfc3339();
    let total = repos.len();
    let mut results = Vec::with_capacity(repos.len());
    for (index, r) in repos.iter().enumerate() {
        progress(index, total, &r.rel_path);
        let on = *enabled.get(&r.rel_path).unwrap_or(&true);
        if !on {
            let pr = PullResult {
                repo: r.clone(),
                original_branch: r.branch.clone(),
                branches: Vec::new(),
                had_local_changes: false,
                stash_conflict: false,
                skipped: Some("disabled".to_string()),
                error: None,
            };
            results.push(pr);
            continue;
        }
        results.push(pull(r));
    }
    SyncReport {
        started_at,
        finished_at: chrono::Local::now().to_rfc3339(),
        root: root.to_path_buf(),
        results,
    }
}
