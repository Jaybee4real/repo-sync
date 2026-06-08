use crate::config::AppConfig;
use crate::repos::SyncReport;
use std::sync::Mutex;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    /// Last completed sync report, for the tray menu and UI.
    pub last_report: Mutex<Option<SyncReport>>,
    /// True while a sync is currently running (prevents double-trigger).
    pub syncing: Mutex<bool>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
            last_report: Mutex::new(None),
            syncing: Mutex::new(false),
        }
    }
}
