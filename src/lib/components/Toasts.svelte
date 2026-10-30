<script lang="ts">
  import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from "@lucide/svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { cn } from "$lib/utils";

  const toneClass = {
    info: "border-border bg-card text-foreground",
    ok: "border-success/35 bg-success/8 text-foreground",
    warn: "border-warning/40 bg-warning/8 text-foreground",
    bad: "border-destructive/40 bg-destructive/8 text-foreground",
  } as const;
</script>

{#if app.toasts.length}
  <div class="pointer-events-none fixed bottom-4 left-1/2 z-50 flex w-[min(46rem,92vw)] -translate-x-1/2 flex-col gap-2">
    {#each app.toasts as toast (toast.id)}
      <div
        class={cn(
          "pointer-events-auto flex items-start gap-2.5 rounded-lg border px-3 py-2.5 text-sm shadow-md",
          toneClass[toast.tone],
        )}
        role="status"
      >
        {#if toast.tone === "ok"}
          <CircleCheck class="mt-0.5 size-4 shrink-0 text-success" />
        {:else if toast.tone === "bad"}
          <CircleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
        {:else if toast.tone === "warn"}
          <TriangleAlert class="mt-0.5 size-4 shrink-0 text-warning" />
        {:else}
          <Info class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        {/if}
        <span class="min-w-0 flex-1 leading-relaxed">{toast.text}</span>
        <button
          type="button"
          class="shrink-0 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          aria-label={t("common.dismiss")}
          onclick={() => (app.toasts = app.toasts.filter((x) => x.id !== toast.id))}
        >
          <X class="size-3.5" />
        </button>
      </div>
    {/each}
  </div>
{/if}
