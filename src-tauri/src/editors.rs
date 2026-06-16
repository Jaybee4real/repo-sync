use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Editor {
    /// Stable id used when the frontend asks to open a path.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// `data:image/png;base64,…` of the app's real icon, if we could extract it.
    pub icon: Option<String>,
    /// True for the always-available "Reveal in Finder/Explorer" action, which
    /// is not a real editor and must not be used as the default.
    pub is_reveal: bool,
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

/// Locate a macOS .app bundle by name.
#[cfg(target_os = "macos")]
fn app_bundle(app: &str) -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from(format!("/Applications/{}.app", app))];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications").join(format!("{}.app", app)));
    }
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(not(target_os = "macos"))]
fn app_bundle(_app: &str) -> Option<PathBuf> {
    None
}

/// Find the .icns file inside an app bundle (prefers CFBundleIconFile, falls
/// back to the first .icns in Resources).
#[cfg(target_os = "macos")]
fn find_icns(bundle: &Path) -> Option<PathBuf> {
    let resources = bundle.join("Contents/Resources");
    // `defaults read` wants the plist path without the .plist extension.
    let info = bundle.join("Contents/Info");
    if let Ok(out) = Command::new("defaults")
        .arg("read")
        .arg(&info)
        .arg("CFBundleIconFile")
        .output()
    {
        if out.status.success() {
            let mut name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() {
                if !name.ends_with(".icns") {
                    name.push_str(".icns");
                }
                let p = resources.join(&name);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let mut icnss: Vec<PathBuf> = std::fs::read_dir(&resources)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "icns").unwrap_or(false))
        .collect();
    icnss.sort();
    icnss.into_iter().next()
}

/// Extract a 32px PNG of the app icon as a data URI, caching the PNG on disk.
#[cfg(target_os = "macos")]
fn icon_data_uri(id: &str, bundle: &Path) -> Option<String> {
    let cache_dir = dirs::data_dir()?.join("repo-sync").join("icons");
    let _ = std::fs::create_dir_all(&cache_dir);
    let png = cache_dir.join(format!("{}.png", id));

    if !png.exists() {
        let icns = find_icns(bundle)?;
        let out = Command::new("sips")
            .args(["-z", "32", "32", "-s", "format", "png"])
            .arg(&icns)
            .arg("--out")
            .arg(&png)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
    }

    let bytes = std::fs::read(&png).ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/png;base64,{}", b64))
}

#[cfg(not(target_os = "macos"))]
fn icon_data_uri(_id: &str, _bundle: &Path) -> Option<String> {
    None
}

/// Detect installed editors (with real icons), plus a "Reveal in Finder /
/// Explorer" action that is always available.
pub fn detect() -> Vec<Editor> {
    let mut out = Vec::new();
    for e in EDITORS {
        let bundle = e.mac_app.and_then(app_bundle);
        let cli_found = e.cli.iter().any(|c| cli_path(c).is_some());
        if cli_found || bundle.is_some() {
            let icon = bundle.as_deref().and_then(|b| icon_data_uri(e.id, b));
            out.push(Editor {
                id: e.id.to_string(),
                name: e.name.to_string(),
                icon,
                is_reveal: false,
            });
        }
    }

    // Reveal action, with the system file-manager icon where we can get it.
    let reveal_name = if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Reveal in Explorer"
    } else {
        "Open folder"
    };
    #[cfg(target_os = "macos")]
    let reveal_icon = icon_data_uri("reveal", Path::new("/System/Library/CoreServices/Finder.app"));
    #[cfg(not(target_os = "macos"))]
    let reveal_icon: Option<String> = None;

    out.push(Editor {
        id: "reveal".to_string(),
        name: reveal_name.to_string(),
        icon: reveal_icon,
        is_reveal: true,
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

    for c in def.cli {
        if cli_path(c).is_some() {
            return Command::new(c)
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(app) = def.mac_app {
        if app_bundle(app).is_some() {
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
