use crate::state::AppState;
use chrono::{Duration, Local, NaiveTime, TimeZone, Timelike};
use tauri::{AppHandle, Manager};
use tokio::time::sleep;

/// Start the background scheduler. Wakes at the configured time daily and
/// triggers a sync. Also performs a catch-up sync at startup if `last_run`
/// was more than 24h ago.
pub fn start(app: AppHandle) {
    tokio::spawn(async move {
        // Catch-up: if last run was more than 24h ago (or never), sync at startup
        // — but wait 10 seconds first so the UI has a moment to come up.
        sleep(std::time::Duration::from_secs(10)).await;
        if should_catch_up(&app) {
            run_sync(&app).await;
        }

        loop {
            let wait = wait_until_next_run(&app);
            sleep(wait).await;
            run_sync(&app).await;
        }
    });
}

fn should_catch_up(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap();
    match &cfg.last_run {
        None => true,
        Some(ts) => {
            let parsed = chrono::DateTime::parse_from_rfc3339(ts).ok();
            match parsed {
                Some(t) => (Local::now() - t.with_timezone(&Local)) > Duration::hours(24),
                None => true,
            }
        }
    }
}

fn wait_until_next_run(app: &AppHandle) -> std::time::Duration {
    let (h, m) = {
        let state = app.state::<AppState>();
        let cfg = state.config.lock().unwrap();
        (cfg.schedule_hour, cfg.schedule_minute)
    };
    let now = Local::now();
    let target_today = Local
        .from_local_datetime(
            &now.date_naive()
                .and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default()),
        )
        .single()
        .unwrap_or(now);
    let target = if target_today <= now {
        target_today + Duration::days(1)
    } else {
        target_today
    };
    let delta = target - now;
    std::time::Duration::from_secs(delta.num_seconds().max(60) as u64)
}

async fn run_sync(app: &AppHandle) {
    let _ = crate::commands::sync_now(app.clone()).await;
}

/// Helper for the tray "Last sync: …" label.
pub fn last_run_label(last_run: &Option<String>) -> String {
    match last_run {
        None => "Never synced".to_string(),
        Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
            Ok(t) => {
                let local = t.with_timezone(&Local);
                let now = Local::now();
                if local.date_naive() == now.date_naive() {
                    format!("Last sync: today {:02}:{:02}", local.hour(), local.minute())
                } else {
                    format!("Last sync: {}", local.format("%b %d %H:%M"))
                }
            }
            Err(_) => format!("Last sync: {}", ts),
        },
    }
}
