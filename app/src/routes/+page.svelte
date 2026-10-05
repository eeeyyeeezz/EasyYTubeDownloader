<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open } from "@tauri-apps/plugin-dialog";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    api,
    isYouTubeUrl,
    looksLikePlaylist,
    PRESETS,
    type Job,
    type JobUpdate,
    type LinkRequest,
    type Preset,
    type Settings,
  } from "$lib/api";
  import { setLanguage, t } from "$lib/i18n.svelte";
  import FirstRun from "$lib/FirstRun.svelte";
  import JobItem from "$lib/JobItem.svelte";
  import SettingsPanel from "$lib/SettingsPanel.svelte";
  import UpdateBanner from "$lib/UpdateBanner.svelte";

  let settings = $state<Settings | null>(null);
  let engine = $state<{ ready: boolean; missing: string[]; force: boolean } | null>(null);
  let jobs = $state<Job[]>([]);
  let url = $state("");
  let formError = $state<string | null>(null);
  let showSettings = $state(false);
  let browsers = $state<string[]>([]);

  // Links from the extension that arrived before the engine was ready.
  let waiting: LinkRequest[] = [];
  // Updates that arrived before `start_download` returned the job id.
  const early = new Map<number, JobUpdate>();
  let lastClipboard = "";

  const preset = $derived(settings?.preset ?? "best");
  const finished = $derived(jobs.some((j) => ["done", "error", "cancelled"].includes(j.status)));

  async function saveSettings(next: Settings) {
    settings = await api.saveSettings(next);
    setLanguage(settings.language);
  }

  async function start(target: string, p: Preset = preset, playlist = settings?.playlist ?? false) {
    if (!settings) return;
    const request = { url: target.trim(), preset: p, dir: settings.downloadDir, playlist };
    const cookies = settings.cookiesBrowser;
    try {
      const id = await api.startDownload(request);
      jobs.unshift({ id, status: "queued", cookies, ...request, ...early.get(id) });
      early.delete(id);
      return true;
    } catch (e) {
      formError = String(e);
      return false;
    }
  }

  async function submit(e?: SubmitEvent) {
    e?.preventDefault();
    formError = null;
    if (!url.trim()) return;
    if (await start(url)) url = "";
  }

  /** Translated message for a known error code, otherwise the raw message. */
  function errorText(code: string) {
    const key = `errors.${code}`;
    const text = t(key);
    return text === key ? code : text;
  }

  function onJobUpdate(u: JobUpdate) {
    const job = jobs.find((j) => j.id === u.id);
    if (job) Object.assign(job, u);
    else early.set(u.id, { ...early.get(u.id), ...u });
  }

  async function takeLinks() {
    const links = await api.takePendingLinks();
    waiting.push(...links);
    if (!engine?.ready || !settings) return;
    const batch = waiting;
    waiting = [];
    for (const link of batch) await start(link.url, link.preset ?? preset);
  }

  async function pasteFromClipboard(onlyYouTube: boolean) {
    const text = (await readText().catch(() => ""))?.trim() ?? "";
    if (!text || (onlyYouTube && (!isYouTubeUrl(text) || text === lastClipboard))) return;
    lastClipboard = text;
    url = text;
  }

  async function chooseFolder() {
    if (!settings) return;
    const dir = await open({ directory: true, defaultPath: settings.downloadDir });
    if (typeof dir === "string") await saveSettings({ ...settings, downloadDir: dir });
  }

  function retry(job: Job) {
    jobs = jobs.filter((j) => j.id !== job.id);
    start(job.url, job.preset, job.playlist);
  }

  async function retryWithCookies(job: Job, browser: string) {
    if (!settings) return;
    await saveSettings({ ...settings, cookiesBrowser: browser });
    retry(job);
  }

  function remove(job: Job) {
    jobs = jobs.filter((j) => j.id !== job.id);
  }

  function clearFinished() {
    jobs = jobs.filter((j) => !["done", "error", "cancelled"].includes(j.status));
  }

  function engineReady() {
    engine = { ready: true, missing: [], force: false };
    takeLinks();
  }

  function reinstall() {
    showSettings = false;
    engine = { ready: false, missing: ["yt-dlp", "deno", "ffmpeg"], force: true };
  }

  onMount(() => {
    const unlisteners = [
      listen<JobUpdate>("job-update", (e) => onJobUpdate(e.payload)),
      listen("deep-link", () => takeLinks()),
      getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused && !url) pasteFromClipboard(true);
      }),
    ];

    (async () => {
      settings = await api.getSettings();
      setLanguage(settings.language);
      browsers = await api.detectBrowsers().catch(() => []);
      const status = await api.engineStatus();
      engine = { ready: status.ready, missing: status.missing, force: false };
      await takeLinks();
      if (!url) pasteFromClipboard(true);
    })();

    return () => unlisteners.forEach((u) => u.then((f) => f()));
  });
</script>

{#if settings && engine}
  {#if !engine.ready}
    {#key engine.force}
      <FirstRun missing={engine.missing} force={engine.force} onready={engineReady} />
    {/key}
  {:else}
    <main>
      <header class="top">
        <div class="brand"><span class="logo" aria-hidden="true">↓</span>EasyYTubeDownloader</div>
        <button class="gear" onclick={() => (showSettings = true)} aria-label={t("settings.title")} title={t("settings.title")}>
          <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
            <path fill="currentColor" d="M19.4 13a7.6 7.6 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.4 7.4 0 0 0-1.7-1L15 3.3h-4l-.4 2.6a7.4 7.4 0 0 0-1.7 1l-2.5-1-2 3.5L6.6 11a7.6 7.6 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1a7.4 7.4 0 0 0 1.7 1l.4 2.6h4l.4-2.6a7.4 7.4 0 0 0 1.7-1l2.5 1 2-3.5zM13 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7z" transform="translate(-1 0)"/>
          </svg>
        </button>
      </header>

      <UpdateBanner />

      <h1>{t("tagline")}</h1>

      <form class="input" onsubmit={submit}>
        <div class="field" class:invalid={formError}>
          <input
            type="url"
            bind:value={url}
            oninput={() => (formError = null)}
            placeholder={t("urlPlaceholder")}
            spellcheck="false"
            autocomplete="off"
            aria-label={t("urlPlaceholder")}
          />
          {#if !url}
            <button type="button" class="paste" onclick={() => pasteFromClipboard(false)}>{t("paste")}</button>
          {/if}
        </div>
        <button type="submit" class="go" disabled={!url.trim()}>{t("download")}</button>
      </form>
      {#if formError}<p class="form-error" role="alert">{errorText(formError)}</p>{/if}

      <div class="presets" role="radiogroup" aria-label={t("settings.defaultPreset")}>
        {#each PRESETS as p (p)}
          <button
            role="radio"
            aria-checked={preset === p}
            class:active={preset === p}
            onclick={() => settings && saveSettings({ ...settings, preset: p })}
          >
            {t(`presets.${p}`)}
          </button>
        {/each}
      </div>
      <p class="hint">{t(`presetHints.${preset}`)}</p>

      <div class="options">
        <div class="folder">
          <span class="label">{t("saveTo")}</span>
          <span class="path" title={settings.downloadDir}><bdi>{settings.downloadDir}</bdi></span>
          <button class="link" onclick={chooseFolder}>{t("change")}</button>
        </div>
        {#if looksLikePlaylist(url)}
          <label class="check">
            <input
              type="checkbox"
              checked={settings.playlist}
              onchange={(e) => settings && saveSettings({ ...settings, playlist: e.currentTarget.checked })}
            />
            {t("playlist")}
          </label>
        {/if}
      </div>

      <section class="queue">
        <div class="queue-head">
          <h2>{t("queue")}</h2>
          {#if finished}<button class="link" onclick={clearFinished}>{t("clearFinished")}</button>{/if}
        </div>
        {#if jobs.length === 0}
          <p class="empty">{t("queueEmpty")}</p>
        {:else}
          <ul>
            {#each jobs as job (job.id)}
              <JobItem
                {job}
                {browsers}
                onusecookies={(browser) => retryWithCookies(job, browser)}
                oncancel={() => api.cancelDownload(job.id)}
                onretry={() => retry(job)}
                onremove={() => remove(job)}
                onreveal={() => revealItemInDir(job.file ?? job.dir)}
              />
            {/each}
          </ul>
        {/if}
      </section>
    </main>

    {#if showSettings}
      <SettingsPanel
        {settings}
        {browsers}
        onchange={saveSettings}
        onclose={() => (showSettings = false)}
        onreinstall={reinstall}
      />
    {/if}
  {/if}
{/if}

<style>
  main {
    max-width: 680px;
    margin: 0 auto;
    padding: 18px 24px 40px;
  }

  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    font-size: 14px;
    letter-spacing: -0.01em;
  }

  .logo {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--accent);
    color: var(--accent-ink);
    display: grid;
    place-items: center;
    font-size: 14px;
  }

  .gear {
    background: none;
    border: none;
    color: var(--muted);
    padding: 6px;
    border-radius: 8px;
    display: grid;
  }

  .gear:hover {
    color: var(--ink);
    background: var(--surface);
  }

  h1 {
    font-size: clamp(26px, 6vw, 38px);
    line-height: 1.1;
    letter-spacing: -0.03em;
    margin: 36px 0 18px;
  }

  .input {
    display: flex;
    gap: 8px;
  }

  .field {
    flex: 1;
    display: flex;
    align-items: center;
    background: var(--surface);
    border: 1.5px solid var(--line);
    border-radius: var(--radius);
    padding-right: 6px;
    min-width: 0;
  }

  .field:focus-within {
    border-color: var(--ink);
  }

  .field.invalid {
    border-color: var(--accent);
  }

  .field input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    padding: 14px 14px;
    font-family: var(--mono);
    font-size: 14px;
    outline: none;
  }

  .paste {
    border: 1px solid var(--line);
    background: var(--bg);
    border-radius: 9px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .go {
    border: none;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 700;
    padding: 0 22px;
    border-radius: var(--radius);
  }

  .go:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .form-error {
    color: var(--accent);
    font-size: 13px;
    margin: 8px 2px 0;
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 16px;
  }

  .presets button {
    border: 1.5px solid var(--line);
    background: transparent;
    border-radius: 999px;
    padding: 5px 14px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }

  .presets button:hover {
    color: var(--ink);
  }

  .presets button.active {
    background: var(--ink);
    border-color: var(--ink);
    color: var(--bg);
  }

  .hint {
    color: var(--muted);
    font-size: 13px;
    margin: 8px 2px 0;
  }

  .options {
    display: grid;
    gap: 10px;
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px dashed var(--line);
  }

  .folder {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 13px;
    min-width: 0;
  }

  .label {
    color: var(--muted);
    white-space: nowrap;
  }

  .path {
    font-family: var(--mono);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
    min-width: 0;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .check input {
    accent-color: var(--accent);
  }

  .queue {
    margin-top: 32px;
  }

  .queue-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 10px;
  }

  h2 {
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
    margin: 0;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 8px;
  }

  .empty {
    color: var(--muted);
    font-size: 14px;
    padding: 28px 16px;
    text-align: center;
    border: 1.5px dashed var(--line);
    border-radius: var(--radius);
    margin: 0;
  }

  @media (max-width: 520px) {
    main {
      padding: 14px 16px 32px;
    }
    .input {
      flex-direction: column;
    }
    .go {
      padding: 12px;
    }
  }
</style>
