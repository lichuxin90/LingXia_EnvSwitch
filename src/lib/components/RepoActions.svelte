<script lang="ts">
  import { Download, Globe, History, RefreshCw, Star } from "@lucide/svelte";
  import Button from "$lib/components/Button.svelte";
  import GitHubMark from "$lib/components/GitHubMark.svelte";
  import { api, messageOf } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { REPO_URL, SITE_URL } from "$lib/utils";
  import type { UpdateInfo } from "$lib/types";

  let info = $state<UpdateInfo | null>(null);
  let busy = $state(false);
  let failed = $state("");

  async function check(manual = true): Promise<void> {
    busy = true;
    failed = "";
    try {
      info = await api.checkUpdate();
    } catch (e) {
      info = null;
      // 自动那一次失败不值得弹提示：没网的时候首页不该先红一行字给人看。
      if (manual) failed = messageOf(e);
    } finally {
      busy = false;
    }
  }

  /** 进页面先悄悄问一次，有更新就直接把按钮变成深色那颗，不用再手点。 */
  let asked = $state(false);
  $effect(() => {
    if (asked) return;
    asked = true;
    void check(false);
  });

  const release = $derived(info && info.hasUpdate ? info : null);
  const status = $derived(
    busy
      ? t("repo.checking")
      : failed
        ? t("repo.failed", { reason: failed })
        : info
          ? info.hasUpdate
            ? t("repo.found", { latest: info.latest, current: info.current })
            : t("repo.latest", { current: info.current })
          : "",
  );
</script>

<!-- 一颗会转的星星 + 仓库入口：首页单独占一个区域，关于页也复用同一份。 -->
<div class="flex flex-wrap items-center gap-4">
  <div class="min-w-0 flex-1">
    <p class="text-sm font-semibold">{t("repo.starTitle")}</p>
    <p class="mt-0.5 text-xs leading-relaxed text-muted-foreground">{t("repo.starHint")}</p>
    {#if status}
      <p class="mono mt-1 text-xs {failed ? "text-destructive" : release ? "text-warning" : "text-muted-foreground"}">
        {status}
      </p>
    {/if}
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <!-- 地址不写进 title：鼠标停上去只看到按钮自己的名字。 -->
    <!-- 星星挪到按钮里面转：以前它在文字左边单独占一格，跟按钮各转各的。 -->
    <Button
      size="sm"
      class="border border-primary/25 bg-primary/10 text-primary elevate hover:bg-primary/15"
      onclick={() => void app.openLink(REPO_URL)}
    >
      <GitHubMark class="size-3.5" />
      {t("repo.starCta")}
      <Star class="size-3.5 fill-warning text-warning animate-[spin_4s_linear_infinite]" />
    </Button>
    <Button size="sm" variant="outline" onclick={() => void app.openLink(SITE_URL)}>
      <Globe class="size-3.5" />
      {t("repo.site")}
    </Button>
    <Button size="sm" variant="outline" onclick={() => void app.openLink(`${REPO_URL}/releases`)}>
      <History class="size-3.5" />
      {t("repo.changelog")}
    </Button>
    {#if release}
      <!-- 有新版就把这颗按钮涂深色，字直接写"更新到 vX"：点下去是发布页，不是再问一次。 -->
      <Button size="sm" onclick={() => void app.openLink(release.url)}>
        <Download class="size-3.5" />
        {t("repo.updateTo", { latest: release.latest })}
      </Button>
    {:else}
      <Button size="sm" variant="outline" disabled={busy} onclick={() => void check()}>
        <RefreshCw class="size-3.5 {busy ? 'animate-spin' : ''}" />
        {t("repo.checkUpdate")}
      </Button>
    {/if}
  </div>
</div>
