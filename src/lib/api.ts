import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, Editor, RepoInfo, SyncReport } from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (cfg: AppConfig) => invoke<void>("set_config", { newConfig: cfg }),
  scanRepos: () => invoke<RepoInfo[]>("scan_repos"),
  syncNow: () => invoke<SyncReport>("sync_now"),
  getLastReport: () => invoke<SyncReport | null>("get_last_report"),
  listReports: () => invoke<string[]>("list_reports"),
  loadReport: (date: string) => invoke<SyncReport | null>("load_report", { date }),
  listEditors: () => invoke<Editor[]>("list_editors"),
  openInEditor: (editorId: string, path: string) =>
    invoke<void>("open_in_editor", { editorId, path }),
};
