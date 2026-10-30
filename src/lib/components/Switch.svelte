<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    checked: boolean;
    onchange: (next: boolean) => void;
    disabled?: boolean;
    label: string;
    title?: string;
    class?: string;
  }

  let { checked, onchange, disabled = false, label, title, class: className }: Props = $props();
</script>

<!-- shadcn 的开关键：轨道 44×24，关着是中性实心轨道，开着整条填主色，钮保持 20px 只平移。 -->
<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {title}
  {disabled}
  onclick={() => onchange(!checked)}
  class={cn(
    "relative h-6 w-11 shrink-0 rounded-full border transition-colors",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    checked ? "border-primary bg-primary" : "border-input bg-input",
    className,
  )}
>
  <span
    class={cn(
      "absolute top-1/2 size-5 -translate-y-1/2 rounded-full bg-background shadow-sm transition-all",
      checked ? "left-[22px]" : "left-0.5",
    )}
  ></span>
</button>
