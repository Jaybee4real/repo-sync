mod commands;
mod config;
mod editors;
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

    // Hydrate `last_report` from disk so the tray doesn't claim "Never synced"
    // when there's a saved report from earlier today (or yesterday).
    {
        let recent = reports::list_reports().into_iter().next();
        if let Some(date) = recent {
            if let Some(r) = reports::load_report(&date) {
                *app_state.last_report.lock().unwrap() = Some(r);
            }
        }
    }

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
            commands::list_editors,
            commands::open_in_editor,
        ])
        .setup(|app| {
            // Menu-bar-only on macOS: hide the Dock icon. Without this, the
            // app appears as a regular app in the Dock and Cmd+Tab list,
            // which is wrong for a tray utility.
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            tray::init(&handle)?;
            // Reflect any hydrated conflict state in the menu-bar indicator.
            tray::refresh(&handle);
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
                // Restore tray-only behavior — drop the Dock icon now that
                // the dashboard window is no longer visible.
                #[cfg(target_os = "macos")]
                {
                    let app = window.app_handle();
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
