<script lang="ts">
  import type { Component } from "svelte";
  import { cn } from "$lib/utils";

  interface Item {
    value: string;
    label: string;
    icon?: Component;
    hint?: string;
  }

  interface Props {
    items: Item[];
    value: string;
    onchange: (next: string) => void;
    /** fill=true 时格子等宽铺满（顶部标签条），否则按内容宽（语言、主题这类短选项）。 */
    fill?: boolean;
    ariaLabel?: string;
    class?: string;
  }

  let { items, value, onchange, fill = false, ariaLabel, class: className }: Props = $props();
</script>

<!-- 参考稿的分段控件：外框一圈边框，选中格直接填主色白字，未选中只在悬停时淡蓝。 -->
<div
  role="tablist"
  aria-label={ariaLabel}
  class={cn("inline-flex items-center gap-1 rounded-lg border border-border bg-card p-1", fill && "flex w-full", className)}
>
  {#each items as it (it.value)}
    {@const Icon = it.icon}
    <button
      type="button"
      role="tab"
      aria-selected={value === it.value}
      title={it.hint}
      class={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-sm transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        fill && "min-w-0 flex-1",
        value === it.value
          ? "bg-primary font-medium text-primary-foreground elevate-hi"
          : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
      )}
      onclick={() => onchange(it.value)}
    >
      {#if Icon}
        <Icon class="size-4 shrink-0" />
      {/if}
      <span class="min-w-0 truncate">{it.label}</span>
    </button>
  {/each}
</div>
