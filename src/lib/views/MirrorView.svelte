<script lang="ts">
  import { onMount } from "svelte";
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
  import type { PackageProbe, PackageSource } from "$lib/types";

  let region = $state("");
  void api.sourceRegion().then((r) => region = r).catch(() => {});
  let runtimeBusy = $state(false);
  let probeGeneration = 0;
  let packageProbes = $state<Record<string, PackageProbe[]>>({});
  let packageSources = $state<Record<string, PackageSource[]>>({});
  let packageBusy = $state<Record<string, boolean>>({});
  type Manager = "pip" | "npm" | "composer" | "deno" | "deno-jsr";
  const managerLabel: Record<Manager, string> = { pip: "pip", npm: "npm / Bun", composer: "Composer", deno: "npm", "deno-jsr": "JSR" };
  const managersByTool: Partial<Record<ToolId, Manager[]>> = { node: ["npm"], python: ["pip"], php: ["composer"], bun: ["npm"], deno: ["deno", "deno-jsr"] };
  const managers = $derived(managersByTool[app.tool] ?? []);
  let sourceKind = $state<"runtime" | Manager>("runtime");
  const probing = $derived(sourceKind === "runtime" ? runtimeBusy : !!packageBusy[sourceKind]);
  const tools = $derived<ToolId[]>(ALL_TOOLS);
  const chosen = $derived(app.settings?.sources[app.tool] ?? "auto");
  const custom = $derived(app.settings?.customBase[app.tool] ?? "");

  /** 这里换工具只是换一个看下载源的对象，既不去版本页，也不重新拉版本清单。 */
  async function chooseTool(tool: ToolId): Promise<void> {
    app.tool = tool;
    app.sources = [];
    app.probes = [];
    sourceKind = "runtime";
    await probe();
  }

  async function probe(): Promise<void> {
    if (sourceKind !== "runtime") return probePackages(sourceKind);
    const generation = ++probeGeneration;
    runtimeBusy = true;
    try { await app.refreshMirror(); }
    finally { if (probeGeneration === generation) runtimeBusy = false; }
  }

  onMount(() => { void probe(); });

  function chooseKind(next: string): void {
    sourceKind = next as typeof sourceKind;
    if (sourceKind !== "runtime" && !packageSources[sourceKind]) void probe();
  }

  async function probePackages(manager: Manager): Promise<void> {
    if (packageBusy[manager]) return;
    packageBusy[manager] = true;
    try {
      packageSources[manager] = await api.packageSources(manager);
      packageProbes[manager] = await api.probePackageSources(manager);
    } catch (e) {
      app.fail(e, t("mir.packageProbeFailed"));
    } finally {
      packageBusy[manager] = false;
    }
  }

  function choosePackageSource(manager: string, id: string): void {
    if (!app.settings) return;
    void app.patch({ packageSources: { ...app.settings.packageSources, [manager]: id } }, t("mir.packageSourceChanged"));
  }

  function setPackageUrl(manager: string, url: string): void {
    if (!app.settings) return;
    const packageSources = { ...app.settings.packageSources, [manager]: url ? `url:${url}` : "auto" };
    void app.patch({ packageSources });
  }

  function fastestPackage(manager: string): string | undefined {
    return packageProbes[manager]?.filter((p) => p.ok && p.latencyMs != null && packageSources[manager]?.some((s) => s.id === p.id))
      .sort((a, b) => a.latencyMs! - b.latencyMs!)[0]?.id;
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
      <Button variant="outline" size="sm" disabled={probing} onclick={probe}>
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

    {#if managers.length}
      <Segmented ariaLabel={t("mir.sourcePurpose")} value={sourceKind} items={[
        { value: "runtime", label: t("mir.runtimeArchive") },
        ...managers.map((manager) => ({ value: manager, label: t("mir.dependencies", { name: managerLabel[manager] }) })),
      ]} onchange={chooseKind} />
    {/if}

    <Section title={sourceKind === "runtime" ? t("mir.sourcesTitle", { tool: TOOL_LABEL[app.tool] }) : t("mir.dependencies", { name: managerLabel[sourceKind] })} desc={t("mir.sourcesDesc")}>
      {#if sourceKind === "runtime"}
        <ul class="divide-y divide-border">
          <li class="flex items-center gap-3 py-2 text-xs">
            <Badge tone="accent">{t("mir.auto")}</Badge>
            <span class="min-w-0 flex-1 leading-relaxed">{t("mir.autoDesc")}{#if region}<span class="mt-1 block text-muted-foreground">{t("mir.region", { region })}</span>{/if}</span>
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
      {:else}
        {@const manager = sourceKind}
        {@const chosenPackage = app.settings?.packageSources?.[manager] ?? "auto"}
        <div class="py-1">
          <ul class="divide-y divide-border">
            <li class="flex items-center gap-3 py-2 text-xs">
              <Badge tone="accent">{t("mir.auto")}</Badge>
              <span class="min-w-0 flex-1 text-muted-foreground">{t("mir.packageAuto")}</span>
              <span class="mono w-24 text-right text-muted-foreground">{#if packageProbes[manager]?.length}{packageProbes[manager].filter((p) => p.ok).length} {t("mir.packageAvailable")}{:else}{t("mir.notProbed")}{/if}</span>
              <Button size="sm" variant={chosenPackage === "auto" ? "default" : "outline"} onclick={() => choosePackageSource(manager, "auto")}>{chosenPackage === "auto" ? t("common.inUse") : t("mir.useAuto")}</Button>
            </li>
            {#each packageSources[manager] ?? [] as source (source.id)}
              {@const result = packageProbes[manager]?.find((p) => p.id === source.id)}
              <li class="flex items-center gap-3 py-2 text-xs">
                <span class="min-w-0 flex-1"><span class="font-medium">{source.label}</span>{#if chosenPackage === "auto" && fastestPackage(manager) === source.id}<span class="ml-2"><Badge tone="ok">{t("mir.fastest")}</Badge></span>{/if}<span class="mono mt-0.5 block break-all text-muted-foreground">{source.url}</span></span>
                <span class="mono w-24 text-right" title={result?.error ?? undefined} class:text-destructive={result && !result.ok}>{#if result?.ok}{t("mir.latency", { n: result.latencyMs ?? 0 })}{:else if result}{t("mir.unreachable")}{:else}{t("mir.notProbed")}{/if}</span>
                <Button size="sm" variant={chosenPackage === source.id ? "default" : "outline"} onclick={() => choosePackageSource(manager, source.id)}>{chosenPackage === source.id ? t("common.inUse") : t("mir.useIt")}</Button>
              </li>
            {/each}
          </ul>
          <Field label={t("mir.packageCustom")}>
            <input class="mono h-8 w-full max-w-xl rounded-md border border-input bg-card px-2 text-xs" value={chosenPackage.startsWith("url:") ? chosenPackage.slice(4) : ""} placeholder={t("mir.packageCustomPlaceholder")} onchange={(e) => setPackageUrl(manager, (e.currentTarget as HTMLInputElement).value.trim())} />
          </Field>
        </div>
      {/if}
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
