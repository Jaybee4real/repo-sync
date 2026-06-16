use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Editor {
    /// Stable id used when the frontend asks to open a path.
    pub id: String,
    /// Human-readable name.
    pub name: String,
}

/// A known editor and how to detect / launch it.
struct EditorDef {
    id: &'static str,
    name: &'static str,
    /// CLI command names to look for on PATH (first match wins).
    cli: &'static [&'static str],
    /// macOS .app bundle names (checked in /Applications and ~/Applications).
    mac_app: Option<&'static str>,
}

const EDITORS: &[EditorDef] = &[
    EditorDef { id: "vscode", name: "VS Code", cli: &["code"], mac_app: Some("Visual Studio Code") },
    EditorDef { id: "cursor", name: "Cursor", cli: &["cursor"], mac_app: Some("Cursor") },
    EditorDef { id: "windsurf", name: "Windsurf", cli: &["windsurf"], mac_app: Some("Windsurf") },
    EditorDef { id: "antigravity", name: "Antigravity", cli: &["antigravity"], mac_app: Some("Antigravity") },
    EditorDef { id: "zed", name: "Zed", cli: &["zed"], mac_app: Some("Zed") },
    EditorDef { id: "sublime", name: "Sublime Text", cli: &["subl"], mac_app: Some("Sublime Text") },
    EditorDef { id: "intellij", name: "IntelliJ IDEA", cli: &["idea"], mac_app: Some("IntelliJ IDEA") },
    EditorDef { id: "webstorm", name: "WebStorm", cli: &["webstorm"], mac_app: Some("WebStorm") },
    EditorDef { id: "android-studio", name: "Android Studio", cli: &[], mac_app: Some("Android Studio") },
];

fn cli_path(cmd: &str) -> Option<String> {
    let which = if cfg!(target_os = "windows") { "where" } else { "which" };
    let out = Command::new(which).arg(cmd).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

#[cfg(target_os = "macos")]
fn mac_app_exists(app: &str) -> bool {
    let candidates = [
        format!("/Applications/{}.app", app),
        format!(
            "{}/Applications/{}.app",
            dirs::home_dir().map(|p| p.display().to_string()).unwrap_or_default(),
            app
        ),
    ];
    candidates.iter().any(|p| Path::new(p).exists())
}

#[cfg(not(target_os = "macos"))]
fn mac_app_exists(_app: &str) -> bool {
    false
}

/// Detect installed editors, plus a "Reveal in Finder/Explorer" entry that is
/// always available.
pub fn detect() -> Vec<Editor> {
    let mut out = Vec::new();
    for e in EDITORS {
        let found = e.cli.iter().any(|c| cli_path(c).is_some())
            || e.mac_app.map(mac_app_exists).unwrap_or(false);
        if found {
            out.push(Editor {
                id: e.id.to_string(),
                name: e.name.to_string(),
            });
        }
    }
    out.push(Editor {
        id: "reveal".to_string(),
        name: if cfg!(target_os = "macos") {
            "Reveal in Finder".to_string()
        } else if cfg!(target_os = "windows") {
            "Reveal in Explorer".to_string()
        } else {
            "Open folder".to_string()
        },
    });
    out
}

/// Open `path` in the editor identified by `id`.
pub fn open_in(id: &str, path: &str) -> Result<(), String> {
    if !Path::new(path).exists() {
        return Err(format!("path does not exist: {}", path));
    }

    if id == "reveal" {
        return reveal(path);
    }

    let def = EDITORS
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown editor: {}", id))?;

    // Prefer a CLI launcher if present.
    for c in def.cli {
        if cli_path(c).is_some() {
            return Command::new(c)
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
    }

    // Fall back to the macOS app bundle.
    #[cfg(target_os = "macos")]
    if let Some(app) = def.mac_app {
        if mac_app_exists(app) {
            return Command::new("open")
                .args(["-a", app, path])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
    }

    Err(format!("{} is not available", def.name))
}

fn reveal(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let cmd = Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let cmd = Command::new("explorer").arg(path).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let cmd = Command::new("xdg-open").arg(path).spawn();

    cmd.map(|_| ()).map_err(|e| e.to_string())
}
