<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  interface Props {
    label: string;
    hint?: string;
    class?: string;
    children: Snippet;
  }

  let { label, hint, class: className, children }: Props = $props();
</script>

<div class={cn("flex items-start justify-between gap-4 py-3", className)}>
  <div class="min-w-0 flex-1">
    <div class="text-sm font-medium">{label}</div>
    {#if hint}
      <div class="mt-1 text-xs leading-relaxed text-muted-foreground">{hint}</div>
    {/if}
  </div>
  <div class="shrink-0">{@render children()}</div>
</div>
