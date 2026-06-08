# repo-sync

A menu-bar app (macOS + Windows) that scans a folder of git repositories and runs
`git pull` on each one, on a daily schedule. Built with [Tauri 2](https://tauri.app)
+ Svelte 5.

## What it does

- Scans a configured root folder (default: `~/Documents/Programming-Codes`) for
  every git repo nested up to 5 levels deep.
- Runs `git pull --ff-only` in each enabled repo at a configurable time (default
  8:00 local) every day.
- Catches up automatically if the Mac was asleep at the scheduled time, or if
  more than 24 hours have passed since the last run.
- Persists per-day reports you can browse in the dashboard.
- Lives in the menu bar / system tray — the main window is hidden until you
  open it via the tray menu or by double-clicking the icon.
- Per-repo enable/disable from the Repos tab.
- Auto-starts at login (via `tauri-plugin-autostart`).

## Development

```bash
npm install
npm run tauri dev       # hot reload
npm run tauri build     # release: .app + .dmg on Mac, .msi + .exe on Windows
```

## Release builds (CI)

Push a tag like `v0.1.0` to trigger `.github/workflows/release.yml`, which builds
artifacts for Mac (arm64 + x86_64) and Windows on their native runners and
uploads them as a draft GitHub Release.

## Architecture

- `src-tauri/src/` — Rust backend
  - `config.rs` — load/save app config (JSON in OS config dir)
  - `repos.rs` — scan + git pull (shells out to `git`)
  - `reports.rs` — daily report persistence
  - `scheduler.rs` — tokio task that fires at the configured time + catch-up
  - `state.rs` — shared app state (config, last report, syncing flag)
  - `commands.rs` — Tauri commands exposed to the frontend
  - `tray.rs` — menu-bar / tray icon and menu
  - `lib.rs` — Tauri app entrypoint
- `src/` — Svelte 5 UI
  - `lib/types.ts` — TypeScript mirror of Rust types
  - `lib/api.ts` — `invoke` wrappers
  - `routes/+page.svelte` — main dashboard (Repos / Reports / Settings tabs)

## File locations

Config + reports live under the OS app-data dir:
- **Mac:** `~/Library/Application Support/repo-sync/`
- **Windows:** `%APPDATA%\repo-sync\`
