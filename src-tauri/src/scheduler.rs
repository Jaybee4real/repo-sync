use crate::state::AppState;
use chrono::{Duration, Local, NaiveTime, TimeZone, Timelike};
use tauri::{AppHandle, Manager};
use tokio::time::sleep;

/// Cap on how long the scheduler will sleep at a stretch. A short cap means
/// changes the user makes to `schedule_hour` / `schedule_minute` in Settings
/// take effect within this window, rather than only on the next fire (which
/// could be 24 hours away).
const MAX_SLEEP_SECS: u64 = 5 * 60;

/// Start the background scheduler. Wakes at the configured time daily and
/// triggers a sync. Also performs a catch-up sync at startup if `last_run`
/// was more than 24h ago.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Catch-up: if last run was more than 24h ago (or never), sync at startup
        // — but wait 10 seconds first so the UI has a moment to come up.
        sleep(std::time::Duration::from_secs(10)).await;
        if should_catch_up(&app) {
            run_sync(&app).await;
        }

        loop {
            let target = next_target(&app);
            // Sleep in short hops so config changes get noticed quickly.
            loop {
                let now = Local::now();
                if now >= target {
                    break;
                }
                let remaining = (target - now).num_seconds().max(1) as u64;
                let nap = remaining.min(MAX_SLEEP_SECS);
                sleep(std::time::Duration::from_secs(nap)).await;

                // If the schedule moved while we were sleeping, restart the
                // outer loop to recompute.
                if next_target(&app) != target {
                    break;
                }
            }
            // Guard against double-firing if config moved the target into the
            // future after we already passed it.
            if Local::now() >= target {
                run_sync(&app).await;
                // After running, sleep a minute so we don't immediately re-fire
                // on the same minute boundary.
                sleep(std::time::Duration::from_secs(60)).await;
            }
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

/// Next scheduled fire time, in local time.
fn next_target(app: &AppHandle) -> chrono::DateTime<Local> {
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
    if target_today <= now {
        target_today + Duration::days(1)
    } else {
        target_today
    }
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
