# repo-sync

A menu-bar app (macOS + Windows) that keeps a folder of git repositories fast-forwarded, on a daily schedule. Built with [Tauri 2](https://tauri.app) and Svelte 5.

## Why

I work across more than one machine. The pattern that kept burning me: sit down at the desk, open a repo I last touched on the laptop, start coding, and twenty minutes later realize I've been building on a branch that's three days stale. Now the fix involves a rebase I didn't need to have.

repo-sync makes that a non-event. Every morning it walks a folder of repos and fast-forwards every branch that can be fast-forwarded, safely, without ever merging anything. Whichever computer I sit down at, the code is already current. Even on a single machine it pays for itself: teammates' merges land while you sleep, and `git pull` stops being the first thing you type every morning.

It never rewrites anything. If a branch has diverged from its upstream, it's flagged in the dashboard and left alone for a human.

It works just as well for a backend engineer running a stack of services locally: point each repo at the branch it should track (`develop`, `main`, a release branch), have it pulled every morning before standup, and land back on your own feature branch afterwards. Repos you don't want touched can be switched off individually.

## What it does

- Scans a configured root folder for every git repo nested up to 5 levels deep.
- Fetches once per repo, then fast-forwards each local branch that is strictly behind its upstream. The checked-out branch gets `merge --ff-only`; the others are advanced with `branch -f` after verifying the move is a pure fast-forward.
- Stashes uncommitted tracked changes before touching anything and restores them after. If the restore conflicts, the repo is flagged and the stash is kept. (Or, per your choice, skips dirty repos entirely and leaves them for you.)
- Per-repo control: pick which repos are pulled, which branch each one checks out before pulling (a *target branch*, created as a tracking branch off the remote if it isn't local yet), which branch it lands on afterwards (a *fallback branch*), and which specific branches to fast-forward. Leave it all blank and a repo behaves exactly as before — every branch with an upstream, back on whatever was checked out.
- Runs daily at a time you pick (default 8:00), catches up on launch if the machine was asleep, and can be paused from the tray without quitting.
- Keeps per-day reports you can browse in the dashboard, pruned after a configurable number of days (default 90).
- Emits a per-repo progress event stream while a sync runs.
- Opens any repo in your editor straight from the dashboard. It detects VS Code, Cursor, Zed, Sublime, JetBrains IDEs and friends, with their real icons.
- Lives in the menu bar / system tray. Conflicts put a badge on the icon; the window stays out of your way until you ask for it.
- Starts at login.

## Install

Grab the latest `.dmg` (macOS) or `.msi` (Windows) from Releases, or build from source:

```bash
npm install
npm run tauri dev       # hot reload
npm run tauri build     # release: .app + .dmg on Mac, .msi + .exe on Windows
```

Releases are built by CI: push a tag like `v0.4.0` and `.github/workflows/release.yml` produces artifacts for macOS (arm64 + x86_64) and Windows as a draft GitHub Release.

## Safety model

The whole tool is built around one rule: only ever move a branch forward to something the remote already has.

- `--ff-only`, always. Nothing is merged, nothing is rebased, no commit is ever created or discarded.
- Diverged branches (local and remote both moved) are reported, never resolved automatically.
- Dirty working trees are stashed first. A clean restore drops the stash; a conflicted restore keeps it and flags the repo, so nothing you had in progress can be lost.
- Repos it can't handle are skipped with a reason, not forced.

## Hardening

Because this runs unattended against every repo on the machine, the git subprocess is boxed in:

- Every git call has a hard timeout. A fetch hung on a dead VPN or a credential prompt fails that repo and moves on; it can't wedge the whole sync.
- `GIT_TERMINAL_PROMPT=0` and ssh `BatchMode` mean git can never sit waiting for input a tray app has no way to provide.
- `core.fsmonitor` is forced off per invocation. A repo's own config can point fsmonitor at an arbitrary executable, which git would happily run; this app refuses to.
- The webview runs under a strict CSP, and the one filesystem-ish command the UI can call (open a repo in an editor) canonicalizes the path and rejects anything outside the configured root.
- Config coming from the UI is validated (schedule bounds, root must exist) before it's persisted.

## Architecture

- `src-tauri/src/` — Rust backend
  - `config.rs` — load/save/validate app config (JSON in the OS config dir)
  - `repos.rs` — scan + the pull engine (shells out to `git` with the hardening above)
  - `reports.rs` — daily report persistence + retention pruning
  - `scheduler.rs` — tokio task that fires at the configured time, with catch-up and pause
  - `state.rs` — shared app state
  - `commands.rs` — the Tauri commands exposed to the frontend
  - `tray.rs` — tray icon, menu, pause toggle, conflict badge
  - `editors.rs` — editor detection + launch
- `src/` — Svelte 5 UI (Repos / Reports / Settings tabs)

## File locations

- **Mac:** `~/Library/Application Support/repo-sync/` (reports) plus `~/Library/Application Support/repo-sync/config.json`
- **Windows:** `%APPDATA%\repo-sync\`

## Things it doesn't do

- It won't resolve a diverged branch for you. That's a feature.
- It doesn't push. Your unpushed work is your business.
- Private repos rely on whatever non-interactive auth you already have (ssh agent, credential helper). If a fetch needs a password typed, that repo fails that run and shows up in the report.
