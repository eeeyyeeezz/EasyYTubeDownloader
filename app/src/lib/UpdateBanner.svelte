<script lang="ts">
  import { onMount } from "svelte";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { t } from "./i18n.svelte";

  let update = $state<Update | null>(null);
  let percent = $state<number | null>(null);
  let failed = $state(false);

  onMount(async () => {
    // Offline, rate-limited or no release yet: just stay quiet.
    update = await check().catch(() => null);
  });

  async function install() {
    if (!update) return;
    failed = false;
    percent = 0;
    let total = 0;
    let received = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          received += event.data.chunkLength;
          percent = total ? Math.min(100, Math.round((received / total) * 100)) : null;
        }
      });
      await relaunch();
    } catch {
      failed = true;
      percent = null;
    }
  }
</script>

{#if update}
  <div class="banner" role="status">
    <span>
      {#if failed}
        {t("update.failed")}
      {:else if percent !== null}
        {t("update.installing", { percent })}
      {:else}
        {t("update.available", { version: update.version })}
      {/if}
    </span>
    {#if percent === null}
      <button class="btn primary" onclick={install}>{failed ? t("retry") : t("update.install")}</button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 16px;
    padding: 10px 12px 10px 14px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    font-size: 13px;
    font-weight: 500;
  }
</style>
