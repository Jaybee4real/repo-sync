mod commands;
mod config;
mod reports;
mod repos;
mod scheduler;
mod state;
mod tray;

use state::AppState;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::load();
    let app_state = AppState::new(cfg);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::scan_repos,
            commands::sync_now,
            commands::get_last_report,
            commands::list_reports,
            commands::load_report,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tray::init(&handle)?;
            scheduler::start(handle.clone());

            // Hide the main window on first open — the tray is the primary UI.
            // Users summon the window via "Open dashboard…".
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window just hides it; the app stays alive in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
