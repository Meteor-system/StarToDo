<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type WindowMode = 'compact' | 'full';
  type ActionKey = 'snapshot' | 'metrics' | 'database' | 'notifications' | 'testNotification' | 'tray' | 'release' | 'alwaysOnTop';
  type JsonPrimitive = string | number | boolean | null;
  type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
  type JsonRecord = { [key: string]: JsonValue };

  interface RuntimeSnapshot extends JsonRecord {}
  interface ProcessMetrics extends JsonRecord {}
  interface DatabaseProbe extends JsonRecord {}
  interface NotificationDiagnostics extends JsonRecord {}
  interface ActionState { busy: boolean; error: string | null; success: string | null }

  const uiRunId = crypto.randomUUID();
  const emptyAction = (): ActionState => ({ busy: false, error: null, success: null });

  let tauriAvailable = $state(false);
  let initialized = $state(false);
  let mode = $state<WindowMode>('full');
  let alwaysOnTop = $state(false);
  let runtimeSnapshot = $state<RuntimeSnapshot | null>(null);
  let metrics = $state<ProcessMetrics | null>(null);
  let database = $state<DatabaseProbe | null>(null);
  let notifications = $state<NotificationDiagnostics | null>(null);
  let actions = $state<Record<ActionKey, ActionState>>({
    snapshot: emptyAction(), metrics: emptyAction(), database: emptyAction(), notifications: emptyAction(),
    testNotification: emptyAction(), tray: emptyAction(), release: emptyAction(), alwaysOnTop: emptyAction()
  });

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function entries(value: JsonRecord | null): Array<[string, JsonValue]> {
    return value ? Object.entries(value) : [];
  }

  function format(value: JsonValue): string {
    if (value === null) return '—';
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  }

  function setAction(key: ActionKey, next: Partial<ActionState>): void {
    actions = { ...actions, [key]: { ...actions[key], ...next } };
  }

  async function run<T>(key: ActionKey, command: string, args?: Record<string, unknown>): Promise<T | null> {
    if (!tauriAvailable) {
      setAction(key, { error: '当前在浏览器预览环境，Tauri 后端不可用。', success: null });
      return null;
    }
    setAction(key, { busy: true, error: null, success: null });
    try {
      const result = await invoke<T>(command, args);
      setAction(key, { busy: false, success: '已完成。' });
      return result;
    } catch (error) {
      setAction(key, { busy: false, error: errorMessage(error) });
      return null;
    }
  }

  async function runVoid(key: ActionKey, command: string, args?: Record<string, unknown>): Promise<boolean> {
    if (!tauriAvailable) {
      setAction(key, { error: '当前在浏览器预览环境，Tauri 后端不可用。', success: null });
      return false;
    }
    setAction(key, { busy: true, error: null, success: null });
    try {
      await invoke<void>(command, args);
      setAction(key, { busy: false, success: '已完成。' });
      return true;
    } catch (error) {
      setAction(key, { busy: false, error: errorMessage(error) });
      return false;
    }
  }

  async function refreshSnapshot(): Promise<void> {
    const result = await run<RuntimeSnapshot>('snapshot', 'get_runtime_snapshot');
    if (result) runtimeSnapshot = result;
  }

  async function sampleMetrics(): Promise<void> {
    const result = await run<ProcessMetrics>('metrics', 'sample_process_metrics');
    if (result) metrics = result;
  }

  async function probeDatabase(): Promise<void> {
    const result = await run<DatabaseProbe>('database', 'run_database_probe');
    if (result) database = result;
  }

  async function refreshNotifications(): Promise<void> {
    const result = await run<NotificationDiagnostics>('notifications', 'get_notification_diagnostics');
    if (result) notifications = result;
  }

  async function sendTestNotification(): Promise<void> {
    await runVoid('testNotification', 'send_test_notification');
  }

  async function hideToTray(): Promise<void> {
    await runVoid('tray', 'hide_to_tray');
  }

  async function releaseUi(): Promise<void> {
    await runVoid('release', 'release_ui');
  }

  function changeMode(next: WindowMode): void {
    mode = next;
  }

  async function changeAlwaysOnTop(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const enabled = input.checked;
    if (await runVoid('alwaysOnTop', 'set_always_on_top', { alwaysOnTop: enabled })) {
      alwaysOnTop = enabled;
    } else {
      input.checked = alwaysOnTop;
    }
  }

  onMount(() => {
    tauriAvailable = isTauriRuntime();
    initialized = true;
    if (!tauriAvailable) return;

    void (async () => {
      const snapshot = await run<RuntimeSnapshot>('snapshot', 'record_ui_ready');
      if (snapshot) runtimeSnapshot = snapshot;
    })();
  });
</script>

<svelte:head>
  <meta name="theme-color" content="#262a30" />
</svelte:head>

<main class:compact={mode === 'compact'} aria-labelledby="page-title">
  <header class="topbar">
    <div class="identity">
      <span class="mark" aria-hidden="true"></span>
      <div>
        <p class="eyebrow">STAR TODO / 阶段 0</p>
        <h1 id="page-title">验证台</h1>
      </div>
    </div>
    <div class="status" aria-live="polite">
      <span class:offline={!tauriAvailable} class="status-dot"></span>
      {#if !initialized}正在初始化{:else if tauriAvailable}Tauri 已连接{:else}浏览器预览 · 后端不可用{/if}
    </div>
  </header>

  {#if !tauriAvailable && initialized}
    <p class="notice" role="status">此界面可在普通浏览器中安全预览；需要桌面容器才能执行运行时、窗口、SQLite 与通知命令。</p>
  {/if}

  <section class="overview" aria-label="运行概览">
    <div class="run-id"><span>UI RUN</span><code>{uiRunId}</code></div>
    <div class="controls">
      <button class="secondary" onclick={refreshSnapshot} disabled={actions.snapshot.busy}>{actions.snapshot.busy ? '读取中…' : '刷新状态'}</button>
      <div class="segmented" aria-label="窗口模式">
        <button class:active={mode === 'compact'} onclick={() => changeMode('compact')}>紧凑</button>
        <button class:active={mode === 'full'} onclick={() => changeMode('full')}>完整</button>
      </div>
    </div>
  </section>

  {#if actions.snapshot.error}<p class="feedback error" role="alert">状态：{actions.snapshot.error}</p>{/if}
  {#if actions.snapshot.success}<p class="feedback success" role="status">状态：{actions.snapshot.success}</p>{/if}

  <section class="statusline" aria-label="验收门槛">
    <span>验收门槛</span><b>UI Ready</b><i></i><b>进程采样</b><i></i><b>SQLite 探测</b><i></i><b>通知诊断</b>
  </section>

  <section class="grid" aria-label="系统验证项">
    <article class="panel runtime">
      <div class="panel-heading"><div><p class="index">01 / 生命周期</p><h2>运行时快照</h2></div><button class="icon-button" aria-label="刷新运行时快照" onclick={refreshSnapshot} disabled={actions.snapshot.busy}>↻</button></div>
      {#if entries(runtimeSnapshot).length}
        <dl class="metrics">{#each entries(runtimeSnapshot) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>
      {:else}<p class="empty">等待首次运行时快照。</p>{/if}
    </article>

    <article class="panel process">
      <div class="panel-heading"><div><p class="index">02 / 资源指标</p><h2>进程采样</h2></div><button onclick={sampleMetrics} disabled={actions.metrics.busy}>{actions.metrics.busy ? '采样中…' : '采样'}</button></div>
      {#if entries(metrics).length}<dl class="metrics">{#each entries(metrics) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p class="empty">按“采样”读取当前进程指标。</p>{/if}
      {#if actions.metrics.error}<p class="feedback error" role="alert">{actions.metrics.error}</p>{:else if actions.metrics.success}<p class="feedback success">{actions.metrics.success}</p>{/if}
    </article>

    {#if mode === 'full'}
      <article class="panel database">
        <div class="panel-heading"><div><p class="index">03 / 持久化</p><h2>SQLite 探测</h2></div><button onclick={probeDatabase} disabled={actions.database.busy}>{actions.database.busy ? '探测中…' : '运行探测'}</button></div>
        {#if entries(database).length}<dl class="metrics">{#each entries(database) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p class="empty">验证数据库文件、连接与基础读写能力。</p>{/if}
        {#if actions.database.error}<p class="feedback error" role="alert">{actions.database.error}</p>{:else if actions.database.success}<p class="feedback success">{actions.database.success}</p>{/if}
      </article>

      <article class="panel notification">
        <div class="panel-heading"><div><p class="index">04 / 通知</p><h2>通知诊断</h2></div><button onclick={refreshNotifications} disabled={actions.notifications.busy}>{actions.notifications.busy ? '检查中…' : '检查'}</button></div>
        {#if entries(notifications).length}<dl class="metrics">{#each entries(notifications) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p class="empty">读取权限、平台支持与通知服务状态。</p>{/if}
        <div class="inline-actions"><button class="secondary" onclick={sendTestNotification} disabled={actions.testNotification.busy}>{actions.testNotification.busy ? '发送中…' : '发送测试通知'}</button></div>
        {#if actions.notifications.error}<p class="feedback error" role="alert">{actions.notifications.error}</p>{:else if actions.notifications.success}<p class="feedback success">{actions.notifications.success}</p>{/if}
        {#if actions.testNotification.error}<p class="feedback error" role="alert">通知：{actions.testNotification.error}</p>{:else if actions.testNotification.success}<p class="feedback success">通知：{actions.testNotification.success}</p>{/if}
      </article>
    {/if}
  </section>

  <footer class="lifecycle">
    <label class="toggle"><input type="checkbox" checked={alwaysOnTop} onchange={changeAlwaysOnTop} disabled={actions.alwaysOnTop.busy} /><span aria-hidden="true"></span>始终置顶</label>
    <div class="footer-actions"><button class="secondary" onclick={hideToTray} disabled={actions.tray.busy}>{actions.tray.busy ? '处理中…' : '隐藏至托盘'}</button><button class="danger" onclick={releaseUi} disabled={actions.release.busy}>{actions.release.busy ? '释放中…' : '释放 UI'}</button></div>
  </footer>
  {#if actions.alwaysOnTop.error || actions.tray.error || actions.release.error}<p class="feedback error footer-feedback" role="alert">{actions.alwaysOnTop.error ?? actions.tray.error ?? actions.release.error}</p>{:else if actions.alwaysOnTop.success || actions.tray.success || actions.release.success}<p class="feedback success footer-feedback">{actions.alwaysOnTop.success ?? actions.tray.success ?? actions.release.success}</p>{/if}
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) { margin: 0; min-width: 320px; background: #1d2025; color: #edf1f6; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
  :global(button), :global(input) { font: inherit; }
  main { --blue: #5ba9ff; --line: rgba(233, 240, 249, .15); --muted: #a9b2bd; width: min(100%, 920px); min-height: 100vh; margin: 0 auto; padding: 22px clamp(16px, 4vw, 36px) 18px; background: linear-gradient(135deg, rgba(71,76,85,.74), rgba(31,34,40,.88)); border-inline: 1px solid rgba(255,255,255,.07); }
  .topbar,.overview,.panel-heading,.lifecycle,.controls,.inline-actions { display:flex; align-items:center; justify-content:space-between; gap:14px; }
  .topbar { padding-bottom:18px; border-bottom:1px solid var(--line); } .identity { display:flex; align-items:center; gap:11px; } .mark { width:9px; height:9px; border-radius:50%; background:var(--blue); box-shadow:0 0 17px rgba(91,169,255,.75); } .eyebrow,.index { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h1,h2 { margin:3px 0 0; font-weight:600; } h1 { font-size:20px; } h2 { font-size:16px; } .status { display:flex; align-items:center; gap:7px; color:var(--muted); font-size:12px; text-align:right; }.status-dot { width:7px; height:7px; border-radius:50%; background:#76dba2; }.status-dot.offline { background:#e0ad65; }
  .notice,.feedback { margin:12px 0 0; font-size:12px; line-height:1.5; }.notice { padding:9px 11px; color:#d9c08f; background:rgba(224,173,101,.09); border-left:2px solid #e0ad65; }.overview { padding:17px 0; }.run-id { display:grid; gap:4px; min-width:0; }.run-id span { color:var(--muted); font-size:10px; letter-spacing:.12em; }.run-id code { overflow:hidden; color:#cbd5e1; font-size:11px; text-overflow:ellipsis; white-space:nowrap; }.controls { flex-wrap:wrap; justify-content:flex-end; }
  button { border:1px solid rgba(91,169,255,.6); border-radius:5px; padding:7px 10px; color:#07111f; background:var(--blue); font-size:12px; font-weight:650; cursor:pointer; transition:background .15s, opacity .15s, transform .15s; } button:hover:not(:disabled) { background:#82beff; } button:active:not(:disabled) { transform:translateY(1px); } button:disabled { cursor:not-allowed; opacity:.52; }.secondary { color:#e5ecf5; background:rgba(255,255,255,.06); border-color:var(--line); }.secondary:hover:not(:disabled) { background:rgba(91,169,255,.16); border-color:var(--blue); }.danger { color:#ffcbcb; background:transparent; border-color:rgba(255,151,151,.48); }.danger:hover:not(:disabled) { background:rgba(255,100,100,.12); }.segmented { display:flex; padding:2px; background:rgba(0,0,0,.18); border:1px solid var(--line); border-radius:6px; }.segmented button { padding:5px 8px; color:var(--muted); background:transparent; border:0; }.segmented button.active { color:#06101c; background:var(--blue); }.icon-button { padding:3px 7px; font-size:17px; line-height:1; }
  .statusline { display:flex; align-items:center; gap:9px; overflow:auto; padding:9px 0; color:var(--muted); border-block:1px solid var(--line); font-size:11px; white-space:nowrap; }.statusline span { color:#dce5ef; font-weight:600; }.statusline b { color:#93bfe9; font-weight:500; }.statusline i { width:3px; height:3px; flex:0 0 auto; border-radius:50%; background:#5f6874; }
  .grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 24px; }.panel { min-width:0; padding:19px 0; border-bottom:1px solid var(--line); }.metrics { margin:15px 0 0; }.metrics div { display:grid; grid-template-columns:minmax(90px,.85fr) minmax(0,1.15fr); gap:11px; padding:5px 0; border-bottom:1px solid rgba(255,255,255,.055); }.metrics div:last-child { border-bottom:0; }.metrics dt { color:var(--muted); font-size:11px; overflow-wrap:anywhere; }.metrics dd { min-width:0; margin:0; color:#e7edf4; font-size:12px; overflow-wrap:anywhere; text-align:right; }.empty { margin:15px 0 0; color:var(--muted); font-size:12px; line-height:1.55; }.inline-actions { justify-content:flex-start; margin-top:16px; }.feedback.success { color:#90ddae; }.feedback.error { color:#ffaeae; }.lifecycle { padding-top:17px; }.footer-actions { display:flex; gap:9px; flex-wrap:wrap; justify-content:flex-end; }.footer-feedback { text-align:right; }.toggle { display:inline-flex; align-items:center; gap:8px; color:#cbd4df; font-size:12px; cursor:pointer; }.toggle input { position:absolute; opacity:0; }.toggle span { position:relative; width:31px; height:17px; border-radius:20px; background:#626b76; transition:.15s; }.toggle span::after { content:""; position:absolute; top:3px; left:3px; width:11px; height:11px; border-radius:50%; background:white; transition:.15s; }.toggle input:checked + span { background:var(--blue); }.toggle input:checked + span::after { transform:translateX(14px); }.toggle input:focus-visible + span, button:focus-visible { outline:2px solid white; outline-offset:2px; }.compact .grid { grid-template-columns:1fr; }.compact .panel { padding-block:15px; }
  @media (max-width:620px) { main { padding-inline:16px; }.topbar,.overview,.lifecycle { align-items:flex-start; flex-direction:column; }.controls,.footer-actions { justify-content:flex-start; }.grid { grid-template-columns:1fr; }.status { text-align:left; }.footer-feedback { text-align:left; } }
  @media (prefers-reduced-motion:reduce) { *,*::before,*::after { transition:none !important; } }
</style>
