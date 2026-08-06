<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "$lib/api";
  import type {
    AppConfig,
    RepoInfo,
    RepoSettings,
    SyncReport,
    PullResult,
    BranchStatus,
    Editor,
  } from "$lib/types";
  import { hasConflict, updatedCount } from "$lib/types";

  let config = $state<AppConfig | null>(null);
  let repos = $state<RepoInfo[]>([]);
  let lastReport = $state<SyncReport | null>(null);
  let availableReports = $state<string[]>([]);
  let selectedReportDate = $state<string>("");
  let viewingReport = $state<SyncReport | null>(null);
  let editors = $state<Editor[]>([]);
  let selectedEditor = $state<string>("");
  let menuOpen = $state<string | null>(null);
  let tab = $state<"repos" | "conflicts" | "report" | "settings">("repos");
  let syncing = $state(false);
  let toast = $state<string>("");

  let unlistenStarted: UnlistenFn | null = null;
  let unlistenFinished: UnlistenFn | null = null;
  let unlistenFailed: UnlistenFn | null = null;
  let onVisible: (() => void) | null = null;

  let conflicts = $derived(
    (lastReport?.results ?? []).filter((r) => hasConflict(r)),
  );

  onMount(async () => {
    await refresh();
    editors = await api.listEditors();
    // Restore the last-used editor; default to Windsurf, else the first real
    // editor. "Reveal in Finder" is never a default (it isn't an editor).
    const real = editors.filter((e) => !e.is_reveal);
    const stored = localStorage.getItem("repo-sync.editor");
    if (stored && real.some((e) => e.id === stored)) {
      selectedEditor = stored;
    } else {
      selectedEditor =
        (real.find((e) => e.id === "windsurf") ?? real[0])?.id ?? "";
    }
    unlistenStarted = await listen("sync-started", () => {
      syncing = true;
      toast = "Syncing…";
    });
    unlistenFinished = await listen<SyncReport>("sync-finished", async (e) => {
      syncing = false;
      lastReport = e.payload;
      config = await api.getConfig();
      const s = summary(e.payload);
      toast = `Sync done · ${s.updated} updated · ${s.clean} ok · ${s.skipped} skipped · ${s.failed} failed · ${s.conflict} conflict`;
      await refreshReports();
    });
    unlistenFailed = await listen<string>("sync-failed", (e) => {
      syncing = false;
      toast = `Sync failed: ${e.payload}`;
    });

    onVisible = () => {
      if (document.visibilityState === "visible") refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
  });

  onDestroy(() => {
    unlistenStarted?.();
    unlistenFinished?.();
    unlistenFailed?.();
    if (onVisible) {
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
    }
  });

  async function refresh() {
    config = await api.getConfig();
    repos = await api.scanRepos();
    lastReport = await api.getLastReport();
    await refreshReports();
  }

  async function refreshReports() {
    availableReports = await api.listReports();
    if (availableReports.length > 0 && !selectedReportDate) {
      selectedReportDate = availableReports[0];
    }
    if (selectedReportDate) {
      viewingReport = await api.loadReport(selectedReportDate);
    }
  }

  async function onSyncNow() {
    if (syncing) return;
    try {
      await api.syncNow();
    } catch (e) {
      toast = `Error: ${e}`;
      syncing = false;
    }
  }

  let expandedRepo = $state<string | null>(null);

  function settingsFor(rel: string): RepoSettings {
    const existing = config?.repo_settings?.[rel];
    if (existing) return existing;
    const enabled = config?.repo_enabled?.[rel] ?? true;
    return { enabled, branches: [], target_branch: null, fallback_branch: null };
  }

  async function updateRepo(rel: string, patch: Partial<RepoSettings>) {
    if (!config) return;
    const next = { ...settingsFor(rel), ...patch };
    config.repo_settings = { ...config.repo_settings, [rel]: next };
    await api.setConfig(config);
  }

  async function toggleRepo(rel: string, on: boolean) {
    await updateRepo(rel, { enabled: on });
  }

  function isEnabled(rel: string): boolean {
    return settingsFor(rel).enabled;
  }

  /** Parse a comma/space/newline-separated branch list into a clean array. */
  function parseBranches(text: string): string[] {
    return text
      .split(/[\s,]+/)
      .map((b) => b.trim())
      .filter(Boolean);
  }

  async function saveSettings() {
    if (!config) return;
    await api.setConfig(config);
    toast = "Settings saved";
  }

  async function onPickReport(date: string) {
    selectedReportDate = date;
    viewingReport = await api.loadReport(date);
  }

  async function openIn(editorId: string, path: string) {
    try {
      await api.openInEditor(editorId, path);
    } catch (e) {
      toast = `Couldn't open: ${e}`;
    }
  }

  let realEditors = $derived(editors.filter((e) => !e.is_reveal));
  let revealEditor = $derived(editors.find((e) => e.is_reveal) ?? null);

  function selectedEditorObj() {
    return editors.find((e) => e.id === selectedEditor) ?? null;
  }
  function selectedEditorName(): string {
    return selectedEditorObj()?.name ?? "editor";
  }

  function toggleMenu(key: string) {
    menuOpen = menuOpen === key ? null : key;
  }

  // Pick an editor from the dropdown: remember it as the new default and open
  // the repo in it right away.
  function chooseEditor(editorId: string, path: string) {
    selectedEditor = editorId;
    localStorage.setItem("repo-sync.editor", editorId);
    menuOpen = null;
    openIn(editorId, path);
  }

  // Reveal is a one-off action — open the folder without changing the default.
  function revealOnly(path: string) {
    menuOpen = null;
    openIn("reveal", path);
  }

  interface Summary {
    updated: number;
    clean: number;
    skipped: number;
    failed: number;
    conflict: number;
  }
  function summary(r: SyncReport): Summary {
    let s: Summary = { updated: 0, clean: 0, skipped: 0, failed: 0, conflict: 0 };
    for (const x of r.results) {
      if (x.error) s.failed++;
      else if (x.skipped) s.skipped++;
      else if (hasConflict(x)) s.conflict++;
      else if (updatedCount(x) > 0) s.updated++;
      else s.clean++;
    }
    return s;
  }

  // One-line status for a repo in the Repos table.
  function repoStatusIcon(r: PullResult | null): string {
    if (!r) return "—";
    if (r.error) return "❌";
    if (r.skipped) return "⏭️";
    if (hasConflict(r)) return "⚠️";
    if (updatedCount(r) > 0) return "🔄";
    return "✅";
  }

  function repoStatusText(r: PullResult | null): string {
    if (!r) return "";
    if (r.error) return r.error;
    if (r.skipped) return `skipped — ${r.skipped}`;
    const ff = updatedCount(r);
    const diverged = r.branches.filter((b) => b.status.kind === "diverged").length;
    const parts: string[] = [];
    if (ff > 0) parts.push(`${ff} branch${ff === 1 ? "" : "es"} updated`);
    if (r.stash_conflict) parts.push("stash conflict");
    if (diverged > 0) parts.push(`${diverged} diverged`);
    if (parts.length === 0) return "up to date";
    return parts.join(" · ");
  }

  function branchLabel(s: BranchStatus): string {
    switch (s.kind) {
      case "up_to_date": return "up to date";
      case "fast_forwarded": return `+${s.commits} commit${s.commits === 1 ? "" : "s"}`;
      case "diverged": return `diverged (↑${s.ahead} ↓${s.behind})`;
      case "no_upstream": return "no upstream";
    }
  }

  function branchIcon(s: BranchStatus): string {
    switch (s.kind) {
      case "up_to_date": return "✅";
      case "fast_forwarded": return "🔄";
      case "diverged": return "⚠️";
      case "no_upstream": return "·";
    }
  }

  function formatTime(iso: string | null): string {
    if (!iso) return "never";
    return new Date(iso).toLocaleString();
  }

  function lastResultFor(rel: string): PullResult | null {
    return lastReport?.results.find((x) => x.repo.rel_path === rel) ?? null;
  }

  $effect(() => {
    if (toast) {
      const t = setTimeout(() => (toast = ""), 4500);
      return () => clearTimeout(t);
    }
  });
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="dot" class:syncing class:warn={conflicts.length > 0}></span>
      <h1>repo-sync</h1>
      <span class="muted">{config?.root ?? ""}</span>
    </div>
    <div class="actions">
      <span class="muted last-sync">Last sync: {formatTime(config?.last_run ?? null)}</span>
      <button class="primary" onclick={onSyncNow} disabled={syncing}>
        {syncing ? "Syncing…" : "Sync now"}
      </button>
    </div>
  </header>

  <nav class="tabs">
    <button class:active={tab === "repos"} onclick={() => (tab = "repos")}>
      Repos <span class="count">{repos.length}</span>
    </button>
    <button class:active={tab === "conflicts"} onclick={() => (tab = "conflicts")}>
      Conflicts
      {#if conflicts.length > 0}<span class="count warn">{conflicts.length}</span>{/if}
    </button>
    <button class:active={tab === "report"} onclick={() => (tab = "report")}>Reports</button>
    <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>Settings</button>
  </nav>

  {#if tab === "repos"}
    <section class="panel">
      {#if repos.length === 0}
        <p class="empty">No git repos found under <code>{config?.root}</code>. Check Settings.</p>
      {:else}
        <table>
          <thead>
            <tr><th></th><th>Repo</th><th>Branch</th><th>Last result</th></tr>
          </thead>
          <tbody>
            {#each repos as repo (repo.rel_path)}
              {@const last = lastResultFor(repo.rel_path)}
              {@const s = settingsFor(repo.rel_path)}
              <tr class:disabled={!isEnabled(repo.rel_path)}>
                <td>
                  <input type="checkbox" checked={isEnabled(repo.rel_path)}
                    onchange={(e) => toggleRepo(repo.rel_path, (e.target as HTMLInputElement).checked)} />
                </td>
                <td>
                  <code>{repo.rel_path}</code>
                  {#if !repo.has_remote}<span class="badge muted">no remote</span>{/if}
                  {#if s.target_branch || s.fallback_branch || s.branches.length}
                    <span class="badge">custom</span>
                  {/if}
                  <button class="linklike"
                    onclick={() => (expandedRepo = expandedRepo === repo.rel_path ? null : repo.rel_path)}>
                    {expandedRepo === repo.rel_path ? "▾" : "▸"} branches
                  </button>
                </td>
                <td><span class="branch">{repo.branch}</span></td>
                <td>
                  <span class="status">{repoStatusIcon(last)}</span>
                  <span class="status-text" class:bad={last && (last.error || hasConflict(last))}>
                    {repoStatusText(last)}
                  </span>
                </td>
              </tr>
              {#if expandedRepo === repo.rel_path}
                <tr class="repo-config">
                  <td></td>
                  <td colspan="3">
                    <div class="repo-config-grid">
                      <label>
                        <span>Target branch <small>checked out before pulling</small></span>
                        <input type="text" placeholder="e.g. develop" value={s.target_branch ?? ""}
                          onchange={(e) => updateRepo(repo.rel_path, { target_branch: (e.target as HTMLInputElement).value.trim() || null })} />
                      </label>
                      <label>
                        <span>Fall back to <small>branch to land on after</small></span>
                        <input type="text" placeholder="your working branch" value={s.fallback_branch ?? ""}
                          onchange={(e) => updateRepo(repo.rel_path, { fallback_branch: (e.target as HTMLInputElement).value.trim() || null })} />
                      </label>
                      <label class="wide">
                        <span>Only pull these branches <small>comma-separated; empty = all</small></span>
                        <input type="text" placeholder="e.g. develop, main" value={s.branches.join(", ")}
                          onchange={(e) => updateRepo(repo.rel_path, { branches: parseBranches((e.target as HTMLInputElement).value) })} />
                      </label>
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {/if}

  {#if tab === "conflicts"}
    <section class="panel">
      {#if conflicts.length === 0}
        <p class="empty">🎉 No conflicts. Every branch fast-forwarded cleanly.</p>
      {:else}
        <p class="hint">
          These repos couldn't be updated cleanly — a branch diverged, or restoring your
          stashed local changes left conflict markers. Open one to resolve it.
        </p>
        {#each conflicts as r (r.repo.rel_path)}
          <div class="conflict-card">
            <div class="conflict-head">
              <div class="conflict-title">
                <code class="conflict-name">{r.repo.rel_path}</code>
                <span class="branch">on {r.original_branch}</span>
              </div>
              {#if realEditors.length > 0}
                {@const sel = selectedEditorObj()}
                <div class="split-btn">
                  <button class="split-main" onclick={() => openIn(selectedEditor, r.repo.abs_path)}>
                    {#if sel?.icon}<img class="ed-icon" src={sel.icon} alt="" />{/if}
                    Open in {selectedEditorName()}
                  </button>
                  <button
                    class="split-caret"
                    class:active={menuOpen === r.repo.rel_path}
                    aria-label="Choose editor"
                    onclick={() => toggleMenu(r.repo.rel_path)}
                  >▾</button>
                  {#if menuOpen === r.repo.rel_path}
                    <div class="split-menu">
                      {#each realEditors as ed}
                        <button
                          class="split-item"
                          class:current={ed.id === selectedEditor}
                          onclick={() => chooseEditor(ed.id, r.repo.abs_path)}
                        >
                          <span class="item-left">
                            {#if ed.icon}<img class="ed-icon" src={ed.icon} alt="" />{:else}<span class="ed-icon ed-fallback"></span>{/if}
                            {ed.name}
                          </span>
                          {#if ed.id === selectedEditor}<span class="check">✓</span>{/if}
                        </button>
                      {/each}
                      {#if revealEditor}
                        <div class="split-sep"></div>
                        <button class="split-item" onclick={() => revealOnly(r.repo.abs_path)}>
                          <span class="item-left">
                            {#if revealEditor.icon}<img class="ed-icon" src={revealEditor.icon} alt="" />{:else}<span class="ed-icon ed-fallback"></span>{/if}
                            {revealEditor.name}
                          </span>
                        </button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
            <ul class="conflict-reasons">
              {#if r.stash_conflict}
                <li>⚠️ Restoring your stashed local changes conflicted — resolve the markers in <code>{r.original_branch}</code>. Your changes are still saved in <code>git stash</code>.</li>
              {/if}
              {#each r.branches.filter((b) => b.status.kind === "diverged") as b}
                <li>⚠️ <span class="branch">{b.branch}</span> {branchLabel(b.status)} — has local commits the remote doesn't. Needs a manual merge or rebase.</li>
              {/each}
            </ul>
          </div>
        {/each}
        {#if menuOpen}
          <button class="menu-backdrop" aria-label="Close menu" onclick={() => (menuOpen = null)}></button>
        {/if}
      {/if}
    </section>
  {/if}

  {#if tab === "report"}
    <section class="panel">
      {#if availableReports.length === 0}
        <p class="empty">No reports yet. Click <strong>Sync now</strong>.</p>
      {:else}
        <div class="report-header">
          <label>
            Date:
            <select value={selectedReportDate} onchange={(e) => onPickReport((e.target as HTMLSelectElement).value)}>
              {#each availableReports as d}<option value={d}>{d}</option>{/each}
            </select>
          </label>
          {#if viewingReport}
            {@const s = summary(viewingReport)}
            <div class="summary">
              <span class="chip chip-updated">{s.updated} updated</span>
              <span class="chip chip-ok">{s.clean} up to date</span>
              <span class="chip chip-skipped">{s.skipped} skipped</span>
              <span class="chip chip-failed">{s.failed} failed</span>
              {#if s.conflict > 0}<span class="chip chip-conflict">{s.conflict} conflict</span>{/if}
            </div>
          {/if}
        </div>

        {#if viewingReport}
          {#each viewingReport.results as r (r.repo.rel_path)}
            <div class="report-repo">
              <div class="report-repo-head">
                <span class="status">{repoStatusIcon(r)}</span>
                <code>{r.repo.rel_path}</code>
                {#if r.error}<span class="status-text bad">{r.error}</span>
                {:else if r.skipped}<span class="muted">skipped — {r.skipped}</span>{/if}
              </div>
              {#if !r.error && !r.skipped && r.branches.length > 0}
                <ul class="branch-list">
                  {#each r.branches as b}
                    <li class:bad={b.status.kind === "diverged"}>
                      <span class="branch-icon">{branchIcon(b.status)}</span>
                      <span class="branch">{b.branch}</span>
                      <span class="branch-status">{branchLabel(b.status)}</span>
                    </li>
                  {/each}
                  {#if r.stash_conflict}
                    <li class="bad"><span class="branch-icon">⚠️</span> stashed local changes conflicted on restore</li>
                  {/if}
                </ul>
              {/if}
            </div>
          {/each}
        {/if}
      {/if}
    </section>
  {/if}

  {#if tab === "settings"}
    <section class="panel settings">
      {#if config}
        <label>
          <span>Root folder</span>
          <input type="text" bind:value={config.root} />
          <small>Folder to scan for git repos (recursively, up to 5 levels deep).</small>
        </label>
        <div class="row">
          <label><span>Daily run hour</span><input type="number" min="0" max="23" bind:value={config.schedule_hour} /></label>
          <label><span>Minute</span><input type="number" min="0" max="59" bind:value={config.schedule_minute} /></label>
        </div>
        <label>
          <span>When a repo has uncommitted changes</span>
          <select bind:value={config.on_dirty}>
            <option value="stash">Stash, pull, then restore (never loses work)</option>
            <option value="skip">Skip the repo, leave my changes untouched</option>
          </select>
          <small>Applies to every repo during the pull. Per-repo target/fallback branches are set on the Repos tab.</small>
        </label>
        <label class="checkbox">
          <input type="checkbox" bind:checked={config.notify_on_finish} />
          <span>Show notification when sync finishes</span>
        </label>
        <label class="checkbox">
          <input type="checkbox" bind:checked={config.paused} />
          <span>Pause scheduled syncs (manual "Sync now" still works)</span>
        </label>
        <label>
          <span>Keep reports for (days)</span>
          <input type="number" min="1" max="3650" bind:value={config.keep_reports_days} />
          <small>Older daily reports are deleted after each sync.</small>
        </label>
        <button class="primary" onclick={saveSettings}>Save settings</button>

        {#if realEditors.length > 0}
          <div class="detected">
            <span class="muted">Detected editors:</span>
            {#each realEditors as ed}
              <span class="badge badge-ed">
                {#if ed.icon}<img class="ed-icon" src={ed.icon} alt="" />{/if}
                {ed.name}
              </span>
            {/each}
          </div>
        {/if}
      {/if}
    </section>
  {/if}

  {#if toast}<div class="toast">{toast}</div>{/if}
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif;
    background: #fafafa; color: #1a1a1a;
  }
  @media (prefers-color-scheme: dark) { :global(body) { background: #1a1a1a; color: #e8e8e8; } }

  .app { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }

  header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 20px; border-bottom: 1px solid #e2e2e2; background: #fff;
  }
  @media (prefers-color-scheme: dark) { header { background: #232323; border-bottom-color: #333; } }
  .brand { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .brand h1 { margin: 0; font-size: 16px; font-weight: 600; }
  .muted { color: #888; font-size: 12px; }
  .last-sync { margin-right: 8px; }

  .dot { width: 10px; height: 10px; border-radius: 50%; background: #36c75a; box-shadow: 0 0 6px #36c75a; }
  .dot.syncing { background: #f5a623; box-shadow: 0 0 6px #f5a623; animation: pulse 1s infinite alternate; }
  .dot.warn { background: #e0483d; box-shadow: 0 0 6px #e0483d; }
  @keyframes pulse { from { opacity: 0.6; } to { opacity: 1; } }

  button {
    background: #f0f0f0; border: 1px solid #d0d0d0; border-radius: 6px;
    padding: 6px 12px; font-size: 13px; cursor: pointer; color: inherit;
  }
  button:hover { background: #e8e8e8; }
  button.primary { background: #2469e6; border-color: #2469e6; color: white; }
  button.primary:hover { background: #1f5fd0; }
  button.primary:disabled { opacity: 0.6; cursor: not-allowed; }
  @media (prefers-color-scheme: dark) {
    button { background: #2d2d2d; border-color: #404040; color: #e8e8e8; }
    button:hover { background: #3a3a3a; }
  }

  .tabs {
    display: flex; gap: 4px; padding: 8px 16px; background: #fff; border-bottom: 1px solid #e2e2e2;
  }
  @media (prefers-color-scheme: dark) { .tabs { background: #232323; border-bottom-color: #333; } }
  .tabs button { background: transparent; border: none; padding: 6px 12px; color: #666; font-weight: 500; }
  .tabs button.active { color: #2469e6; border-bottom: 2px solid #2469e6; border-radius: 0; }
  @media (prefers-color-scheme: dark) { .tabs button { color: #aaa; } .tabs button.active { color: #6ba1ff; border-bottom-color: #6ba1ff; } }
  .count { display: inline-block; background: #eee; color: #555; padding: 1px 7px; border-radius: 10px; font-size: 11px; margin-left: 4px; }
  .count.warn { background: #fde3e3; color: #b32626; }
  @media (prefers-color-scheme: dark) { .count { background: #3a3a3a; color: #ccc; } .count.warn { background: #4d1d1d; color: #ff9b9b; } }

  .panel { flex: 1; overflow-y: auto; padding: 16px 20px; }

  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 8px 10px; font-size: 13px; border-bottom: 1px solid #ececec; }
  th { font-weight: 500; color: #666; font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; }
  @media (prefers-color-scheme: dark) { th, td { border-bottom-color: #2e2e2e; } th { color: #999; } }
  tr.disabled { opacity: 0.45; }

  code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12.5px; }
  .branch { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; color: #888; }
  .badge { display: inline-block; padding: 1px 6px; border-radius: 4px; background: #eee; font-size: 11px; margin-left: 6px; }
  @media (prefers-color-scheme: dark) { .badge { background: #333; } }
  .linklike { background: none; border: none; color: #0a7; font-size: 11px; cursor: pointer; padding: 0 4px; margin-left: 4px; }
  .linklike:hover { text-decoration: underline; }
  .repo-config td { background: rgba(0, 0, 0, 0.03); }
  @media (prefers-color-scheme: dark) { .repo-config td { background: rgba(255, 255, 255, 0.04); } }
  .repo-config-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px 14px; padding: 8px 4px 12px; }
  .repo-config-grid label { display: flex; flex-direction: column; gap: 3px; }
  .repo-config-grid label.wide { grid-column: 1 / -1; }
  .repo-config-grid span { font-size: 12px; font-weight: 500; }
  .repo-config-grid small { font-weight: 400; color: #888; }
  .repo-config-grid input { padding: 5px 8px; }

  .status { width: 22px; display: inline-block; }
  .status-text { font-size: 12.5px; color: #666; }
  .status-text.bad { color: #d33; }
  @media (prefers-color-scheme: dark) { .status-text { color: #aaa; } }

  .hint { font-size: 13px; color: #777; margin: 0 0 14px; }

  .conflict-card { border: 1px solid #e0483d; background: transparent; border-radius: 8px; padding: 12px 14px; margin-bottom: 12px; }
  @media (prefers-color-scheme: dark) { .conflict-card { border-color: #b5453b; } }
  /* Title left, action far right; the action wraps under only when cramped. */
  .conflict-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px 16px; flex-wrap: wrap; }
  .conflict-title { min-width: 0; flex: 1 1 auto; }
  .conflict-name { font-size: 13.5px; font-weight: 600; margin-right: 8px; }

  /* Split button: primary opens last-used editor; caret opens the picker. */
  .split-btn { position: relative; display: inline-flex; flex: 0 0 auto; margin-left: auto; }
  .split-main, .split-caret {
    border: 1px solid #d0d0d0; background: #fff; color: inherit;
    font-size: 12.5px; padding: 5px 12px; cursor: pointer;
  }
  .split-main { display: inline-flex; align-items: center; gap: 7px; }
  .ed-icon { width: 16px; height: 16px; border-radius: 3px; object-fit: contain; flex: 0 0 auto; }
  .ed-fallback { background: #c8c8c8; border-radius: 4px; }
  @media (prefers-color-scheme: dark) { .ed-fallback { background: #555; } }
  .item-left { display: inline-flex; align-items: center; gap: 9px; }
  .split-sep { height: 1px; background: #e2e2e2; margin: 4px 0; }
  @media (prefers-color-scheme: dark) { .split-sep { background: #444; } }
  .split-main { border-radius: 6px 0 0 6px; }
  .split-caret { border-left: none; border-radius: 0 6px 6px 0; padding: 5px 9px; font-size: 11px; }
  .split-main:hover, .split-caret:hover, .split-caret.active { background: #f0f0f0; }
  @media (prefers-color-scheme: dark) {
    .split-main, .split-caret { background: #2d2d2d; border-color: #474747; }
    .split-main:hover, .split-caret:hover, .split-caret.active { background: #3a3a3a; }
  }
  .split-menu {
    position: absolute; top: calc(100% + 4px); right: 0; z-index: 20;
    min-width: 170px; background: #fff; border: 1px solid #d0d0d0; border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0,0,0,0.16); overflow: hidden; padding: 4px;
  }
  @media (prefers-color-scheme: dark) { .split-menu { background: #2a2a2a; border-color: #474747; } }
  .split-item {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    width: 100%; text-align: left; border: none; background: transparent; color: inherit;
    padding: 7px 10px; font-size: 12.5px; border-radius: 5px; cursor: pointer;
  }
  .split-item:hover { background: #eef3ff; }
  .split-item.current { font-weight: 600; }
  .split-item .check { color: #2469e6; }
  @media (prefers-color-scheme: dark) { .split-item:hover { background: #33405e; } .split-item .check { color: #6ba1ff; } }
  .menu-backdrop { position: fixed; inset: 0; z-index: 15; background: transparent; border: none; padding: 0; cursor: default; }

  .conflict-reasons { margin: 10px 0 0; padding-left: 4px; list-style: none; }
  .conflict-reasons li { font-size: 12.5px; margin-bottom: 6px; line-height: 1.5; }

  .report-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; gap: 16px; flex-wrap: wrap; }
  .report-header label { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .report-header select { border: 1px solid #d0d0d0; border-radius: 6px; padding: 4px 8px; font-size: 13px; background: white; color: inherit; }
  @media (prefers-color-scheme: dark) { .report-header select { background: #2d2d2d; border-color: #404040; color: #e8e8e8; } }

  .summary { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { padding: 3px 10px; border-radius: 12px; font-size: 12px; font-weight: 500; }
  .chip-updated { background: #e0eaff; color: #1c4fbf; }
  .chip-ok { background: #dff5e3; color: #1f7a3a; }
  .chip-skipped { background: #ececec; color: #555; }
  .chip-failed { background: #fde3e3; color: #b32626; }
  .chip-conflict { background: #ffe9cc; color: #9a5b00; }
  @media (prefers-color-scheme: dark) {
    .chip-updated { background: #1a2d5c; color: #9fbfff; }
    .chip-ok { background: #1d3d28; color: #7ed29a; }
    .chip-skipped { background: #333; color: #bbb; }
    .chip-failed { background: #4d1d1d; color: #ff9b9b; }
    .chip-conflict { background: #4a3413; color: #ffcd85; }
  }

  .report-repo { border-bottom: 1px solid #ececec; padding: 10px 0; }
  @media (prefers-color-scheme: dark) { .report-repo { border-bottom-color: #2e2e2e; } }
  .report-repo-head { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .branch-list { list-style: none; margin: 6px 0 0 30px; padding: 0; }
  .branch-list li { display: flex; align-items: center; gap: 8px; padding: 3px 0; font-size: 12.5px; }
  .branch-list li.bad .branch-status { color: #d33; }
  .branch-icon { width: 18px; text-align: center; }
  .branch-status { color: #888; }

  .settings { max-width: 540px; }
  .settings label { display: flex; flex-direction: column; gap: 4px; margin-bottom: 16px; }
  .settings label span:first-child { font-weight: 500; font-size: 13px; }
  .settings label small { color: #888; font-size: 11.5px; }
  .settings input[type="text"], .settings input[type="number"] {
    padding: 6px 10px; border: 1px solid #d0d0d0; border-radius: 6px; font-size: 13px; background: white; color: inherit;
  }
  @media (prefers-color-scheme: dark) { .settings input { background: #2d2d2d; border-color: #404040; color: #e8e8e8; } }
  .settings .row { display: flex; gap: 16px; } .settings .row label { flex: 1; }
  .settings .checkbox { flex-direction: row; align-items: center; gap: 8px; }
  .settings .checkbox input { margin: 0; }
  .detected { margin-top: 18px; padding-top: 14px; border-top: 1px solid #ececec; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .badge-ed { display: inline-flex; align-items: center; gap: 6px; }
  @media (prefers-color-scheme: dark) { .detected { border-top-color: #2e2e2e; } }

  .empty { text-align: center; color: #888; padding: 40px; }

  .toast {
    position: fixed; bottom: 20px; left: 50%; transform: translateX(-50%);
    background: #1a1a1a; color: white; padding: 10px 18px; border-radius: 8px;
    font-size: 13px; box-shadow: 0 4px 16px rgba(0,0,0,0.2); max-width: 80%;
  }
</style>
