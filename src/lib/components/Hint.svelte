<script lang="ts">
  import { Info } from "@lucide/svelte";
  import Tip from "$lib/components/Tip.svelte";
  import { t } from "$lib/i18n/store.svelte";

  interface Props {
    text: string;
  }

  let { text }: Props = $props();
</script>

<Tip {text}>
  <button
    type="button"
    class="grid size-4 shrink-0 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-label={t("common.hintAria")}
  >
    <Info class="size-3.5" />
  </button>
</Tip>
