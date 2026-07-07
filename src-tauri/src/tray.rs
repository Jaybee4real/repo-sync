use crate::scheduler::last_run_label;
use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ID_SYNC: &str = "sync_now";
const ID_PAUSE: &str = "toggle_pause";
const ID_OPEN: &str = "open_window";
const ID_QUIT: &str = "quit";

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .icon_as_template(true)
        .tooltip("repo-sync")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            ID_SYNC => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::commands::sync_now(app).await;
                });
            }
            ID_PAUSE => {
                let updated = {
                    let state = app.state::<AppState>();
                    let mut cfg = state.config.lock().unwrap();
                    cfg.paused = !cfg.paused;
                    cfg.clone()
                };
                let _ = crate::config::save(&updated);
                refresh(app);
            }
            ID_OPEN => {
                show_window(app);
            }
            ID_QUIT => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let (last_run, paused) = {
        let cfg = state.config.lock().unwrap();
        (cfg.last_run.clone(), cfg.paused)
    };
    let report = state.last_report.lock().unwrap().clone();

    let (status_label, conflicts) = match &report {
        Some(r) => {
            let (u, ok, sk, f, c) = r.summary();
            (
                format!(
                    "{} {} updated · {} ok · {} skipped · {} failed · {} conflict",
                    if c > 0 { "⚠" } else { "●" },
                    u,
                    ok,
                    sk,
                    f,
                    c
                ),
                r.conflict_count(),
            )
        }
        None => ("● No sync run yet".to_string(), 0),
    };

    let status_item = MenuItem::with_id(app, "status", &status_label, false, None::<&str>)?;
    let last_run_item =
        MenuItem::with_id(app, "last_run", &last_run_label(&last_run), false, None::<&str>)?;
    let sync_item = MenuItem::with_id(app, ID_SYNC, "Sync now", true, Some("Cmd+R"))?;
    let pause_label = if paused {
        "Resume scheduled syncs"
    } else {
        "Pause scheduled syncs"
    };
    let pause_item = MenuItem::with_id(app, ID_PAUSE, pause_label, true, None::<&str>)?;
    let open_item = MenuItem::with_id(app, ID_OPEN, "Open dashboard…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, ID_QUIT, "Quit repo-sync", true, Some("Cmd+Q"))?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    if conflicts > 0 {
        let conflict_item = MenuItem::with_id(
            app,
            ID_OPEN,
            &format!("⚠ {} repo{} need attention…", conflicts, if conflicts == 1 { "" } else { "s" }),
            true,
            None::<&str>,
        )?;
        Menu::with_items(
            app,
            &[
                &status_item,
                &last_run_item,
                &sep1,
                &conflict_item,
                &sync_item,
                &pause_item,
                &open_item,
                &sep2,
                &quit_item,
            ],
        )
    } else {
        Menu::with_items(
            app,
            &[
                &status_item,
                &last_run_item,
                &sep1,
                &sync_item,
                &pause_item,
                &open_item,
                &sep2,
                &quit_item,
            ],
        )
    }
}

/// Rebuild the tray menu (e.g. after a sync changes the status label) and
/// update the menu-bar indicator: a "!" next to the icon when any repo has a
/// conflict, cleared otherwise.
pub fn refresh(app: &AppHandle) {
    let (conflicts, paused) = {
        let state = app.state::<AppState>();
        let report = state.last_report.lock().unwrap();
        let paused = state.config.lock().unwrap().paused;
        (report.as_ref().map(|r| r.conflict_count()).unwrap_or(0), paused)
    };

    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
        // macOS shows this text next to the menu-bar icon. On Windows it's a
        // no-op, but the tooltip below covers that platform.
        let title = if conflicts > 0 {
            "!"
        } else if paused {
            "⏸"
        } else {
            ""
        };
        let _ = tray.set_title(Some(title));
        let tooltip = if conflicts > 0 {
            format!("repo-sync — {} repo(s) need attention", conflicts)
        } else if paused {
            "repo-sync — scheduled syncs paused".to_string()
        } else {
            "repo-sync".to_string()
        };
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}

fn show_window(app: &AppHandle) {
    // On macOS with `ActivationPolicy::Accessory` the app has no Dock presence,
    // so `set_focus` alone won't make the window jump in front of whatever app
    // is currently frontmost. Briefly switch the activation policy to Regular
    // so the app is allowed to become frontmost, then switch back.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }

    // Note: we deliberately don't flip back to Accessory here. The Dock icon
    // appears while the window is open (acceptable) and disappears again when
    // the window is hidden — see `on_window_event` in lib.rs.
}
