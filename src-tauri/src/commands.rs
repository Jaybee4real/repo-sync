use crate::{config, editors, repos, reports, state::AppState};
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub fn list_editors() -> Vec<editors::Editor> {
    editors::detect()
}

/// Opens are confined to the configured root: the webview can only ask us to
/// open repos we manage, not arbitrary filesystem paths.
#[tauri::command]
pub fn open_in_editor(
    state: tauri::State<'_, AppState>,
    editor_id: String,
    path: String,
) -> Result<(), String> {
    let root = state.config.lock().unwrap().root.clone();
    let root = std::fs::canonicalize(&root).map_err(|e| format!("bad root: {}", e))?;
    let target =
        std::fs::canonicalize(&path).map_err(|_| format!("path does not exist: {}", path))?;
    if !target.starts_with(&root) {
        return Err("path is outside the configured root folder".to_string());
    }
    editors::open_in(&editor_id, &target.to_string_lossy())
}

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> config::AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    new_config: config::AppConfig,
) -> Result<(), String> {
    new_config.validate()?;
    {
        let mut cfg = state.config.lock().unwrap();
        *cfg = new_config.clone();
    }
    config::save(&new_config).map_err(|e| e.to_string())?;
    // Tray label depends on config (specifically `last_run` formatting); refresh
    // so any UI changes the user made show up immediately.
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn scan_repos(state: tauri::State<'_, AppState>) -> Vec<repos::RepoInfo> {
    let root = state.config.lock().unwrap().root.clone();
    repos::scan(&root)
}

#[tauri::command]
pub fn list_reports() -> Vec<String> {
    reports::list_reports()
}

#[tauri::command]
pub fn load_report(date: String) -> Option<repos::SyncReport> {
    reports::load_report(&date)
}

#[tauri::command]
pub fn get_last_report(state: tauri::State<'_, AppState>) -> Option<repos::SyncReport> {
    state.last_report.lock().unwrap().clone()
}

/// Guard that flips `syncing` back to false on drop, even on early return /
/// panic. Without this, an error path in `sync_now` would strand the UI on
/// "Syncing…" forever and block subsequent sync attempts.
struct SyncingGuard {
    app: AppHandle,
}
impl Drop for SyncingGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.app.state::<AppState>().syncing.lock() {
            *s = false;
        }
    }
}

/// Run a sync now. Returns the final report. Emits `sync-started` and
/// `sync-finished` events so the UI can show progress.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<repos::SyncReport, String> {
    // Take the syncing flag atomically and arm a Drop guard so it can't leak.
    {
        let state = app.state::<AppState>();
        let mut s = state.syncing.lock().unwrap();
        if *s {
            return Err("sync already in progress".to_string());
        }
        *s = true;
    }
    let _guard = SyncingGuard { app: app.clone() };

    let _ = app.emit("sync-started", ());

    let (root, cfg_snapshot, keep_days) = {
        let state = app.state::<AppState>();
        let cfg = state.config.lock().unwrap();
        (cfg.root.clone(), cfg.clone(), cfg.keep_reports_days)
    };

    // Heavy work on a blocking thread so we don't stall the runtime.
    let progress_app = app.clone();
    let report_result = tauri::async_runtime::spawn_blocking(move || {
        let list = repos::scan(&root);
        repos::pull_all(&root, &list, &cfg_snapshot, |index, total, rel_path| {
            let _ = progress_app.emit(
                "sync-progress",
                serde_json::json!({ "index": index, "total": total, "repo": rel_path }),
            );
        })
    })
    .await;

    let report = match report_result {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("sync failed: {}", e);
            // Notify the UI so it stops showing "Syncing…"; guard will reset the flag.
            let _ = app.emit("sync-failed", &msg);
            crate::tray::refresh(&app);
            return Err(msg);
        }
    };

    {
        let state = app.state::<AppState>();
        *state.last_report.lock().unwrap() = Some(report.clone());
        let mut cfg = state.config.lock().unwrap();
        cfg.last_run = Some(report.finished_at.clone());
        let _ = config::save(&cfg);
    }

    let _ = reports::save_report(&report);
    reports::prune(keep_days);
    let _ = app.emit("sync-finished", &report);

    crate::tray::refresh(&app);

    Ok(report)
}
