<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { cn } from "$lib/utils";

  type Variant = "default" | "secondary" | "outline" | "ghost" | "danger" | "link";
  type Size = "sm" | "md" | "lg" | "icon";

  interface Props extends HTMLButtonAttributes {
    variant?: Variant;
    size?: Size;
    children: Snippet;
  }

  let {
    variant = "default",
    size = "md",
    class: className,
    type = "button",
    disabled = false,
    title,
    onclick,
    children,
    ...rest
  }: Props = $props();

  const base =
    "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors " +
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background " +
    "disabled:pointer-events-none disabled:opacity-45";

  const variants = {
    default: "bg-primary text-primary-foreground elevate hover:bg-primary/90",
    secondary: "bg-muted text-foreground hover:bg-muted/70",
    outline: "border border-input bg-card elevate hover:bg-accent hover:text-accent-foreground",
    ghost: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
    danger: "border border-destructive/30 bg-destructive/8 text-destructive hover:bg-destructive/15",
    link: "text-primary underline-offset-4 hover:underline",
  };

  const sizes = {
    sm: "h-8 px-3 text-xs",
    md: "h-9 px-3.5 text-sm",
    lg: "h-10 px-4 text-sm",
    icon: "size-9",
  };
</script>

<button
  {type}
  {disabled}
  {title}
  onclick={onclick}
  class={cn(base, variants[variant], sizes[size], className)}
  {...rest}
>
  {@render children()}
</button>
