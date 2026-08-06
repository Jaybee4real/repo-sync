/** Per-repo pull configuration (branch targets + enable). */
export interface RepoSettings {
  enabled: boolean;
  /** Explicit branches to fast-forward; empty = every branch with an upstream. */
  branches: string[];
  /** Check this branch out before pulling. null = don't switch. */
  target_branch: string | null;
  /** Check this branch out after pulling. null = return to the original. */
  fallback_branch: string | null;
}

export type DirtyPolicy = "stash" | "skip";

export interface AppConfig {
  root: string;
  schedule_hour: number;
  schedule_minute: number;
  /** Legacy enable map, superseded by repo_settings (kept for migration). */
  repo_enabled: Record<string, boolean>;
  repo_settings: Record<string, RepoSettings>;
  on_dirty: DirtyPolicy;
  last_run: string | null;
  notify_on_finish: boolean;
  paused: boolean;
  keep_reports_days: number;
}

export interface SyncProgress {
  index: number;
  total: number;
  repo: string;
}

export interface RepoInfo {
  rel_path: string;
  abs_path: string;
  branch: string;
  has_remote: boolean;
}

export type BranchStatus =
  | { kind: "up_to_date" }
  | { kind: "fast_forwarded"; commits: number }
  | { kind: "diverged"; ahead: number; behind: number }
  | { kind: "no_upstream" };

export interface BranchResult {
  branch: string;
  status: BranchStatus;
}

export interface PullResult {
  repo: RepoInfo;
  original_branch: string;
  final_branch: string;
  branches: BranchResult[];
  had_local_changes: boolean;
  stash_conflict: boolean;
  skipped: string | null;
  error: string | null;
}

export interface SyncReport {
  started_at: string;
  finished_at: string;
  root: string;
  results: PullResult[];
}

export interface Editor {
  id: string;
  name: string;
  icon: string | null;
  is_reveal: boolean;
}

/** Mirror of PullResult::has_conflict in Rust. */
export function hasConflict(r: PullResult): boolean {
  return (
    r.stash_conflict || r.branches.some((b) => b.status.kind === "diverged")
  );
}

export function updatedCount(r: PullResult): number {
  return r.branches.filter((b) => b.status.kind === "fast_forwarded").length;
}
