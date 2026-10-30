<script lang="ts">
  import { Gauge, RefreshCw, Upload } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Field from "$lib/components/Field.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { ALL_TOOLS, TOOL_LABEL, regionName, sourceName, sourceNote } from "$lib/utils";
  import type { ToolId } from "$lib/types";

  let probing = $state(false);
  const tools = $derived<ToolId[]>(ALL_TOOLS);
  const chosen = $derived(app.settings?.sources[app.tool] ?? "auto");
  const custom = $derived(app.settings?.customBase[app.tool] ?? "");

  /** 这里换工具只是换一个看下载源的对象，既不去版本页，也不重新拉版本清单。 */
  async function chooseTool(tool: ToolId): Promise<void> {
    app.tool = tool;
    app.probes = [];
    await app.refreshMirror();
  }

  async function probe(): Promise<void> {
    probing = true;
    try {
      await app.refreshMirror();
    } finally {
      probing = false;
    }
  }

  function setSource(id: string): void {
    if (!app.settings) return;
    void app.patch({ sources: { ...app.settings.sources, [app.tool]: id } }, t("mir.sourceChanged"));
  }

  function setCustom(value: string): void {
    if (!app.settings) return;
    void app.patch({ customBase: { ...app.settings.customBase, [app.tool]: value } });
  }

  const regionTone = (region: string): "ok" | "accent" | "neutral" =>
    region === "国内" ? "ok" : region === "第三方" ? "accent" : "neutral";
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <h2 class="text-base font-semibold">{t("mir.title")}</h2>
    <Hint text={t("mir.tagline")} />
    <div class="ml-auto flex gap-2">
      <Button variant="outline" size="sm" onclick={probe}>
        <Gauge class="size-3.5 {probing ? 'animate-spin' : ''}" />
        {t("mir.reprobe")}
      </Button>
      <Button variant="outline" size="sm" title={t("mir.pickOfflineTip")} onclick={() => void app.pickOffline()}>
        <Upload class="size-3.5" />
        {t("mir.importOffline")}
      </Button>
    </div>
  </header>

  <div class="space-y-5 px-6 py-5">
    <Segmented
      fill
      ariaLabel={t("mir.pickToolAria")}
      value={app.tool}
      items={tools.map((tool) => ({ value: tool, label: TOOL_LABEL[tool], hint: t("mir.pickToolHint") }))}
      onchange={(next) => void chooseTool(next as ToolId)}
    />

    <Section title={t("mir.sourcesTitle", { tool: TOOL_LABEL[app.tool] })} desc={t("mir.sourcesDesc")}>
      <ul class="divide-y divide-border">
        <li class="flex items-center gap-3 py-2 text-xs">
          <Badge tone="accent">{t("mir.auto")}</Badge>
          <span class="min-w-0 flex-1">{t("mir.autoDesc")}</span>
          <Button size="sm" variant={chosen === "auto" ? "default" : "outline"} onclick={() => setSource("auto")}>
            {chosen === "auto" ? t("common.inUse") : t("mir.useAuto")}
          </Button>
        </li>

        {#each app.sources as s (s.id)}
          {@const probe = app.probes.find((p) => p.id === s.id)}
          <li class="flex items-start gap-3 py-2 text-xs">
            <Badge tone={regionTone(s.region)}>{regionName(s.region)}</Badge>
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium">{sourceName(app.tool, s.id, s.label)}</p>
              <p class="mono mt-0.5 break-all text-muted-foreground">{s.base}</p>
              {#if s.wrap}
                <p class="mono mt-0.5 break-all text-muted-foreground">{t("mir.relay", { url: s.wrap })}</p>
              {/if}
              {#if s.note}
                <p class="mt-0.5 text-muted-foreground">{sourceNote(app.tool, s.id, s.note)}</p>
              {/if}
            </div>
            <div class="w-28 shrink-0 text-right">
              {#if probe}
                {#if probe.ok}
                  <span class="mono">{t("mir.latency", { n: probe.latencyMs ?? 0 })}</span>
                {:else}
                  <span class="text-destructive">{t("mir.unreachable")}</span>
                {/if}
              {:else}
                <span class="text-muted-foreground">{t("mir.notProbed")}</span>
              {/if}
            </div>
            <div class="w-24 shrink-0 text-right">
              <Button
                size="sm"
                variant={chosen === s.id ? "default" : "outline"}
                onclick={() => setSource(s.id)}
              >{chosen === s.id ? t("common.inUse") : t("mir.useIt")}</Button>
            </div>
          </li>
          {#if probe?.error}
            <li class="pb-2 pl-14 text-xs text-destructive">{probe.error}</li>
          {/if}
        {/each}
      </ul>

      <div class="mt-3 border-t border-border pt-3">
        <Field label={t("mir.manualBase")} hint={t("mir.manualBaseHint")}>
          <input
            class="mono h-8 w-72 rounded-md border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring/50"
            placeholder={t("mir.manualPlaceholder")}
            aria-label={t("mir.manualBaseAria", { tool: TOOL_LABEL[app.tool] })}
            value={custom}
            onchange={(e) => setCustom((e.currentTarget as HTMLInputElement).value.trim())}
          />
        </Field>
      </div>
    </Section>

    <Section title={t("mir.proxyTitle")} desc={t("mir.proxyDesc")}>
      <div class="pt-1">
        <p class="flex items-center gap-1.5 text-sm font-medium">
          {t("mir.proxyMode")}
          <Hint text={t("mir.proxyModeDesc")} />
        </p>
        <Segmented
          class="mt-3"
          fill
          ariaLabel={t("mir.proxyMode")}
          value={app.settings?.proxyMode ?? "system"}
          items={[
            { value: "system", label: t("mir.proxySystem") },
            { value: "direct", label: t("mir.proxyDirect") },
            { value: "custom", label: t("mir.proxyCustom") },
          ]}
          onchange={(next) => void app.patch({ proxyMode: next as "system" })}
        />
      </div>
      {#if app.settings?.proxyMode === "custom"}
        <Field label={t("mir.proxyUrl")} hint={t("mir.proxyUrlHint")}>
          <div class="flex items-center gap-2">
            <input
              class="mono h-8 w-72 rounded-md border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring/50"
              placeholder="http://127.0.0.1:7890"
              aria-label={t("mir.proxyUrl")}
              value={app.settings.proxyUrl ?? ""}
              onchange={(e) => void app.patch({ proxyUrl: (e.currentTarget as HTMLInputElement).value.trim() })}
            />
            <Button
              size="sm"
              variant="outline"
              onclick={async () => {
                try {
                  const note = await api.testProxy("custom", app.settings?.proxyUrl ?? null);
                  app.toast(note, "ok", 7000);
                } catch (e) {
                  app.fail(e, t("mir.proxyTestFailed"));
                }
              }}
            >{t("mir.proxyTest")}</Button>
          </div>
        </Field>
      {/if}
    </Section>

    <Section title={t("mir.dlTitle")} desc={t("mir.dlDesc")}>
      <Field label={t("mir.conns")} hint={t("mir.connsHint")}>
        <input
          type="number"
          min="1"
          max="8"
          class="mono h-8 w-20 rounded-md border border-input bg-card px-2 text-xs"
          aria-label={t("mir.conns")}
          value={app.settings?.downloadConns ?? 4}
          onchange={(e) => {
            const n = Math.min(8, Math.max(1, Number((e.currentTarget as HTMLInputElement).value) || 1));
            void app.patch({ downloadConns: n });
          }}
        />
      </Field>
      <Field label={t("mir.keepArchives")} hint={t("mir.keepArchivesHint")}>
        <Switch
          label={t("mir.keepSwitch")}
          checked={app.settings?.keepArchives ?? false}
          onchange={(next) => void app.patch({ keepArchives: next })}
        />
      </Field>
    </Section>

    <Section title={t("mir.offlineTitle")} desc={`${t("mir.offlineDesc")} ${t("mir.offlineBody")}`}>
      <div class="flex gap-2">
        <Button size="sm" variant="outline" onclick={() => void app.pickOffline()}>
          <RefreshCw class="size-3.5" />
          {t("mir.pickInstallers")}
        </Button>
      </div>
    </Section>
  </div>
</div>
