<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  type Tone = "neutral" | "ok" | "warn" | "bad" | "accent" | "outline";

  interface Props {
    tone?: Tone;
    class?: string;
    children: Snippet;
  }

  let { tone = "neutral", class: className, children }: Props = $props();

  /**
   * 状态色的分工在全站固定：accent 蓝只表示「正在使用」，ok 绿只表示「全局默认 / 健康」，
   * warn 与 bad 留给风险，neutral 与 outline 只是身份或空状态，不承担状态。
   */
  const tones = {
    neutral: "bg-muted text-muted-foreground",
    ok: "bg-success/12 text-success",
    warn: "bg-warning/14 text-warning",
    bad: "bg-destructive/12 text-destructive",
    accent: "bg-primary/12 text-primary",
    outline: "border border-border text-muted-foreground",
  };
</script>

<span
  class={cn(
    "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs leading-none font-medium",
    tones[tone],
    className,
  )}
>
  {@render children()}
</span>
