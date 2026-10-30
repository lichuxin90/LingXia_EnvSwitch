<script lang="ts">
  import { BookOpen, ExternalLink } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import RepoActions from "$lib/components/RepoActions.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { osName, TOOL_LABEL } from "$lib/utils";

  const links = $derived([
    { label: t("about.linkNode"), url: "https://nodejs.org/en/download/releases", note: t("about.linkNodeNote") },
    { label: t("about.linkNpm"), url: "https://npmmirror.com/mirrors/node/", note: t("about.linkNpmNote") },
    {
      label: t("about.linkPy"),
      url: "https://github.com/astral-sh/python-build-standalone/releases",
      note: t("about.linkPyNote"),
    },
    { label: t("about.linkAsdf"), url: "https://asdf-vm.com/manage/configuration.html", note: t("about.linkAsdfNote") },
  ]);

  const facts = $derived(
    app.boot
      ? [
          [t("about.factVersion"), app.boot.version],
          [t("about.factPlatform"), osName(app.boot.platform)],
          [t("about.factPrivilege"), app.boot.elevated ? t("about.admin") : t("about.normalUser")],
          [t("nav.dataDir"), app.boot.layout?.root ?? t("common.unknown")],
          [
            t("about.factShims"),
            t("about.shimsValue", {
              ok: app.boot.shims.filter((s) => s.exists).length,
              total: app.boot.shims.length,
            }),
          ],
          [
            t("about.factHooks"),
            t("about.hooksValue", {
              ok: app.boot.hooks.filter((h) => h.installed).length,
              total: app.boot.hooks.length,
            }),
          ],
        ]
      : [],
  );
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <h2 class="text-base font-semibold">{t("nav.about")}</h2>
    <Hint text={t("about.tagline")} />
  </header>

  <div class="space-y-5 px-6 py-5">
    <Section>
      <div class="flex items-start gap-4">
        <img src="/logo.png" alt="" class="size-14 shrink-0 rounded-xl object-contain" />
        <div class="min-w-0">
          <p class="text-sm font-semibold">灵匣·EnvSwitch</p>
          <p class="mt-1 text-xs leading-relaxed text-muted-foreground">{t("about.blurb")}</p>
          <div class="mt-2 flex flex-wrap gap-1.5">
            {#each Object.keys(TOOL_LABEL) as code (code)}
              <Badge tone="outline">{t("about.supported", { tool: TOOL_LABEL[code as "node"] })}</Badge>
            {/each}
            <Badge tone="neutral">{t("about.planned")}</Badge>
          </div>
        </div>
      </div>
    </Section>

    <Section title={t("repo.title")} desc={t("about.repo")} dense>
      <RepoActions />
    </Section>

    <Section title={t("about.howTitle")} dense>
      <ol class="space-y-1.5 text-xs leading-relaxed text-muted-foreground">
        <li>{t("about.how1")}</li>
        <li>{t("about.how2")}</li>
        <li>{t("about.how3")}</li>
        <li>{t("about.how4")}</li>
      </ol>
    </Section>

    <Section title={t("about.runtimeTitle")}>
      <ul class="divide-y divide-border">
        {#each facts as [k, v] (k)}
          <li class="flex items-center justify-between gap-3 py-1.5 text-xs">
            <span class="shrink-0 text-muted-foreground">{k}</span>
            <span class="mono min-w-0 flex-1 truncate text-right" title={v}>{v}</span>
          </li>
        {/each}
      </ul>
    </Section>

    <Section title={t("about.sourcesTitle")} dense>
      <ul class="space-y-1">
        {#each links as item (item.url)}
          <li class="flex items-center gap-2 text-xs">
            <BookOpen class="size-3.5 shrink-0 text-muted-foreground" />
            <span class="w-40 shrink-0">{item.label}</span>
            <span class="min-w-0 flex-1 truncate text-muted-foreground">{item.note}</span>
            <Button size="sm" variant="ghost" title={t("about.openInBrowser")} onclick={() => void app.openLink(item.url)}>
              <ExternalLink class="size-3.5" />
            </Button>
          </li>
        {/each}
      </ul>
    </Section>
  </div>
</div>
