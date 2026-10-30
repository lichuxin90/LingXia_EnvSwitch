<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { ChevronDown } from "@lucide/svelte";
  import Hint from "$lib/components/Hint.svelte";
  import { cn } from "$lib/utils";

  interface Props {
    title?: string;
    /** 折叠面板的说明直接显示在标题下面；不折叠的分区仍收在 ⓘ 气泡里。 */
    desc?: string;
    /** 标题前的图标：一屏好几块面板时，靠图形比靠读字快。 */
    icon?: Component;
    /** 折叠面板：默认收起，点标题才展开，长页面因此能一屏看完。 */
    collapsible?: boolean;
    defaultOpen?: boolean;
    dense?: boolean;
    class?: string;
    /** 标题行右侧的常驻内容（状态、按钮）。 */
    trailing?: Snippet;
    children?: Snippet;
  }

  let {
    title,
    desc,
    icon,
    collapsible = false,
    defaultOpen = false,
    dense = false,
    class: className,
    trailing,
    children,
  }: Props = $props();

  // 折叠状态只拿 defaultOpen 当初始值，之后归本地管。
  // svelte-ignore state_referenced_locally
  let open = $state(defaultOpen);

  const Icon = $derived(icon);

  function toggle(): void {
    open = !open;
  }
</script>

<!-- Clean Admin 的分区卡：1px 边框负责分隔，阴影只留一层极淡的落地。 -->
<section
  class={cn(
    "rounded-lg border border-border bg-card elevate",
    dense ? "p-4" : "p-5",
    className,
  )}
>
  {#if title || desc || trailing}
    <div class="flex items-start gap-2 {collapsible || !children ? "" : "mb-3"}">
      {#if collapsible}
        <button
          type="button"
          class="flex min-w-0 flex-1 items-start gap-2 rounded-md py-0.5 text-left transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          aria-expanded={open}
          onclick={toggle}
        >
          {#if Icon}
            <Icon class="mt-1 size-4 shrink-0 text-primary" aria-hidden="true" />
          {/if}
          <span class="min-w-0 flex-1">
            {#if title}<span class="block text-base font-semibold">{title}</span>{/if}
            {#if desc}
              <span class="mt-0.5 block text-xs leading-relaxed text-muted-foreground">{desc}</span>
            {/if}
          </span>
          <ChevronDown class="mt-1.5 size-4 shrink-0 text-muted-foreground transition-transform duration-200 {open ? "" : "-rotate-90"}" />
        </button>
      {:else}
        {#if Icon}
          <Icon class="mt-1 size-4 shrink-0 text-primary" aria-hidden="true" />
        {/if}
        {#if title}<h3 class="min-w-0 text-base font-semibold">{title}</h3>{/if}
        {#if desc}<Hint text={desc} />{/if}
        <span class="min-w-0 flex-1"></span>
      {/if}
      {#if trailing}
        <span class="flex shrink-0 items-center gap-2">{@render trailing()}</span>
      {/if}
    </div>
  {/if}
  {#if children}
    {#if collapsible}
      <!-- 上边距留在裁剪框里面：以前它随 open 瞬间增减，动画两头各跳一下，看着就是"页面晃"。 -->
      <div class="grid transition-[grid-template-rows] duration-300 ease-out" style="grid-template-rows:{open ? '1fr' : '0fr'}">
        <div class="min-h-0 overflow-hidden">
          <div class="pt-3" aria-hidden={!open} inert={!open}>
            {@render children()}
          </div>
        </div>
      </div>
    {:else}
      {@render children()}
    {/if}
  {/if}
</section>
