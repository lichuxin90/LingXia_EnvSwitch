<script lang="ts">
  import { X } from "@lucide/svelte";
  import Progress from "$lib/components/Progress.svelte";
  import { app } from "$lib/state.svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { humanEta, humanRate, percentOf, phaseLabel, TOOL_LABEL } from "$lib/utils";

  const rows = $derived.by(() => {
    app.tick;
    return [...app.progress.values()];
  });
</script>

{#if rows.length}
  <div class="shrink-0 border-t border-border bg-card">
    <ul class="divide-y divide-border">
      {#each rows as r (r.jobId)}
        {@const pct = r.percent ?? (r.total ? percentOf(r.received, r.total) : null)}
        <!-- 五列用 minmax 撑开，换成较长的英文或日文词条时靠截断，不会把进度条挤没。 -->
        <li
          class="grid grid-cols-[minmax(8rem,1fr)_auto_minmax(6rem,3fr)_minmax(8rem,2fr)_auto] items-center gap-3 px-4 py-2.5 text-xs"
        >
          <span class="min-w-0 truncate text-muted-foreground">{r.tool ? TOOL_LABEL[r.tool] : t("common.task")} {r.version ?? ""}</span>
          <span class="shrink-0 whitespace-nowrap font-medium">{phaseLabel(r.phase)}</span>
          <div class="min-w-0">
            {#if r.phase === "downloading"}
              <Progress value={pct ?? 0} indeterminate={pct === null} />
            {:else}
              <div class="h-2 w-full rounded-full bg-muted/60"></div>
            {/if}
          </div>
          <span class="mono min-w-0 truncate text-right text-muted-foreground">
            {#if r.message}
              {r.message}
            {:else if r.phase === "downloading"}
              {pct !== null ? `${pct}% · ` : ""}{humanRate(r.bytesPerSec)}{humanEta(r.etaSecs) ? ` · ${humanEta(r.etaSecs)}` : ""}
            {:else}
              {t("job.runningInstall")}
            {/if}
          </span>
          <button
            type="button"
            class="shrink-0 rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            title={t("job.cancel")}
            aria-label={t("job.cancel")}
            onclick={() => void app.cancel(r.jobId)}
          >
            <X class="size-3.5" />
          </button>
        </li>
      {/each}
    </ul>
  </div>
{/if}
