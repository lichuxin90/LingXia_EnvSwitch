<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    value: number;
    indeterminate?: boolean;
    class?: string;
  }

  let { value, indeterminate = false, class: className }: Props = $props();
</script>

<div
  class={cn("h-2 w-full overflow-hidden rounded-full bg-muted", className)}
  role="progressbar"
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={indeterminate ? undefined : Math.round(value)}
>
  <div
    class={cn("h-full rounded-full bg-primary transition-[width] duration-200", indeterminate && "w-1/3 animate-pulse")}
    style={indeterminate ? undefined : `width: ${Math.max(2, Math.min(100, value))}%`}
  ></div>
</div>
