use crate::{config, repos, reports, state::AppState};
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> config::AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(
    state: tauri::State<'_, AppState>,
    new_config: config::AppConfig,
) -> Result<(), String> {
    {
        let mut cfg = state.config.lock().unwrap();
        *cfg = new_config.clone();
    }
    config::save(&new_config).map_err(|e| e.to_string())
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

/// Run a sync now. Returns the final report. Emits `sync-started` /
/// `sync-progress` / `sync-finished` events so the UI can show progress.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<repos::SyncReport, String> {
    {
        let state = app.state::<AppState>();
        let mut s = state.syncing.lock().unwrap();
        if *s {
            return Err("sync already in progress".to_string());
        }
        *s = true;
    }

    let _ = app.emit("sync-started", ());

    let (root, enabled) = {
        let state = app.state::<AppState>();
        let cfg = state.config.lock().unwrap();
        (cfg.root.clone(), cfg.repo_enabled.clone())
    };

    // Heavy work on a blocking thread so we don't stall the runtime.
    let report = tokio::task::spawn_blocking(move || {
        let list = repos::scan(&root);
        repos::pull_all(&root, &list, &enabled)
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let state = app.state::<AppState>();
        *state.last_report.lock().unwrap() = Some(report.clone());
        let mut cfg = state.config.lock().unwrap();
        cfg.last_run = Some(report.finished_at.clone());
        let _ = config::save(&cfg);
    }

    let _ = reports::save_report(&report);
    let _ = app.emit("sync-finished", &report);

    {
        let state = app.state::<AppState>();
        *state.syncing.lock().unwrap() = false;
    }

    // Update tray label
    crate::tray::refresh(&app);

    Ok(report)
}
