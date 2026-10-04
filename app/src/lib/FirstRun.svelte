<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, formatBytes, type EngineProgress } from "./api";
  import { t } from "./i18n.svelte";

  let {
    missing,
    force = false,
    onready,
  }: { missing: string[]; force?: boolean; onready: () => void } = $props();

  const order = ["yt-dlp", "deno", "ffmpeg"] as const;
  let progress = $state<Record<string, EngineProgress | undefined>>({});
  let error = $state<string | null>(null);
  let running = $state(false);

  // svelte-ignore state_referenced_locally
  const components = order.filter((c) => force || missing.includes(c));

  async function install() {
    error = null;
    running = true;
    try {
      const status = await api.engineInstall(force);
      if (status.ready) onready();
    } catch (e) {
      error = String(e);
    } finally {
      running = false;
    }
  }

  onMount(() => {
    const unlisten = listen<EngineProgress>("engine-progress", (e) => {
      progress[e.payload.component] = e.payload;
    });
    install();
    return () => {
      unlisten.then((f) => f());
    };
  });

  function percent(p: EngineProgress | undefined): number {
    if (!p) return 0;
    if (p.phase !== "download") return 100;
    return p.total ? Math.min(100, (p.downloaded / p.total) * 100) : 0;
  }
</script>

<section class="first-run">
  <div class="mark" aria-hidden="true">↓</div>
  <h1>{t("firstRun.title")}</h1>
  <p class="lead">{t("firstRun.text")}</p>

  <ul>
    {#each components as c (c)}
      {@const p = progress[c]}
      <li class:done={p?.phase === "done"}>
        <div class="row">
          <span class="name">{t(`firstRun.components.${c}`)}</span>
          <span class="phase">
            {t(`firstRun.phase.${p?.phase ?? "wait"}`)}
            {#if p?.phase === "download" && p.downloaded}
              · {formatBytes(p.downloaded)}{p.total ? ` / ${formatBytes(p.total)}` : ""}
            {/if}
          </span>
        </div>
        <div class="bar"><div class="fill" style:width="{percent(p)}%"></div></div>
      </li>
    {/each}
  </ul>

  {#if error}
    <div class="error" role="alert">
      <p>{t("firstRun.failed")}</p>
      <code>{error}</code>
      <button class="btn primary" onclick={install} disabled={running}>{t("firstRun.retry")}</button>
    </div>
  {/if}
</section>

<style>
  .first-run {
    max-width: 440px;
    margin: 0 auto;
    padding: 64px 24px 32px;
  }

  .mark {
    width: 52px;
    height: 52px;
    border-radius: 16px;
    background: var(--accent);
    color: var(--accent-ink);
    display: grid;
    place-items: center;
    font-size: 28px;
    font-weight: 700;
    animation: bob 1.6s ease-in-out infinite;
  }

  @keyframes bob {
    50% {
      transform: translateY(4px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .mark {
      animation: none;
    }
  }

  h1 {
    font-size: 26px;
    letter-spacing: -0.02em;
    margin: 20px 0 6px;
  }

  .lead {
    color: var(--muted);
    margin: 0 0 28px;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 16px;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 14px;
    margin-bottom: 6px;
  }

  .phase {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }

  li.done .fill {
    background: var(--ok);
  }

  .error {
    margin-top: 28px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--accent-soft);
  }

  .error p {
    margin: 0 0 8px;
    font-weight: 500;
  }

  .error code {
    display: block;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 12px;
    word-break: break-word;
  }
</style>
