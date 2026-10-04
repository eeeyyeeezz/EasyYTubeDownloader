<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, COOKIE_BROWSERS, EXTENSION_URL, PRESETS, REPO_URL, type Settings } from "./api";
  import { t } from "./i18n.svelte";

  let {
    settings,
    onchange,
    onclose,
    onreinstall,
  }: {
    settings: Settings;
    onchange: (s: Settings) => void;
    onclose: () => void;
    onreinstall: () => void;
  } = $props();

  let version = $state<string | null>(null);
  let updating = $state(false);
  let updateMessage = $state<string | null>(null);

  onMount(async () => {
    version = await api.engineVersion().catch(() => null);
  });

  async function update() {
    updating = true;
    updateMessage = null;
    try {
      updateMessage = lastLine(await api.engineUpdate());
      version = await api.engineVersion().catch(() => version);
    } catch (e) {
      updateMessage = lastLine(String(e));
    } finally {
      updating = false;
    }
  }

  function lastLine(text: string): string {
    return text.trim().split("\n").pop() ?? "";
  }

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    onchange({ ...settings, [key]: value });
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={onclose} role="presentation"></div>
<div class="panel" role="dialog" aria-modal="true" aria-labelledby="settings-title">
  <header>
    <h2 id="settings-title">{t("settings.title")}</h2>
    <button class="btn" onclick={onclose}>{t("settings.close")}</button>
  </header>

  <label class="field">
    <span>{t("settings.language")}</span>
    <select
      value={settings.language}
      onchange={(e) => set("language", e.currentTarget.value as Settings["language"])}
    >
      <option value="auto">{t("settings.auto")}</option>
      <option value="en">English</option>
      <option value="ru">Русский</option>
    </select>
  </label>

  <label class="field">
    <span>{t("settings.defaultPreset")}</span>
    <select
      value={settings.preset}
      onchange={(e) => set("preset", e.currentTarget.value as Settings["preset"])}
    >
      {#each PRESETS as p (p)}
        <option value={p}>{t(`presets.${p}`)} · {t(`presetHints.${p}`)}</option>
      {/each}
    </select>
  </label>

  <label class="field">
    <span>{t("settings.parallel")}</span>
    <select
      value={String(settings.maxParallel)}
      onchange={(e) => set("maxParallel", Number(e.currentTarget.value))}
    >
      {#each [1, 2, 3, 4, 5] as n (n)}
        <option value={String(n)}>{n}</option>
      {/each}
    </select>
  </label>

  <label class="field">
    <span>{t("settings.cookies")}</span>
    <select value={settings.cookiesBrowser} onchange={(e) => set("cookiesBrowser", e.currentTarget.value)}>
      <option value="">{t("settings.cookiesOff")}</option>
      {#each Object.entries(COOKIE_BROWSERS) as [id, name] (id)}
        <option value={id}>{name}</option>
      {/each}
    </select>
    <small class="hint">{t("settings.cookiesHint")}</small>
  </label>

  <section>
    <h3>{t("settings.engine")}</h3>
    <p class="hint">{t("settings.engineHint")}</p>
    {#if version}<p class="mono">{t("settings.version", { v: version })}</p>{/if}
    <div class="buttons">
      <button class="btn primary" onclick={update} disabled={updating}>
        {updating ? t("settings.updating") : t("settings.update")}
      </button>
      <button class="btn" onclick={onreinstall} disabled={updating}>{t("settings.reinstall")}</button>
    </div>
    {#if updateMessage}<p class="mono">{updateMessage}</p>{/if}
  </section>

  <section>
    <h3>{t("settings.extension")}</h3>
    <p class="hint">{t("settings.extensionHint")}</p>
    <button class="btn" onclick={() => openUrl(EXTENSION_URL)}>{t("settings.getExtension")}</button>
  </section>

  <footer>
    <button class="link" onclick={() => openUrl(REPO_URL)}>{t("settings.about")}</button>
  </footer>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 10;
  }

  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(400px, 100%);
    background: var(--surface);
    border-left: 1px solid var(--line);
    padding: 20px;
    overflow-y: auto;
    overflow-x: hidden;
    z-index: 11;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h2 {
    margin: 0;
    font-size: 20px;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0 0 4px;
    font-size: 14px;
  }

  .field {
    display: grid;
    gap: 6px;
    font-size: 13px;
    font-weight: 500;
  }

  select {
    width: 100%;
    min-width: 0;
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--bg);
  }

  section {
    border-top: 1px solid var(--line);
    padding-top: 16px;
  }

  .hint {
    color: var(--muted);
    font-size: 13px;
    font-weight: 400;
    margin: 0 0 10px;
  }

  small.hint {
    margin: 0;
    font-size: 12px;
  }

  .mono {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--muted);
    margin: 8px 0;
    word-break: break-word;
  }

  .buttons {
    display: flex;
    gap: 8px;
  }

  footer {
    margin-top: auto;
  }
</style>
