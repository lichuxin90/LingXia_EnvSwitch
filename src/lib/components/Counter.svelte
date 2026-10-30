<script lang="ts">
  import { untrack } from "svelte";

  interface Props {
    value: number;
    /** 数到位的时长；换算法只是让数字别硬跳。 */
    duration?: number;
  }

  let { value, duration = 700 }: Props = $props();
  let shown = $state(0);

  $effect(() => {
    // 起点要读当前显示值，但不进依赖：否则自己写自己，effect 永远转不完。
    const from = untrack(() => shown);
    const to = value;
    if (from === to) return;
    const started = performance.now();
    let raf = requestAnimationFrame(function step(now: number) {
      const k = Math.min(1, (now - started) / duration);
      shown = Math.round(from + (to - from) * (1 - Math.pow(1 - k, 3)));
      if (k < 1) raf = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(raf);
  });
</script>

<span class="tabular-nums">{shown}</span>
