<script lang="ts">
  import type { ToolId } from "$lib/types";

  interface Props {
    tool: ToolId;
    class?: string;
  }

  let { tool, class: className = "size-4" }: Props = $props();
</script>

<!-- 四个运行时的品牌标记，全部内联 SVG，颜色用各家官方色；选中态由父级决定。 -->
{#if tool === "node"}
  <svg viewBox="0 0 24 24" class={className} aria-hidden="true">
    <path
      d="M12 1.8 20.8 6.9v10.2L12 22.2 3.2 17.1V6.9z"
      fill="none"
      stroke="#539e43"
      stroke-width="1.6"
      stroke-linejoin="round"
    />
    <path
      d="M9 15.6V8.4l6 7.2V8.4"
      fill="none"
      stroke="#539e43"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
{:else if tool === "python"}
  <svg viewBox="0 0 24 24" class={className} aria-hidden="true">
    <path
      d="M9.4 2.6h5.2a2.9 2.9 0 0 1 2.9 2.9v2.4h-5.1a1.1 1.1 0 0 0-1.1 1.1v1.5H6.2a2.9 2.9 0 0 0-2.9 2.9v1.6h5.1v-1.5a1.1 1.1 0 0 1 1.1-1.1h5.1V8.4"
      fill="#3776ab"
    />
    <path
      d="M14.6 21.4H9.4a2.9 2.9 0 0 1-2.9-2.9v-2.4h5.1a1.1 1.1 0 0 0 1.1-1.1v-1.5h5.2a2.9 2.9 0 0 0 2.9-2.9V8.6h-5.1v1.5a1.1 1.1 0 0 1-1.1 1.1H9.6v3.9"
      fill="#ffd43b"
    />
    <circle cx="10.4" cy="5.6" r="0.95" fill="#ffffff" opacity=".9" />
    <circle cx="13.6" cy="18.4" r="0.95" fill="#3b3b3b" opacity=".55" />
  </svg>
{:else if tool === "go"}
  <svg viewBox="0 0 24 24" class={className} aria-hidden="true">
    <path
      d="M6.6 6.4a5.4 5.4 0 0 1 10.8 0v6.4a5.4 5.4 0 0 1-10.8 0z"
      fill="#00add8"
    />
    <circle cx="9.6" cy="7.6" r="1.7" fill="#ffffff" />
    <circle cx="14.4" cy="7.6" r="1.7" fill="#ffffff" />
    <circle cx="9.9" cy="7.7" r=".7" fill="#0b3a4a" />
    <circle cx="14.1" cy="7.7" r=".7" fill="#0b3a4a" />
    <path d="M9.4 12.2h5.2" stroke="#0b3a4a" stroke-width="1.1" stroke-linecap="round" />
    <path d="M4.4 8.6h2.2M17.4 8.6h2.2" stroke="#00add8" stroke-width="1.5" stroke-linecap="round" />
  </svg>
{:else}
  <svg viewBox="0 0 24 24" class={className} aria-hidden="true">
    <circle cx="8.4" cy="9.4" r="3.6" fill="none" stroke="#ff6f1f" stroke-width="1.8" />
    <path d="M12 9.4v6.2a2.4 2.4 0 0 1-4.8 0" fill="none" stroke="#ff6f1f" stroke-width="1.8" stroke-linecap="round" />
    <circle cx="15.6" cy="9.4" r="3.6" fill="none" stroke="#ffb98a" stroke-width="1.8" />
    <path d="M19.2 9.4v6.2a2.4 2.4 0 0 1-4.8 0" fill="none" stroke="#ffb98a" stroke-width="1.8" stroke-linecap="round" />
  </svg>
{/if}
