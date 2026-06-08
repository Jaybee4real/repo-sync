export interface AppConfig {
  root: string;
  schedule_hour: number;
  schedule_minute: number;
  repo_enabled: Record<string, boolean>;
  last_run: string | null;
  notify_on_finish: boolean;
}

export interface RepoInfo {
  rel_path: string;
  abs_path: string;
  branch: string;
  has_remote: boolean;
}

export type PullOutcome =
  | { kind: "up_to_date" }
  | { kind: "updated"; commits: number }
  | { kind: "skipped"; reason: string }
  | { kind: "failed"; message: string };

export interface PullResult {
  repo: RepoInfo;
  outcome: PullOutcome;
}

export interface SyncReport {
  started_at: string;
  finished_at: string;
  root: string;
  results: PullResult[];
}
