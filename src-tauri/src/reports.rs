use crate::repos::SyncReport;
use std::path::PathBuf;

/// Where reports are saved. Lives in app-data dir so it survives uninstall-then-reinstall.
pub fn reports_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("repo-sync").join("reports")
}

pub fn save_report(report: &SyncReport) -> anyhow::Result<PathBuf> {
    let dir = reports_dir();
    std::fs::create_dir_all(&dir)?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = dir.join(format!("{}.json", date));
    // If a report exists for today, merge by appending results under a list.
    // Keep it simple: overwrite — UI shows latest.
    std::fs::write(&path, serde_json::to_string_pretty(report)?)?;
    Ok(path)
}

pub fn load_report(date: &str) -> Option<SyncReport> {
    let path = reports_dir().join(format!("{}.json", date));
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn list_reports() -> Vec<String> {
    let dir = reports_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".json") {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out.reverse();
    out
}
