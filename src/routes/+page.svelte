<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "$lib/api";
  import type { AppConfig, RepoInfo, SyncReport, PullOutcome } from "$lib/types";

  let config = $state<AppConfig | null>(null);
  let repos = $state<RepoInfo[]>([]);
  let lastReport = $state<SyncReport | null>(null);
  let availableReports = $state<string[]>([]);
  let selectedReportDate = $state<string>("");
  let viewingReport = $state<SyncReport | null>(null);
  let tab = $state<"repos" | "report" | "settings">("repos");
  let syncing = $state(false);
  let toast = $state<string>("");

  let unlistenStarted: UnlistenFn | null = null;
  let unlistenFinished: UnlistenFn | null = null;
  let unlistenFailed: UnlistenFn | null = null;
  let onVisible: (() => void) | null = null;

  onMount(async () => {
    await refresh();
    unlistenStarted = await listen("sync-started", () => {
      syncing = true;
      toast = "Syncing…";
    });
    unlistenFinished = await listen<SyncReport>("sync-finished", async (e) => {
      syncing = false;
      lastReport = e.payload;
      // Re-fetch config so `last_run` (and any other backend-side changes) are reflected.
      config = await api.getConfig();
      const [u, ok, sk, f] = summary(e.payload);
      toast = `Sync done · ${u} updated · ${ok} ok · ${sk} skipped · ${f} failed`;
      await refreshReports();
    });
    unlistenFailed = await listen<string>("sync-failed", (e) => {
      syncing = false;
      toast = `Sync failed: ${e.payload}`;
    });

    // The window may be hidden + shown via the tray rather than closed/reopened,
    // so onMount only fires once. Refresh whenever the page becomes visible
    // again so we don't show stale data after a scheduled sync ran in the
    // background.
    onVisible = () => {
      if (document.visibilityState === "visible") {
        refresh();
      }
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

  async function toggleRepo(rel: string, on: boolean) {
    if (!config) return;
    config.repo_enabled[rel] = on;
    await api.setConfig(config);
  }

  function isEnabled(rel: string): boolean {
    if (!config) return true;
    return config.repo_enabled[rel] ?? true;
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

  function summary(r: SyncReport): [number, number, number, number] {
    let u = 0, ok = 0, sk = 0, f = 0;
    for (const x of r.results) {
      if (x.outcome.kind === "updated") u++;
      else if (x.outcome.kind === "up_to_date") ok++;
      else if (x.outcome.kind === "skipped") sk++;
      else if (x.outcome.kind === "failed") f++;
    }
    return [u, ok, sk, f];
  }

  function outcomeIcon(o: PullOutcome): string {
    switch (o.kind) {
      case "updated": return "🔄";
      case "up_to_date": return "✅";
      case "skipped": return "⏭️";
      case "failed": return "❌";
    }
  }

  function outcomeLabel(o: PullOutcome): string {
    switch (o.kind) {
      case "updated": return `pulled ${o.commits} commit${o.commits === 1 ? "" : "s"}`;
      case "up_to_date": return "up to date";
      case "skipped": return `skipped — ${o.reason}`;
      case "failed": return o.message;
    }
  }

  function formatTime(iso: string | null): string {
    if (!iso) return "never";
    const d = new Date(iso);
    return d.toLocaleString();
  }

  function findLastOutcomeFor(rel: string): PullOutcome | null {
    if (!lastReport) return null;
    const r = lastReport.results.find((x) => x.repo.rel_path === rel);
    return r ? r.outcome : null;
  }

  $effect(() => {
    if (toast) {
      const t = setTimeout(() => (toast = ""), 4000);
      return () => clearTimeout(t);
    }
  });
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="dot" class:syncing></span>
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
    <button class:active={tab === "report"} onclick={() => (tab = "report")}>
      Reports
    </button>
    <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>
      Settings
    </button>
  </nav>

  {#if tab === "repos"}
    <section class="panel">
      {#if repos.length === 0}
        <p class="empty">No git repos found under <code>{config?.root}</code>. Check your Settings.</p>
      {:else}
        <table>
          <thead>
            <tr>
              <th></th>
              <th>Repo</th>
              <th>Branch</th>
              <th>Last result</th>
            </tr>
          </thead>
          <tbody>
            {#each repos as repo (repo.rel_path)}
              {@const last = findLastOutcomeFor(repo.rel_path)}
              <tr class:disabled={!isEnabled(repo.rel_path)}>
                <td>
                  <input
                    type="checkbox"
                    checked={isEnabled(repo.rel_path)}
                    onchange={(e) =>
                      toggleRepo(repo.rel_path, (e.target as HTMLInputElement).checked)}
                  />
                </td>
                <td>
                  <code>{repo.rel_path}</code>
                  {#if !repo.has_remote}
                    <span class="badge muted">no remote</span>
                  {/if}
                </td>
                <td><span class="branch">{repo.branch}</span></td>
                <td>
                  {#if last}
                    <span class="outcome outcome-{last.kind}">
                      {outcomeIcon(last)} {outcomeLabel(last)}
                    </span>
                  {:else}
                    <span class="muted">—</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {/if}

  {#if tab === "report"}
    <section class="panel">
      {#if availableReports.length === 0}
        <p class="empty">No reports yet. Click <strong>Sync now</strong> to generate one.</p>
      {:else}
        <div class="report-header">
          <label>
            Date:
            <select value={selectedReportDate} onchange={(e) => onPickReport((e.target as HTMLSelectElement).value)}>
              {#each availableReports as d}
                <option value={d}>{d}</option>
              {/each}
            </select>
          </label>
          {#if viewingReport}
            {@const s = summary(viewingReport)}
            <div class="summary">
              <span class="chip chip-updated">{s[0]} updated</span>
              <span class="chip chip-ok">{s[1]} up to date</span>
              <span class="chip chip-skipped">{s[2]} skipped</span>
              <span class="chip chip-failed">{s[3]} failed</span>
            </div>
          {/if}
        </div>

        {#if viewingReport}
          <ul class="report-list">
            {#each viewingReport.results as r (r.repo.rel_path)}
              <li>
                <span class="outcome-icon">{outcomeIcon(r.outcome)}</span>
                <code>{r.repo.rel_path}</code>
                <span class="branch">({r.repo.branch})</span>
                <span class="outcome-text outcome-{r.outcome.kind}">{outcomeLabel(r.outcome)}</span>
              </li>
            {/each}
          </ul>
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
          <label>
            <span>Daily run hour</span>
            <input type="number" min="0" max="23" bind:value={config.schedule_hour} />
          </label>
          <label>
            <span>Minute</span>
            <input type="number" min="0" max="59" bind:value={config.schedule_minute} />
          </label>
        </div>
        <label class="checkbox">
          <input type="checkbox" bind:checked={config.notify_on_finish} />
          <span>Show notification when sync finishes</span>
        </label>
        <button class="primary" onclick={saveSettings}>Save settings</button>
      {/if}
    </section>
  {/if}

  {#if toast}
    <div class="toast">{toast}</div>
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif;
    background: #fafafa;
    color: #1a1a1a;
  }
  @media (prefers-color-scheme: dark) {
    :global(body) { background: #1a1a1a; color: #e8e8e8; }
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border, #e2e2e2);
    background: var(--bg-surface, #fff);
  }
  @media (prefers-color-scheme: dark) {
    header { background: #232323; border-bottom-color: #333; }
  }

  .brand { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .brand h1 { margin: 0; font-size: 16px; font-weight: 600; }
  .muted { color: #888; font-size: 12px; }
  .last-sync { margin-right: 8px; }

  .dot {
    width: 10px; height: 10px; border-radius: 50%;
    background: #36c75a; box-shadow: 0 0 6px #36c75a;
  }
  .dot.syncing {
    background: #f5a623; box-shadow: 0 0 6px #f5a623;
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse { from { opacity: 0.6; } to { opacity: 1; } }

  button {
    background: #f0f0f0; border: 1px solid #d0d0d0; border-radius: 6px;
    padding: 6px 12px; font-size: 13px; cursor: pointer;
  }
  button:hover { background: #e8e8e8; }
  button.primary {
    background: #2469e6; border-color: #2469e6; color: white;
  }
  button.primary:hover { background: #1f5fd0; }
  button.primary:disabled { opacity: 0.6; cursor: not-allowed; }
  @media (prefers-color-scheme: dark) {
    button { background: #2d2d2d; border-color: #404040; color: #e8e8e8; }
    button:hover { background: #3a3a3a; }
  }

  .tabs {
    display: flex; gap: 4px; padding: 8px 16px;
    background: var(--bg-surface, #fff);
    border-bottom: 1px solid var(--border, #e2e2e2);
  }
  @media (prefers-color-scheme: dark) {
    .tabs { background: #232323; border-bottom-color: #333; }
  }
  .tabs button {
    background: transparent; border: none; padding: 6px 12px;
    color: #666; font-weight: 500;
  }
  .tabs button.active { color: #2469e6; border-bottom: 2px solid #2469e6; border-radius: 0; }
  @media (prefers-color-scheme: dark) {
    .tabs button { color: #aaa; }
    .tabs button.active { color: #6ba1ff; border-bottom-color: #6ba1ff; }
  }
  .count {
    display: inline-block; background: #eee; color: #555;
    padding: 1px 7px; border-radius: 10px; font-size: 11px; margin-left: 4px;
  }
  @media (prefers-color-scheme: dark) { .count { background: #3a3a3a; color: #ccc; } }

  .panel {
    flex: 1; overflow-y: auto; padding: 16px 20px;
  }

  table { width: 100%; border-collapse: collapse; }
  th, td {
    text-align: left; padding: 8px 10px; font-size: 13px;
    border-bottom: 1px solid var(--border, #ececec);
  }
  th { font-weight: 500; color: #666; font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; }
  @media (prefers-color-scheme: dark) {
    th, td { border-bottom-color: #2e2e2e; }
    th { color: #999; }
  }
  tr.disabled { opacity: 0.45; }

  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12.5px;
  }
  .branch {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px; color: #888;
  }
  .badge {
    display: inline-block; padding: 1px 6px; border-radius: 4px;
    background: #eee; font-size: 11px; margin-left: 6px;
  }
  @media (prefers-color-scheme: dark) { .badge { background: #333; } }

  .outcome { font-size: 12.5px; }
  .outcome-updated { color: #2469e6; }
  .outcome-up_to_date { color: #36a857; }
  .outcome-skipped { color: #888; }
  .outcome-failed { color: #d33; }

  .report-header {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 16px; gap: 16px; flex-wrap: wrap;
  }
  .report-header label { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .report-header select {
    border: 1px solid #d0d0d0; border-radius: 6px; padding: 4px 8px; font-size: 13px;
    background: white;
  }
  @media (prefers-color-scheme: dark) { .report-header select { background: #2d2d2d; border-color: #404040; color: #e8e8e8; } }

  .summary { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip {
    padding: 3px 10px; border-radius: 12px; font-size: 12px; font-weight: 500;
  }
  .chip-updated { background: #e0eaff; color: #1c4fbf; }
  .chip-ok { background: #dff5e3; color: #1f7a3a; }
  .chip-skipped { background: #ececec; color: #555; }
  .chip-failed { background: #fde3e3; color: #b32626; }
  @media (prefers-color-scheme: dark) {
    .chip-updated { background: #1a2d5c; color: #9fbfff; }
    .chip-ok { background: #1d3d28; color: #7ed29a; }
    .chip-skipped { background: #333; color: #bbb; }
    .chip-failed { background: #4d1d1d; color: #ff9b9b; }
  }

  .report-list { list-style: none; padding: 0; margin: 0; }
  .report-list li {
    display: flex; align-items: center; gap: 10px; padding: 8px 4px;
    border-bottom: 1px solid var(--border, #ececec); font-size: 13px;
  }
  @media (prefers-color-scheme: dark) { .report-list li { border-bottom-color: #2e2e2e; } }
  .outcome-icon { width: 24px; text-align: center; }
  .outcome-text { margin-left: auto; font-size: 12.5px; }

  .settings { max-width: 540px; }
  .settings label {
    display: flex; flex-direction: column; gap: 4px; margin-bottom: 16px;
  }
  .settings label span:first-child { font-weight: 500; font-size: 13px; }
  .settings label small { color: #888; font-size: 11.5px; }
  .settings input[type="text"], .settings input[type="number"] {
    padding: 6px 10px; border: 1px solid #d0d0d0; border-radius: 6px;
    font-size: 13px; background: white; color: inherit;
  }
  @media (prefers-color-scheme: dark) {
    .settings input { background: #2d2d2d; border-color: #404040; color: #e8e8e8; }
  }
  .settings .row { display: flex; gap: 16px; }
  .settings .row label { flex: 1; }
  .settings .checkbox {
    flex-direction: row; align-items: center; gap: 8px;
  }
  .settings .checkbox input { margin: 0; }

  .empty { text-align: center; color: #888; padding: 40px; }

  .toast {
    position: fixed; bottom: 20px; left: 50%; transform: translateX(-50%);
    background: #1a1a1a; color: white; padding: 10px 18px; border-radius: 8px;
    font-size: 13px; box-shadow: 0 4px 16px rgba(0,0,0,0.2);
  }
</style>
