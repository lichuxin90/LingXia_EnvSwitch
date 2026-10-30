<script lang="ts">
  import { FolderInput, FolderOpen, Layers, Link2, Plus, RefreshCw, Trash } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Field from "$lib/components/Field.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { ALL_TOOLS, resolutionLabel, TOOL_LABEL } from "$lib/utils";
  import type { InstalledVersion, ToolId } from "$lib/types";

  const ctx = $derived(app.dirContext);
  let projects = $state<Awaited<ReturnType<typeof api.registeredProjects>>>([]);
  let lockTool = $state<ToolId | null>(null);
  let lockCandidates = $state<InstalledVersion[]>([]);
  let manualVersion = $state("");

  async function reloadProjects(): Promise<void> {
    try {
      projects = await api.registeredProjects();
    } catch (e) {
      app.fail(e, t("proj.listFailed"));
    }
  }

  $effect(() => {
    if (app.view === "project") void reloadProjects();
  });

  async function pickDir(): Promise<void> {
    const dir = await api.pickFolder();
    if (dir) await app.refreshDir(dir);
  }

  /** 四个运行时都能锁：点开哪个就只读那个工具的已装版本，不用先切页面。 */
  async function openLock(tool: ToolId): Promise<void> {
    if (lockTool === tool) {
      lockTool = null;
      return;
    }
    lockTool = tool;
    try {
      lockCandidates = (await api.installedVersions(tool, ctx?.dir ?? null)).versions;
    } catch (e) {
      lockCandidates = [];
      app.fail(e, t("proj.installedReadFailed", { tool: TOOL_LABEL[tool] }));
    }
  }

  const entryList = $derived(ctx?.lock ? Object.entries(ctx.lock.entries) : []);
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <h2 class="text-base font-semibold">{t("proj.title")}</h2>
    <Hint text={t("proj.tagline")} />
    <div class="ml-auto flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={pickDir}>
        <FolderInput class="size-3.5" />
        {t("proj.pickAnother")}
      </Button>
      <Button variant="outline" size="sm" title={t("proj.rereadTip")} onclick={() => void app.refreshDir(ctx?.dir ?? null)}>
        <RefreshCw class="size-3.5" />
        {t("proj.reread")}
      </Button>
    </div>
  </header>

  <div class="space-y-5 px-6 py-5">
    <Section>
      <div class="flex items-start gap-3">
        <FolderOpen class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div class="min-w-0 flex-1">
          <p class="mono break-all text-sm">{ctx?.dir ?? t("proj.noDir")}</p>
          {#if ctx && !ctx.exists}
            <p class="mt-1 text-xs text-warning">{t("proj.dirGone")}</p>
          {/if}
          <ul class="mt-2 space-y-1">
            {#each ctx?.resolutions ?? [] as r (r.tool)}
              <li class="flex items-center gap-2 text-xs">
                <span class="w-16 shrink-0">{TOOL_LABEL[r.tool]}</span>
                <span class="mono w-24 shrink-0">{r.version ?? t("proj.notSet")}</span>
                <Badge tone={r.source === "projectFile" ? "accent" : r.source === "none" ? "outline" : "neutral"}>
                  {resolutionLabel(r.source)}
                </Badge>
                <span class="min-w-0 flex-1 truncate text-muted-foreground">{r.foundIn ?? r.note ?? ""}</span>
              </li>
            {/each}
          </ul>
        </div>
      </div>
    </Section>

    <Section title={t("proj.lockTitle")} desc={ctx?.lock ? t("proj.lockWritten", { file: ctx.lock.file }) : t("proj.lockNone")}>
      {#if entryList.length}
        <ul class="mb-3 divide-y divide-border rounded-lg border border-border">
          {#each entryList as [key, ver] (key)}
            {@const tool = (ALL_TOOLS as string[]).includes(key) ? (key as ToolId) : null}
            <li class="flex items-center gap-2 px-2 py-1.5 text-xs">
              <span class="mono w-20 shrink-0">{key}</span>
              <span class="mono min-w-0 flex-1 truncate">{ver}</span>
              {#if tool}
                <Button size="sm" variant="danger" onclick={() => void app.unlockProject(tool)}>
                  <Trash class="size-3.5" />
                  {t("common.cancel")}
                </Button>
              {:else}
                <span class="text-muted-foreground">{t("proj.foreignEntry")}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if ctx?.lock?.extraLines.length}
        <p class="mb-2 text-xs text-muted-foreground">{t("proj.extraLines", { lines: ctx.lock.extraLines.join(" / ") })}</p>
      {/if}

      <div class="flex flex-wrap items-center gap-2">
        {#each ALL_TOOLS as tool (tool)}
          <button
            type="button"
            class="inline-flex items-center gap-1 rounded-md border border-input px-2 py-1 text-xs hover:bg-accent {lockTool === tool
              ? 'bg-accent text-accent-foreground'
              : ''}"
            aria-expanded={lockTool === tool}
            onclick={() => void openLock(tool)}
          >
            <Plus class="size-3.5" />
            {t("proj.lockTool", { tool: TOOL_LABEL[tool] })}
          </button>
        {/each}
      </div>

      {#if lockTool}
        <div class="mt-2 rounded-md border border-border bg-detail px-2 py-2">
          {#if lockCandidates.length}
            <ul class="divide-y divide-border">
              {#each lockCandidates as v (v.version)}
                <li class="flex items-center gap-2 py-1 text-xs">
                  <span class="mono w-28 shrink-0">{v.version}</span>
                  <span class="min-w-0 flex-1 truncate text-muted-foreground">{v.usable ? t("proj.usable") : v.unusableReason ?? t("proj.incomplete")}</span>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={!v.usable}
                    onclick={() => {
                      const tool = lockTool;
                      lockTool = null;
                      if (tool) void app.lockProject(tool, v.version);
                    }}
                  >{t("proj.lockToThis")}</Button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="text-xs text-muted-foreground">{t("proj.noneInstalled", { tool: TOOL_LABEL[lockTool] })}</p>
            <div class="mt-1.5 flex gap-2">
              <input
                class="mono h-8 w-56 rounded-md border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring/50"
                aria-label={t("proj.manualAria", { tool: TOOL_LABEL[lockTool] })}
                placeholder={t("proj.versionPlaceholder")}
                bind:value={manualVersion}
              />
              <Button
                size="sm"
                disabled={!manualVersion.trim()}
                onclick={() => {
                  const tool = lockTool;
                  const ver = manualVersion.trim();
                  if (tool && ver) void app.lockProject(tool, ver);
                  lockTool = null;
                  manualVersion = "";
                }}
              >{t("proj.lockWithThis")}</Button>
            </div>
          {/if}
        </div>
      {/if}
    </Section>

    <Section title={t("nav.lockSwitch")} desc={t("proj.globalDesc")}>
      <Field label={t("proj.globalField")} hint={t("proj.globalHint")}>
        <Switch
          label={t("nav.lockSwitch")}
          checked={app.settings?.lockGlobal ?? false}
          onchange={(next) => void app.patch({ lockGlobal: next }, next ? t("proj.globalOnNote") : t("proj.globalOffNote"))}
        />
      </Field>
    </Section>

    <Section title={t("proj.registeredTitle")} desc={t("proj.registeredDesc")} icon={Link2} collapsible>
      {#if projects.length === 0}
        <p class="py-3 text-center text-xs text-muted-foreground">{t("proj.registeredEmpty")}</p>
      {:else}
        <ul class="divide-y divide-border">
          {#each projects as p (p.dir)}
            <li class="flex items-center gap-2 py-2 text-xs">
              <Link2 class="size-3.5 shrink-0 text-muted-foreground" />
              <button
                type="button"
                class="mono min-w-0 flex-1 truncate text-left hover:underline"
                title={p.dir}
                onclick={() => void app.refreshDir(p.dir)}
              >{p.dir}</button>
              {#each Object.entries(p.entries) as [k, v] (k)}
                <Badge tone="outline">{k} {v}</Badge>
              {/each}
              <Button
                size="sm"
                variant="ghost"
                title={t("proj.forgetTip")}
                onclick={async () => {
                  await api.forgetProject(p.dir);
                  await reloadProjects();
                }}
              >
                <Trash class="size-3.5" />
                {t("proj.forget")}
              </Button>
            </li>
          {/each}
        </ul>
      {/if}
    </Section>

    <!-- 说明沉到最底下：先把能操作的东西放在手边，读解释是最后一步。 -->
    {#if ctx?.summary.length}
      <Section title={t("proj.summaryTitle")} collapsible dense icon={Layers}>
        <ul class="space-y-1 text-xs text-muted-foreground">
          {#each ctx.summary as s (s)}
            <li>{s}</li>
          {/each}
        </ul>
      </Section>
    {/if}
  </div>
</div>
