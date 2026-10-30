<script lang="ts">
  import {
    BookOpen,
    Bot,
    Boxes,
    Copy,
    FolderOpen,
    Link,
    Link2,
    Plus,
    Radar,
    RefreshCw,
    ScrollText,
    Terminal,
    Trash,
  } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import {
    AI_CLIENT_NAME,
    ALL_TOOLS,
    baseName,
    dirOf,
    envswitchFile,
    fileNameOf,
    joinDir,
    resolutionLabel,
    TOOL_LABEL,
  } from "$lib/utils";
  import type { AiClient, ProjectLock, ToolId } from "$lib/types";

  /** 给 AI 看的规则片段：核心是「别自己下运行时，用本机这套」。 */
  const RULES = $derived(t("ai.rules"));

  const CLI = $derived([
    { cmd: "envswitch status", note: t("ai.cmdStatus") },
    { cmd: "envswitch explain", note: t("ai.cmdExplain") },
    { cmd: "envswitch use node 20", note: t("ai.cmdUse") },
    { cmd: "envswitch unuse node", note: t("ai.cmdUnuse") },
    { cmd: "envswitch global node 22", note: t("ai.cmdGlobal") },
    { cmd: "envswitch shims sync", note: t("ai.cmdShims") },
    { cmd: "envswitch hooks on", note: t("ai.cmdHooks") },
  ]);

  /** 词条里用反引号圈出等宽片段，翻成哪种语言都能保持代码样式。 */
  function segs(line: string): { text: string; code: boolean }[] {
    return line.split("`").map((text, i) => ({ text, code: i % 2 === 1 }));
  }

  let locks = $state<ProjectLock[]>([]);
  let loading = $state(false);
  let registered = $state(false);

  async function reload(): Promise<void> {
    loading = true;
    try {
      await app.refreshDir(app.dirContext?.dir ?? null);
      locks = await api.registeredProjects();
    } catch (e) {
      app.fail(e, t("proj.listFailed"));
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (app.view === "ai") void reload();
  });

  const dir = $derived(app.dirContext?.dir ?? "");
  const rows = $derived(
    ALL_TOOLS.map((tool) => ({
      tool,
      res: (app.dirContext?.resolutions ?? []).find((r) => r.tool === tool) ?? null,
    })),
  );
  const lockEntries = $derived(app.dirContext?.lock?.entries ?? {});
  const howSteps = $derived([t("ai.how1"), t("ai.how2"), t("ai.how3"), t("ai.how4")]);

  /** 把当前解析出来的版本写进项目声明，AI 生成的项目就此固定环境。 */
  async function declareHere(): Promise<void> {
    if (!dir) return;
    const targets = rows.filter((r) => r.res?.version);
    if (!targets.length) {
      app.toast(t("ai.noVersionResolved"), "warn", 7000);
      return;
    }
    try {
      for (const row of targets) await app.lockProject(row.tool, row.res?.version ?? "");
      await api.setWatchDir(dir);
      registered = true;
      await reload();
      app.toast(t("ai.declaredToast", { n: targets.length, dir }), "ok", 8000);
    } catch (e) {
      app.fail(e, t("ai.declareFailed"));
    }
  }

  async function copy(text: string, label: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      app.toast(t("ai.copyDone", { what: label }), "ok", 3000);
    } catch {
      app.toast(t("ai.copyFailed"), "warn", 6000);
    }
  }

  async function forget(path: string): Promise<void> {
    try {
      await api.forgetProject(path);
      await reload();
      app.toast(t("ai.forgotOne"), "ok", 6000);
    } catch (e) {
      app.fail(e, t("ai.forgetFailed"));
    }
  }

  async function pickAndDeclare(): Promise<void> {
    try {
      const picked = await api.pickFolder();
      if (!picked) return;
      await app.refreshDir(picked);
      app.toast(t("ai.pickedDir", { dir: picked }), "info", 7000);
    } catch (e) {
      app.fail(e, t("ai.pickFailed"));
    }
  }

  // ---------------------------------------------------------------- 本机客户端检测与一键导入

  type Scope = "global" | "project";
  type Mode = "solo" | "append";

  /** 一个落点：往哪个目录、写哪个文件、开头必须带什么头部。 */
  interface Spot {
    dir: string;
    file: string;
    head: string;
  }

  interface Row {
    key: string;
    label: string;
    install: string | null;
    config: string | null;
    solo: Spot | null;
    append: Spot | null;
  }

  let clients = $state<AiClient[] | null>(null);
  let detecting = $state(false);
  let importing = $state(false);
  let scope = $state<Scope>("global");
  let chosen = $state<Record<string, boolean>>({});
  let modes = $state<Record<string, Mode>>({});
  let files = $state<Record<string, string>>({});
  let written = $state<Record<string, { kind: Mode; dir: string; file: string }>>({});
  let manualDir = $state("");

  const sep = $derived(t("home.listSep"));

  /** 装在哪个盘只当证据看：写规则靠的是配置目录或项目目录，两条路分开列。 */
  function spotFor(c: AiClient): { solo: Spot | null; append: Spot | null } {
    if (scope === "global") {
      return {
        solo: c.rulesDir
          ? { dir: c.rulesDir, file: envswitchFile(c.rulesSuffix), head: c.rulesHead }
          : null,
        append: c.mainFile ? { dir: dirOf(c.mainFile), file: fileNameOf(c.mainFile), head: "" } : null,
      };
    }
    if (!dir) return { solo: null, append: null };
    return {
      solo: c.projectRules
        ? { dir: joinDir(dir, c.projectRules), file: envswitchFile(c.rulesSuffix), head: c.projectHead }
        : null,
      append: c.projectFile ? { dir, file: c.projectFile, head: "" } : null,
    };
  }

  const clientRows = $derived.by<Row[]>(() => {
    const out: Row[] = (clients ?? [])
      .filter((c) => c.found)
      .map((c) => ({
        key: c.id,
        label: AI_CLIENT_NAME[c.id] ?? c.id,
        install: c.install,
        config: c.path,
        ...spotFor(c),
      }));
    if (manualDir) {
      out.push({
        key: "manual",
        label: t("ai.manualWord"),
        install: manualDir,
        config: manualDir,
        solo: { dir: manualDir, file: "envswitch.md", head: "" },
        append: null,
      });
    }
    return out;
  });

  const missed = $derived((clients ?? []).filter((c) => !c.found).map((c) => AI_CLIENT_NAME[c.id] ?? c.id));

  function modeOf(x: Row): Mode | null {
    const picked = modes[x.key];
    if (picked === "solo" && x.solo) return "solo";
    if (picked === "append" && x.append) return "append";
    if (x.solo) return "solo";
    return x.append ? "append" : null;
  }

  function spotOf(x: Row): Spot | null {
    const m = modeOf(x);
    return m === "solo" ? x.solo : m === "append" ? x.append : null;
  }

  function fileOf(x: Row): string {
    return files[x.key] || spotOf(x)?.file || "";
  }

  function pathOf(x: Row): string {
    const spot = spotOf(x);
    return spot ? joinDir(spot.dir, fileOf(x)) : "";
  }

  function setChosen(key: string, on: boolean): void {
    chosen = { ...chosen, [key]: on };
  }

  function setMode(key: string, m: Mode): void {
    modes = { ...modes, [key]: m };
  }

  function setFile(key: string, name: string): void {
    files = { ...files, [key]: name };
  }

  async function detect(): Promise<void> {
    detecting = true;
    try {
      const list = await api.aiDetect();
      clients = list;
      // 检测完默认把能写的都勾上：大多数人不想再点第二遍。
      const pick: Record<string, boolean> = {};
      for (const c of list) {
        if (!c.found) continue;
        const spots = spotFor(c);
        if (spots.solo || spots.append) pick[c.id] = true;
      }
      chosen = pick;
    } catch (e) {
      app.fail(e, t("ai.detectFailed"));
    } finally {
      detecting = false;
    }
  }

  async function pickManual(): Promise<void> {
    try {
      const picked = await api.pickFolder();
      if (!picked) return;
      manualDir = picked;
      setChosen("manual", true);
    } catch (e) {
      app.fail(e, t("ai.pickFailed"));
    }
  }

  /** 一条一条写：返回成功与否，批量导入也复用它。 */
  async function writeOne(x: Row): Promise<boolean> {
    const kind = modeOf(x);
    const spot = spotOf(x);
    if (!kind || !spot) {
      app.toast(t("ai.noSpotLine", { name: x.label }), "warn", 6000);
      return false;
    }
    const name = fileOf(x);
    try {
      if (kind === "solo") await api.aiWriteFile(spot.dir, name, spot.head, RULES);
      else await api.aiWriteRules(spot.dir, name, RULES);
      written = { ...written, [x.key]: { kind, dir: spot.dir, file: name } };
      return true;
    } catch (e) {
      app.fail(e, t("ai.importFailed", { name: x.label }));
      return false;
    }
  }

  async function importOne(x: Row): Promise<void> {
    if (await writeOne(x)) app.toast(t("ai.wroteOne", { name: x.label }), "ok", 6000);
  }

  async function revokeOne(x: Row): Promise<void> {
    const w = written[x.key];
    if (!w) {
      app.toast(t("ai.revokeNone", { name: x.label }), "warn", 6000);
      return;
    }
    try {
      const path = w.kind === "solo" ? await api.aiRemoveFile(w.dir, w.file) : await api.aiRemoveRules(w.dir, w.file);
      const next = { ...written };
      delete next[x.key];
      written = next;
      app.toast(t("ai.revokeDone", { path }), "ok", 7000);
    } catch (e) {
      app.fail(e, t("ai.revokeFailed", { name: x.label }));
    }
  }

  const pickedRows = $derived(clientRows.filter((x) => chosen[x.key] && modeOf(x)));
  const pickedCount = $derived(pickedRows.length);

  async function importSelected(): Promise<void> {
    if (!pickedRows.length) {
      app.toast(t("ai.importNone"), "warn", 6000);
      return;
    }
    importing = true;
    let done = 0;
    let bad = 0;
    for (const x of pickedRows) {
      if (await writeOne(x)) done += 1;
      else bad += 1;
    }
    importing = false;
    if (bad) app.toast(t("ai.importPartial", { n: done, m: bad }), "warn", 8000);
    else app.toast(t("ai.importDone", { n: done }), "ok", 8000);
  }
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <div class="flex min-w-0 flex-1 items-center gap-2">
      <h2 class="shrink-0 text-base font-semibold">{t("ai.title")}</h2>
      <Hint text={t("ai.tagline")} />
    </div>
    <div class="ml-auto">
      <Button variant="outline" size="sm" onclick={() => void reload()}>
        <RefreshCw class="size-3.5 {loading ? 'animate-spin' : ''}" />
        {t("ai.reload")}
      </Button>
    </div>
  </header>

  <div class="space-y-5 px-6 py-5">
    <Section title={t("ai.howTitle")} desc={t("ai.howDesc")} icon={BookOpen} collapsible>
      <ol class="space-y-1 text-xs leading-relaxed text-muted-foreground">
        {#each howSteps as line (line)}
          <li>
            {#each segs(line) as seg}
              {#if seg.code}<span class="mono">{seg.text}</span>{:else}{seg.text}{/if}
            {/each}
          </li>
        {/each}
      </ol>
    </Section>

    <Section title={t("ai.detectTitle")} desc={t("ai.detectDesc")} icon={Bot}>
      <!-- 动作和范围选择分两行摆：挤在一行时右边那组会换行悬空，整块看着像错位。 -->
      <div class="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={detecting} onclick={() => void detect()}>
          <Radar class="size-3.5 {detecting ? 'animate-spin' : ''}" />
          {detecting ? t("ai.detecting") : t("ai.detectBtn")}
        </Button>
        <Button size="sm" variant="outline" onclick={() => void pickManual()}>
          <FolderOpen class="size-3.5" />
          {t("ai.pickDir")}
        </Button>
        <span class="ml-auto flex items-center gap-2">
          {#if clients}
            <Badge tone={clientRows.length ? "ok" : "warn"}>
              {clientRows.length ? t("ai.detectDone", { n: clientRows.length }) : t("ai.detectNone")}
            </Badge>
          {:else}
            <span class="text-xs text-muted-foreground">{t("ai.pickHint")}</span>
          {/if}
        </span>
      </div>

      <div class="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
        <div class="flex items-center gap-1 rounded-md border border-border p-0.5">
          <Button size="sm" variant={scope === "global" ? "secondary" : "ghost"} onclick={() => (scope = "global")}>
            {t("ai.scopeGlobal")}
          </Button>
          <Button size="sm" variant={scope === "project" ? "secondary" : "ghost"} onclick={() => (scope = "project")}>
            {t("ai.scopeProject")}
          </Button>
        </div>
        <Hint
          text={scope === "global"
            ? t("ai.scopeGlobalDesc")
            : dir
              ? t("ai.scopeProjectDesc", { dir })
              : t("ai.scopeProjectNoDir")}
        />
      </div>

      {#if clientRows.length}
        <ul class="mt-1 divide-y divide-border">
          {#each clientRows as x (x.key)}
            {@const spot = spotOf(x)}
            <li class="py-2.5 text-xs">
              <div class="flex flex-wrap items-center gap-2">
                <label class="flex min-w-0 cursor-pointer items-center gap-2">
                  <input
                    type="checkbox"
                    class="size-4 shrink-0 accent-primary"
                    checked={!!chosen[x.key]}
                    disabled={!spot}
                    onchange={(e) => setChosen(x.key, (e.currentTarget as HTMLInputElement).checked)}
                  />
                  <span class="shrink-0 font-medium">{x.label}</span>
                </label>
                <Badge tone={x.key === "manual" ? "outline" : "ok"}>
                  {x.key === "manual" ? t("ai.manualWord") : t("ai.foundWord")}
                </Badge>
                {#if written[x.key]}
                  <Badge tone="accent">{t("ai.wroteWord")}</Badge>
                {/if}
                <span class="ml-auto flex shrink-0 items-center gap-1">
                  <Button size="sm" variant="outline" disabled={!spot || importing} onclick={() => void importOne(x)}>
                    <Plus class="size-3.5" />
                    {t("ai.importOne")}
                  </Button>
                  {#if written[x.key]}
                    <Button size="sm" variant="ghost" onclick={() => void revokeOne(x)}>
                      <Trash class="size-3.5" />
                      {t("ai.revokeBtn")}
                    </Button>
                  {/if}
                </span>
              </div>

              <div class="mt-1.5 flex flex-wrap items-center gap-2 pl-6">
                {#if x.solo && x.append}
                  <div class="flex items-center gap-1 rounded-md border border-border p-0.5">
                    <Button size="sm" variant={modeOf(x) === "solo" ? "secondary" : "ghost"} onclick={() => setMode(x.key, "solo")}>
                      {t("ai.modeStandalone")}
                    </Button>
                    <Button size="sm" variant={modeOf(x) === "append" ? "secondary" : "ghost"} onclick={() => setMode(x.key, "append")}>
                      {t("ai.modeAppend")}
                    </Button>
                  </div>
                {:else if !spot}
                  <Badge tone="outline">{t("ai.modeNone")}</Badge>
                  <span class="min-w-0 flex-1 leading-relaxed text-muted-foreground">{t("ai.modeNoneDesc")}</span>
                {:else}
                  <span class="shrink-0 text-muted-foreground">
                    {modeOf(x) === "solo" ? t("ai.modeStandalone") : t("ai.modeAppend")}
                  </span>
                {/if}
                {#if spot && modeOf(x) === "solo"}
                  <span class="shrink-0 text-muted-foreground">{t("ai.fileLabel")}</span>
                  <input
                    class="mono h-8 w-40 rounded-md border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring/50"
                    aria-label={`${t("ai.fileLabel")} · ${x.label}`}
                    value={fileOf(x)}
                    onchange={(e) => setFile(x.key, (e.currentTarget as HTMLInputElement).value.trim())}
                  />
                {/if}
                {#if spot}
                  <span class="mono min-w-0 flex-1 truncate text-muted-foreground" title={pathOf(x)}>{pathOf(x)}</span>
                {/if}
              </div>

              {#if modeOf(x) === "append" && x.append}
                <p class="mt-1 pl-6 text-xs leading-relaxed text-muted-foreground">
                  {t("ai.modeAppendDesc", { file: x.append.file })}
                </p>
              {/if}
              {#if x.config}
                <p class="mono mt-1 truncate pl-6 text-xs text-muted-foreground" title={x.config}>
                  {t("ai.configWord")}：{x.config}
                </p>
              {/if}
              {#if x.install && x.install !== x.config}
                <p class="mono mt-0.5 truncate pl-6 text-xs text-muted-foreground" title={x.install}>
                  {t("ai.installWord")}：{x.install}
                </p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if missed.length}
        <p class="mt-2 text-xs leading-relaxed text-muted-foreground">
          {t("ai.missingLine", { names: missed.join(sep) })}
        </p>
      {/if}

      {#if clients}
        <div class="mt-3 flex flex-wrap items-center gap-3 border-t border-border pt-3">
          <Button size="sm" disabled={!pickedCount || importing} onclick={() => void importSelected()}>
            <Plus class="size-3.5" />
            {importing ? t("ai.importing") : t("ai.importBtn")}
          </Button>
          <Hint text={t("ai.detectFoot")} />
        </div>
      {/if}
    </Section>

    <Section title={t("ai.curTitle")} desc={t("ai.curDesc")} icon={Boxes} collapsible>
      <p class="mono mb-2 break-all text-xs text-muted-foreground">{t("ai.dirLine", { dir: dir || t("common.unselected") })}</p>
      <ul class="divide-y divide-border">
        {#each rows as { tool, res } (tool)}
          <li class="flex flex-wrap items-center gap-2 py-2 text-xs">
            <span class="w-16 shrink-0 font-medium">{TOOL_LABEL[tool]}</span>
            <span class="mono w-32 shrink-0">{res?.version ?? t("ai.notSet")}</span>
            {#if res}
              <Badge tone={res.source === "projectFile" ? "accent" : res.version ? "ok" : "neutral"}>
                {resolutionLabel(res.source)}
              </Badge>
            {/if}
            {#if res?.foundIn}
              <span class="mono min-w-0 flex-1 truncate text-muted-foreground" title={res.foundIn}>{res.foundIn}</span>
            {:else}
              <span class="min-w-0 flex-1 truncate text-muted-foreground">
                {#if tool === "pypy"}
                  {t("ai.pypyNote")}
                {:else if res?.version}
                  {t("ai.execPath", { path: res.execPath ?? t("ai.execMissing") })}
                {:else}
                  {t("ai.noDeclare")}
                {/if}
              </span>
            {/if}
            {#if res?.version}
              <Button
                size="sm"
                variant="outline"
                disabled={!!lockEntries[tool]}
                onclick={() => void app.lockProject(tool, res?.version ?? "")}
              >{lockEntries[tool] ? t("ai.declared") : t("ai.lockHere")}</Button>
            {:else}
              <Button size="sm" variant="ghost" onclick={() => void app.setTool(tool)}>{t("ai.installOne")}</Button>
            {/if}
          </li>
          {#if res?.note}
            <li class="pb-2 text-xs text-warning">{res.note}</li>
          {/if}
        {/each}
      </ul>
      <div class="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
        <Button size="sm" onclick={() => void declareHere()} disabled={!dir}>
          <Plus class="size-3.5" />
          {t("ai.declareAll")}
        </Button>
        <Button size="sm" variant="outline" onclick={() => void pickAndDeclare()}>
          <FolderOpen class="size-3.5" />
          {t("ai.pickProject")}
        </Button>
        {#if registered}
          <Badge tone="ok">{t("ai.registered")}</Badge>
        {/if}
        <Hint text={t("ai.declareFoot")} />
      </div>
    </Section>

    <Section title={t("ai.rulesTitle")} desc={t("ai.rulesDesc")} icon={ScrollText} collapsible>
      <div class="relative">
        <pre class="mono max-h-64 overflow-auto rounded-md border border-border bg-detail p-3 text-xs leading-relaxed whitespace-pre-wrap">{RULES}</pre>
        <Button size="sm" variant="outline" class="absolute top-2 right-2" onclick={() => void copy(RULES, t("ai.copyLabelRules"))}>
          <Copy class="size-3.5" />
          {t("ai.copy")}
        </Button>
      </div>
    </Section>

    <Section title={t("ai.cliTitle")} desc={t("ai.cliDesc")} icon={Terminal} collapsible>
      <ul class="divide-y divide-border">
        {#each CLI as item (item.cmd)}
          <li class="flex flex-wrap items-center gap-2 py-1.5 text-xs">
            <code class="mono min-w-0 flex-1 truncate">{item.cmd}</code>
            <span class="w-64 shrink-0 text-muted-foreground">{item.note}</span>
            <Button size="sm" variant="ghost" onclick={() => void copy(item.cmd, t("ai.copyLabelCmd"))}>
              <Copy class="size-3" />
              {t("ai.copy")}
            </Button>
          </li>
        {/each}
      </ul>
      <p class="mt-2 text-xs leading-relaxed text-muted-foreground">
        {#each segs(t("ai.shimNote", { dir: app.boot?.layout?.shims ?? "shims" })) as seg}
          {#if seg.code}<span class="mono">{seg.text}</span>{:else}{seg.text}{/if}
        {/each}
      </p>
    </Section>

    <Section title={t("ai.lockedTitle")} desc={t("ai.lockedDesc")} icon={Link2} collapsible>
      {#if locks.length === 0}
        <p class="py-3 text-center text-xs text-muted-foreground">{t("ai.lockedEmpty")}</p>
      {:else}
        <ul class="divide-y divide-border">
          {#each locks as l (l.dir)}
            <li class="flex flex-wrap items-center gap-2 py-2 text-xs">
              <Link class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="mono min-w-0 flex-1 truncate" title={l.dir}>{baseName(l.dir)}</span>
              <span class="mono w-40 shrink-0 text-muted-foreground" title={Object.entries(l.entries).map(([k, v]) => `${k} ${v}`).join("　")}>
                {Object.entries(l.entries).map(([k, v]) => `${TOOL_LABEL[k as ToolId] ?? k} ${v}`).join(" · ") || t("ai.noEntries")}
              </span>
              <Button size="sm" variant="ghost" title={t("detail.revealHere")} onclick={() => void app.revealPath(l.dir)}>
                <FolderOpen class="size-3.5" />
              </Button>
              <Button size="sm" variant="danger" onclick={() => void forget(l.dir)}>
                <Trash class="size-3.5" />
                {t("ai.forget")}
              </Button>
            </li>
          {/each}
        </ul>
      {/if}
    </Section>
  </div>
</div>
