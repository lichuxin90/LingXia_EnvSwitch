<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { cn } from "$lib/utils";

  interface Props {
    title: string;
    onclose: () => void;
    /** danger 时标题前挂警示图标，整块边框转成危险色。 */
    tone?: "default" | "danger";
    class?: string;
    children?: Snippet;
    footer?: Snippet;
  }

  let { title, onclose, tone = "default", class: className, children, footer }: Props = $props();

  // 捕获阶段拦 Esc：设置浮层也在 window 上听 Esc，不拦住会连它一起关掉。
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onclose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

<!-- 遮罩不是控件：键盘走上面的 Esc 和标题栏的关闭按钮，这里只处理"点外面关掉"。 -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="fixed inset-0 z-40 flex items-center justify-center bg-foreground/25 px-4 backdrop-blur-[1px]"
  onclick={(e) => {
    if (e.target === e.currentTarget) onclose();
  }}
>
  <div
    class={cn(
      "flex max-h-[86vh] w-[min(36rem,94vw)] flex-col rounded-lg border bg-card shadow-lg",
      tone === "danger" ? "border-destructive/50" : "border-border",
      className,
    )}
    role="dialog"
    aria-modal="true"
    aria-label={title}
  >
    <header
      class={cn(
        "flex shrink-0 items-center gap-2 border-b px-4 py-3",
        tone === "danger" ? "border-destructive/30 bg-destructive/6" : "border-border",
      )}
    >
      {#if tone === "danger"}
        <svg viewBox="0 0 24 24" class="size-5 shrink-0 text-destructive" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
        </svg>
      {/if}
      <h2 class="min-w-0 flex-1 text-sm font-semibold">{title}</h2>
      <button
        type="button"
        class="shrink-0 rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        aria-label={t("common.close")}
        onclick={onclose}
      >
        <X class="size-4" />
      </button>
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4 text-sm leading-relaxed">
      {#if children}{@render children()}{/if}
    </div>
    {#if footer}
      <footer class="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-border px-4 py-3">
        {@render footer()}
      </footer>
    {/if}
  </div>
</div>