<script lang="ts">
  import { formatBytes, formatEta, type Job } from "./api";
  import { t } from "./i18n.svelte";

  let {
    job,
    oncancel,
    onretry,
    onremove,
    onreveal,
  }: {
    job: Job;
    oncancel: () => void;
    onretry: () => void;
    onremove: () => void;
    onreveal: () => void;
  } = $props();

  let showLog = $state(false);

  const active = $derived(["queued", "starting", "downloading", "processing"].includes(job.status));
  const indeterminate = $derived(
    job.status === "queued" ||
      job.status === "starting" ||
      job.status === "processing" ||
      (job.status === "downloading" && job.percent == null),
  );

  const detail = $derived.by(() => {
    const parts: string[] = [];
    if (job.items && job.item) parts.push(t("item", { n: job.item, total: job.items }));
    if (job.status === "downloading") {
      if (job.percent != null) parts.push(`${job.percent.toFixed(0)}%`);
      if (job.speed) parts.push(`${formatBytes(job.speed)}/s`);
      if (job.eta) parts.push(t("left", { time: formatEta(job.eta) }));
    }
    return parts.join(" · ");
  });
</script>

<li class="job {job.status}">
  <div class="thumb">
    {#if job.thumbnail}
      <img src={job.thumbnail} alt="" loading="lazy" referrerpolicy="no-referrer" />
    {:else}
      <span aria-hidden="true">{job.preset === "mp3" || job.preset === "m4a" ? "♪" : "▶"}</span>
    {/if}
  </div>

  <div class="body">
    <div class="title" title={job.title ?? job.url}>{job.title ?? job.url}</div>
    <div class="meta">
      <span class="tag">{t(`presets.${job.preset}`)}</span>
      <span class="status">{t(`status.${job.status}`)}</span>
      {#if detail}<span class="detail">{detail}</span>{/if}
    </div>

    {#if job.status === "error"}
      <p class="error">{t(`errors.${job.error ?? "unknown"}`)}</p>
    {/if}

    {#if active}
      <div class="bar" class:indeterminate>
        <div class="fill" style:width={indeterminate ? undefined : `${job.percent ?? 0}%`}></div>
      </div>
    {/if}

    {#if showLog && job.log}
      <pre class="log">{job.log}</pre>
    {/if}
  </div>

  <div class="actions">
    {#if active}
      <button class="btn" onclick={oncancel}>{t("cancel")}</button>
    {:else if job.status === "done"}
      <button class="btn" onclick={onreveal}>{t("showInFolder")}</button>
      <button class="icon" onclick={onremove} aria-label={t("remove")} title={t("remove")}>×</button>
    {:else}
      <button class="btn" onclick={onretry}>{t("retry")}</button>
      {#if job.log}
        <button class="link" onclick={() => (showLog = !showLog)}>
          {showLog ? t("hideLog") : t("showLog")}
        </button>
      {/if}
      <button class="icon" onclick={onremove} aria-label={t("remove")} title={t("remove")}>×</button>
    {/if}
  </div>
</li>

<style>
  .job {
    display: grid;
    grid-template-columns: 72px 1fr auto;
    gap: 12px;
    align-items: start;
    padding: 12px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .thumb {
    width: 72px;
    aspect-ratio: 16 / 9;
    border-radius: 8px;
    overflow: hidden;
    background: var(--track);
    display: grid;
    place-items: center;
    color: var(--muted);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .body {
    min-width: 0;
  }

  .title {
    font-weight: 600;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: center;
    font-size: 12px;
    color: var(--muted);
    margin-top: 2px;
    font-variant-numeric: tabular-nums;
  }

  .tag {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0 6px;
    font-weight: 600;
  }

  .done .status {
    color: var(--ok);
    font-weight: 600;
  }

  .error .status,
  p.error {
    color: var(--accent);
  }

  p.error {
    font-size: 13px;
    margin: 6px 0 0;
  }

  .bar {
    position: relative;
    height: 4px;
    margin-top: 10px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.25s ease;
  }

  .indeterminate .fill {
    position: absolute;
    width: 30%;
    animation: slide 1.2s ease-in-out infinite;
  }

  @keyframes slide {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .indeterminate .fill {
      animation: none;
      width: 100%;
      opacity: 0.4;
    }
  }

  .log {
    margin: 10px 0 0;
    padding: 10px;
    max-height: 180px;
    overflow: auto;
    font-family: var(--mono);
    font-size: 11px;
    background: var(--bg);
    border-radius: 8px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .actions {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }

  .icon {
    background: none;
    border: none;
    color: var(--muted);
    font-size: 18px;
    line-height: 1;
    padding: 2px 4px;
  }

  .icon:hover {
    color: var(--ink);
  }

  @media (max-width: 520px) {
    .job {
      grid-template-columns: 1fr auto;
    }
    .thumb {
      display: none;
    }
  }
</style>
