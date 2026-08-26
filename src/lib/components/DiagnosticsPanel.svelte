<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { ReminderReport } from '$lib/tasks';
  import { enterImmersiveMode, exitImmersiveMode, setMainWindowMaximized, setAlwaysOnTop } from '$lib/windowing';

  type WindowMode = 'normal' | 'maximized' | 'fullscreen';
  type ActionKey = 'snapshot' | 'metrics' | 'database' | 'notifications' | 'testNotification' | 'scheduleNotification' | 'cancelNotification' | 'tray' | 'release' | 'alwaysOnTop' | 'windowMode' | 'reminderResync';
  type JsonPrimitive = string | number | boolean | null;
  type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
  type JsonRecord = { [key: string]: JsonValue };
  type ActionState = { busy: boolean; error: string | null; success: string | null };
  type RunResult<T> = { ok: true; value: T } | { ok: false };
  interface Props {
    tauriAvailable: boolean;
    uiRunId: string;
    mode: WindowMode;
    initialAlwaysOnTop: boolean;
    activationId: number | null;
    activationError: string | null;
    initialSnapshot: JsonRecord | null;
    onModeChange: (mode: WindowMode) => void;
    latestReminderReport: ReminderReport | null;
    reminderSyncedAt: number | null;
    reminderWarningListenerToken: number | null;
    onReminderReconcile: () => Promise<ReminderReport>;
  }

  let { tauriAvailable, uiRunId, mode, initialAlwaysOnTop, activationId, activationError, initialSnapshot, latestReminderReport, reminderSyncedAt, reminderWarningListenerToken, onModeChange, onReminderReconcile }: Props = $props();
  const emptyAction = (): ActionState => ({ busy: false, error: null, success: null });
  let alwaysOnTop = $state(false);
  let runtimeSnapshot = $state<JsonRecord | null>(null);
  let metrics = $state<JsonRecord | null>(null);
  let database = $state<JsonRecord | null>(null);
  let notifications = $state<JsonRecord | null>(null);
  let actions = $state<Record<ActionKey, ActionState>>({ snapshot: emptyAction(), metrics: emptyAction(), database: emptyAction(), notifications: emptyAction(), testNotification: emptyAction(), scheduleNotification: emptyAction(), cancelNotification: emptyAction(), tray: emptyAction(), release: emptyAction(), alwaysOnTop: emptyAction(), windowMode: emptyAction(), reminderResync: emptyAction() });
  $effect(() => { runtimeSnapshot = initialSnapshot; alwaysOnTop = initialAlwaysOnTop; });
  const entries = (value: JsonRecord | null): Array<[string, JsonValue]> => value ? Object.entries(value) : [];
  const format = (value: JsonValue): string => value === null ? '—' : typeof value === 'object' ? JSON.stringify(value) : String(value);
  const message = (error: unknown): string => error instanceof Error ? error.message : String(error);
  const formatSyncTime = (value: number | null): string => value === null ? '—' : new Date(value).toLocaleString();
  function setAction(key: ActionKey, next: Partial<ActionState>): void { actions = { ...actions, [key]: { ...actions[key], ...next } }; }
  async function run<T>(key: ActionKey, command: string, args?: Record<string, unknown>): Promise<RunResult<T>> {
    if (!tauriAvailable) {
      setAction(key, { error: '当前在浏览器预览环境，Tauri 后端不可用。', success: null });
      return { ok: false };
    }
    setAction(key, { busy: true, error: null, success: null });
    try {
      const value = await invoke<T>(command, args);
      setAction(key, { busy: false, success: '已完成。' });
      return { ok: true, value };
    } catch (error) {
      setAction(key, { busy: false, error: message(error) });
      return { ok: false };
    }
  }
  async function refreshSnapshot(): Promise<void> { const result = await run<JsonRecord>('snapshot', 'get_runtime_snapshot'); if (result.ok) runtimeSnapshot = result.value; }
  async function sampleMetrics(): Promise<void> { const result = await run<JsonRecord>('metrics', 'sample_process_metrics'); if (result.ok) metrics = result.value; }
  async function probeDatabase(): Promise<void> { const result = await run<JsonRecord>('database', 'run_database_probe'); if (result.ok) database = result.value; }
  async function refreshNotifications(): Promise<void> { const result = await run<JsonRecord>('notifications', 'get_notification_diagnostics'); if (result.ok) notifications = result.value; }
  async function sendTestNotification(): Promise<void> { await run<void>('testNotification', 'send_test_notification'); }
  async function scheduleTestNotification(): Promise<void> { const result = await run<JsonRecord>('scheduleNotification', 'schedule_test_notification', { dueAtUtc: new Date(Date.now() + 30_000).toISOString() }); if (result.ok) await refreshNotifications(); }
  async function cancelTestNotification(): Promise<void> { const result = await run<JsonRecord>('cancelNotification', 'cancel_test_notification'); if (result.ok) await refreshNotifications(); }
  async function hideToTray(): Promise<void> { await run<void>('tray', 'hide_to_tray'); }
  async function releaseUi(): Promise<void> {
    if (reminderWarningListenerToken === null) {
      setAction('release', { error: '提醒告警消费者尚未登记。' });
      return;
    }
    await run<void>('release', 'release_ui', { listenerToken: reminderWarningListenerToken });
  }
  async function changeMode(next: WindowMode): Promise<void> {
    if (next === mode) return;
    setAction('windowMode', { busy: true, error: null, success: null });
    try {
      if (next === 'fullscreen') await enterImmersiveMode();
      else if (mode === 'fullscreen') await exitImmersiveMode();
      if (next === 'normal' || next === 'maximized') await setMainWindowMaximized(next === 'maximized');
      setAction('windowMode', { busy: false, success: '已完成。' });
      onModeChange(next);
    } catch (error) {
      setAction('windowMode', { busy: false, error: message(error) });
    }
  }
  async function resyncReminders(): Promise<void> {
    if (!tauriAvailable) {
      setAction('reminderResync', { error: '当前在浏览器预览环境，Tauri 后端不可用。', success: null });
      return;
    }
    setAction('reminderResync', { busy: true, error: null, success: null });
    try {
      const report = await onReminderReconcile();
      setAction('reminderResync', {
        busy: false,
        error: report.warning,
        success: report.warning ? '对账完成，但有警告。' : '提醒已同步。',
      });
    } catch (error) {
      setAction('reminderResync', { busy: false, error: message(error) });
    }
  }
  async function changeAlwaysOnTop(event: Event): Promise<void> { const input = event.currentTarget as HTMLInputElement; const enabled = input.checked; const result = await run<void>('alwaysOnTop', 'set_always_on_top', { alwaysOnTop: enabled }); if (result.ok) { await setAlwaysOnTop(enabled); alwaysOnTop = enabled; } else input.checked = alwaysOnTop; }
</script>

<details class:compact={false} class="diagnostics">
  <summary>诊断与桌面控制</summary>
  <div class="content">
    <div class="overview"><div><span class="label">UI RUN</span><code>{uiRunId}</code></div><div class="segmented" aria-label="窗口模式"><button class:active={mode === 'normal'} onclick={() => void changeMode('normal')} disabled={actions.windowMode.busy}>普通</button><button class:active={mode === 'maximized'} onclick={() => void changeMode('maximized')} disabled={actions.windowMode.busy}>最大化</button><button class:active={mode === 'fullscreen'} onclick={() => void changeMode('fullscreen')} disabled={actions.windowMode.busy}>全屏</button></div></div>
    <section class="panel"><div class="panel-heading"><h3>运行时快照</h3><button aria-label="刷新运行时快照" onclick={refreshSnapshot} disabled={actions.snapshot.busy}>{actions.snapshot.busy ? '读取中…' : '刷新'}</button></div>{#if entries(runtimeSnapshot).length}<dl>{#each entries(runtimeSnapshot) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>等待首次运行时快照。</p>{/if}{#if actions.snapshot.error}<p class="error" role="alert">{actions.snapshot.error}</p>{/if}</section>
    <div class="grid"><section class="panel"><div class="panel-heading"><h3>进程采样</h3><button onclick={sampleMetrics} disabled={actions.metrics.busy}>{actions.metrics.busy ? '采样中…' : '采样'}</button></div>{#if entries(metrics).length}<dl>{#each entries(metrics) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>读取当前进程指标。</p>{/if}{#if actions.metrics.error}<p class="error" role="alert">{actions.metrics.error}</p>{/if}</section>
      {#if true}<section class="panel"><div class="panel-heading"><h3>SQLite 探测</h3><button onclick={probeDatabase} disabled={actions.database.busy}>{actions.database.busy ? '探测中…' : '运行探测'}</button></div>{#if entries(database).length}<dl>{#each entries(database) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>验证数据库文件、连接与基础读写能力。</p>{/if}{#if actions.database.error}<p class="error" role="alert">{actions.database.error}</p>{/if}</section>
      <section class="panel"><div class="panel-heading"><h3>通知诊断</h3><button onclick={refreshNotifications} disabled={actions.notifications.busy}>{actions.notifications.busy ? '检查中…' : '检查'}</button></div>{#if entries(notifications).length}<dl>{#each entries(notifications) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>读取权限、平台支持与通知服务状态。</p>{/if}<div class="actions"><button onclick={sendTestNotification} disabled={actions.testNotification.busy}>发送即时通知</button><button onclick={scheduleTestNotification} disabled={actions.scheduleNotification.busy}>30 秒后提醒</button><button onclick={cancelTestNotification} disabled={actions.cancelNotification.busy}>取消测试提醒</button><button onclick={resyncReminders} disabled={actions.reminderResync.busy}>{actions.reminderResync.busy ? '同步中…' : '重新同步提醒'}</button></div>{#if latestReminderReport}<div class="reminder-summary" aria-live="polite"><p>最近同步：{formatSyncTime(reminderSyncedAt)}</p><p>已排程 {latestReminderReport.scheduled} · 已取消 {latestReminderReport.cancelled} · 已错过 {latestReminderReport.missedCount} · 能力 {latestReminderReport.capability}</p>{#if latestReminderReport.warning}<p class="error" role="alert">对账警告：{latestReminderReport.warning}</p>{/if}</div>{/if}{#each ['notifications', 'testNotification', 'scheduleNotification', 'cancelNotification', 'reminderResync'] as key}{#if actions[key as ActionKey].error}<p class="error" role="alert">{actions[key as ActionKey].error}</p>{/if}{/each}{#if activationId}<p class="success" role="status">通知点击：已定位任务 {activationId}</p>{/if}{#if activationError}<p class="error" role="alert">{activationError}</p>{/if}</section>{/if}
    </div>
    <footer><label><input type="checkbox" checked={alwaysOnTop} onchange={changeAlwaysOnTop} disabled={actions.alwaysOnTop.busy} /> 始终置顶</label><div class="actions"><button onclick={hideToTray} disabled={actions.tray.busy}>{actions.tray.busy ? '处理中…' : '隐藏至托盘'}</button><button class="danger" onclick={releaseUi} disabled={actions.release.busy || reminderWarningListenerToken === null}>{actions.release.busy ? '释放中…' : '释放 UI'}</button></div></footer>
    {#if actions.alwaysOnTop.error || actions.tray.error || actions.release.error}<p class="error" role="alert">{actions.alwaysOnTop.error ?? actions.tray.error ?? actions.release.error}</p>{/if}
  </div>
</details>

<style>
  .diagnostics { margin-top:8px; padding:14px 0 0; border-top:1px solid var(--line); color:var(--muted); font-size:12px; opacity:.82; }
  .diagnostics:not([open]) { opacity:.68; }
  .diagnostics summary { cursor:pointer; color:var(--muted); font-size:11px; font-weight:600; letter-spacing:.04em; }
  .diagnostics[open] summary { color:var(--text-soft); }
  .content { padding-top:12px; }
  .overview,.panel-heading,footer,.actions { display:flex; align-items:center; justify-content:space-between; gap:10px; }
  .label { display:block; margin-bottom:4px; font-size:10px; letter-spacing:.1em; }
  code { color:var(--text-soft); font-size:11px; }
  .segmented { display:flex; border:1px solid var(--line); border-radius:var(--radius-sm); }
  .segmented button { border:0; border-radius:0; }
  .segmented .active { color:#07111f; background:var(--info); }
  .grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 20px; }
  .panel { padding:14px 0; border-bottom:1px solid var(--line); }
  .panel h3 { margin:0; color:var(--text-soft); font-size:12px; }
  .panel p { line-height:1.5; }
  .panel dl { margin:10px 0 0; }
  .panel dl div { display:grid; grid-template-columns:1fr 1fr; gap:8px; padding:4px 0; border-bottom:1px solid rgba(255,255,255,.045); }
  .panel dt { overflow-wrap:anywhere; }
  .panel dd { margin:0; color:var(--text-soft); text-align:right; overflow-wrap:anywhere; }
  .actions { justify-content:flex-start; flex-wrap:wrap; margin-top:10px; }
  .reminder-summary { margin-top:10px; }
  .reminder-summary p { margin:4px 0; }
  footer { padding-top:14px; }
  button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:6px 8px; color:var(--muted); background:transparent; font-size:11px; }
  button:hover:not(:disabled) { border-color:var(--line-strong); color:var(--text-soft); background:rgba(255,255,255,.04); }
  .danger,.error { color:var(--danger); }
  .success { color:var(--success); }
  @media (max-width:620px) { .grid { grid-template-columns:1fr; } .overview,footer { align-items:flex-start; flex-direction:column; } }
</style>
