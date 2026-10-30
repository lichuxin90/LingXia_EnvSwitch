<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  interface Props {
    text: string;
    class?: string;
    children?: Snippet;
  }

  let { text, class: className, children }: Props = $props();

  let trig = $state<HTMLSpanElement | null>(null);
  let bubble = $state<HTMLDivElement | null>(null);
  let open = $state(false);

  function show(): void {
    if (text) open = true;
  }

  /** 气泡用视口坐标画：页面里全是带 overflow 的滚动容器，挂在原地会被裁掉。 */
  function place(): void {
    if (!trig || !bubble) return;
    const t = trig.getBoundingClientRect();
    const b = bubble.getBoundingClientRect();
    let x = t.left + t.width / 2 - b.width / 2;
    x = Math.min(Math.max(8, x), window.innerWidth - b.width - 8);
    const y = t.top - b.height - 8 > 8 ? t.top - b.height - 8 : t.bottom + 8;
    bubble.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
  }

  $effect(() => {
    if (!open) return;
    place();
    const hide = () => (open = false);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", hide);
    return () => {
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("resize", hide);
    };
  });
</script>

<!-- 外层只是定位盒：真正能读到的内容在 children 里，气泡本体带 role="tooltip"。 -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class={cn("inline-flex", className)}
  bind:this={trig}
  onmouseenter={show}
  onmouseleave={() => (open = false)}
  onfocusin={show}
  onfocusout={() => (open = false)}
>
  {@render children?.()}
</span>

{#if open}
  <div
    bind:this={bubble}
    role="tooltip"
    class="pointer-events-none fixed left-0 top-0 z-50 max-w-[26rem] rounded-md border border-border bg-card px-2.5 py-1.5 text-xs leading-relaxed text-card-foreground shadow-lg"
  >
    {text}
  </div>
{/if}
