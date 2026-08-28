<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { ReminderReport } from '$lib/tasks';
  import type { AutoImmersivePreference } from '$lib/ui-preferences';
  import { setAlwaysOnTop } from '$lib/windowing';

  type ActionKey = 'snapshot' | 'metrics' | 'database' | 'notifications' | 'testNotification' | 'scheduleNotification' | 'cancelNotification' | 'tray' | 'release' | 'alwaysOnTop' | 'reminderResync';
  type JsonPrimitive = string | number | boolean | null;
  type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
  type JsonRecord = { [key: string]: JsonValue };
  type ActionState = { busy: boolean; error: string | null; success: string | null };
  type RunResult<T> = { ok: true; value: T } | { ok: false };
  interface Props {
    tauriAvailable: boolean;
    uiRunId: string;
    initialAlwaysOnTop: boolean;
    activationId: number | null;
    activationError: string | null;
    initialSnapshot: JsonRecord | null;
    latestReminderReport: ReminderReport | null;
    reminderSyncedAt: number | null;
    reminderWarningListenerToken: number | null;
    autoImmersivePreference: AutoImmersivePreference;
    onAutoImmersivePreferenceChange: (value: AutoImmersivePreference) => void;
    onReminderReconcile: () => Promise<ReminderReport>;
  }

  let { tauriAvailable, uiRunId, initialAlwaysOnTop, activationId, activationError, initialSnapshot, latestReminderReport, reminderSyncedAt, reminderWarningListenerToken, autoImmersivePreference, onAutoImmersivePreferenceChange, onReminderReconcile }: Props = $props();
  const emptyAction = (): ActionState => ({ busy: false, error: null, success: null });
  let alwaysOnTop = $state(false);
  let runtimeSnapshot = $state<JsonRecord | null>(null);
  let metrics = $state<JsonRecord | null>(null);
  let database = $state<JsonRecord | null>(null);
  let notifications = $state<JsonRecord | null>(null);
  let actions = $state<Record<ActionKey, ActionState>>({ snapshot: emptyAction(), metrics: emptyAction(), database: emptyAction(), notifications: emptyAction(), testNotification: emptyAction(), scheduleNotification: emptyAction(), cancelNotification: emptyAction(), tray: emptyAction(), release: emptyAction(), alwaysOnTop: emptyAction(), reminderResync: emptyAction() });
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
  async function changeAlwaysOnTop(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const enabled = input.checked;
    if (!tauriAvailable) {
      setAction('alwaysOnTop', { error: '当前在浏览器预览环境，Tauri 后端不可用。', success: null });
      input.checked = alwaysOnTop;
      return;
    }
    setAction('alwaysOnTop', { busy: true, error: null, success: null });
    try {
      await setAlwaysOnTop(enabled);
      setAction('alwaysOnTop', { busy: false, success: '已完成。' });
      alwaysOnTop = enabled;
    } catch (error) {
      setAction('alwaysOnTop', { busy: false, error: message(error) });
      input.checked = alwaysOnTop;
    }
  }
</script>

<div class="diagnostics">
  <div class="content">
    <div class="overview"><div><span class="label">UI RUN</span><code>{uiRunId}</code></div></div>
    <section class="panel preference-panel">
      <div>
        <h3>自动沉浸</h3>
        <p>决定开始新的专注阶段时是否自动进入沉浸模式。</p>
      </div>
      <label>
        <span class="sr-only">自动沉浸偏好</span>
        <select
          value={autoImmersivePreference}
          onchange={(event) => onAutoImmersivePreferenceChange((event.currentTarget as HTMLSelectElement).value as AutoImmersivePreference)}
        >
          <option value="unset">下次询问</option>
          <option value="enabled">自动进入</option>
          <option value="disabled">保持窗口模式</option>
        </select>
      </label>
    </section>
    <section class="panel"><div class="panel-heading"><h3>运行时快照</h3><button aria-label="刷新运行时快照" onclick={refreshSnapshot} disabled={actions.snapshot.busy}>{actions.snapshot.busy ? '读取中…' : '刷新'}</button></div>{#if entries(runtimeSnapshot).length}<dl>{#each entries(runtimeSnapshot) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>等待首次运行时快照。</p>{/if}{#if actions.snapshot.error}<p class="error" role="alert">{actions.snapshot.error}</p>{/if}</section>
    <div class="grid"><section class="panel"><div class="panel-heading"><h3>进程采样</h3><button onclick={sampleMetrics} disabled={actions.metrics.busy}>{actions.metrics.busy ? '采样中…' : '采样'}</button></div>{#if entries(metrics).length}<dl>{#each entries(metrics) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>读取当前进程指标。</p>{/if}{#if actions.metrics.error}<p class="error" role="alert">{actions.metrics.error}</p>{/if}</section>
      {#if true}<section class="panel"><div class="panel-heading"><h3>SQLite 探测</h3><button onclick={probeDatabase} disabled={actions.database.busy}>{actions.database.busy ? '探测中…' : '运行探测'}</button></div>{#if entries(database).length}<dl>{#each entries(database) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>验证数据库文件、连接与基础读写能力。</p>{/if}{#if actions.database.error}<p class="error" role="alert">{actions.database.error}</p>{/if}</section>
      <section class="panel"><div class="panel-heading"><h3>通知诊断</h3><button onclick={refreshNotifications} disabled={actions.notifications.busy}>{actions.notifications.busy ? '检查中…' : '检查'}</button></div>{#if entries(notifications).length}<dl>{#each entries(notifications) as [key, value]}<div><dt>{key}</dt><dd>{format(value)}</dd></div>{/each}</dl>{:else}<p>读取权限、平台支持与通知服务状态。</p>{/if}<div class="actions"><button onclick={sendTestNotification} disabled={actions.testNotification.busy}>发送即时通知</button><button onclick={scheduleTestNotification} disabled={actions.scheduleNotification.busy}>30 秒后提醒</button><button onclick={cancelTestNotification} disabled={actions.cancelNotification.busy}>取消测试提醒</button><button onclick={resyncReminders} disabled={actions.reminderResync.busy}>{actions.reminderResync.busy ? '同步中…' : '重新同步提醒'}</button></div>{#if latestReminderReport}<div class="reminder-summary" aria-live="polite"><p>最近同步：{formatSyncTime(reminderSyncedAt)}</p><p>已排程 {latestReminderReport.scheduled} · 已取消 {latestReminderReport.cancelled} · 已错过 {latestReminderReport.missedCount} · 能力 {latestReminderReport.capability}</p>{#if latestReminderReport.warning}<p class="error" role="alert">对账警告：{latestReminderReport.warning}</p>{/if}</div>{/if}{#each ['notifications', 'testNotification', 'scheduleNotification', 'cancelNotification', 'reminderResync'] as key}{#if actions[key as ActionKey].error}<p class="error" role="alert">{actions[key as ActionKey].error}</p>{/if}{/each}{#if activationId}<p class="success" role="status">通知点击：已定位任务 {activationId}</p>{/if}{#if activationError}<p class="error" role="alert">{activationError}</p>{/if}</section>{/if}
    </div>
    <footer><label><input type="checkbox" checked={alwaysOnTop} onchange={changeAlwaysOnTop} disabled={actions.alwaysOnTop.busy} /> 始终置顶</label><div class="actions"><button onclick={hideToTray} disabled={actions.tray.busy}>{actions.tray.busy ? '处理中…' : '隐藏至托盘'}</button><button class="danger" onclick={releaseUi} disabled={actions.release.busy || reminderWarningListenerToken === null}>{actions.release.busy ? '释放中…' : '释放 UI'}</button></div></footer>
    {#if actions.alwaysOnTop.error || actions.tray.error || actions.release.error}<p class="error" role="alert">{actions.alwaysOnTop.error ?? actions.tray.error ?? actions.release.error}</p>{/if}
  </div>
</div>

<style>
  .diagnostics { color:var(--muted); font-size:12px; }
  .content { padding-top:12px; }
  .overview,.panel-heading,footer,.actions { display:flex; align-items:center; justify-content:space-between; gap:10px; }
  .label { display:block; margin-bottom:4px; font-size:10px; letter-spacing:.1em; }
  code { color:var(--text-soft); font-size:11px; }
  .grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:0 20px; }
  .panel { padding:14px 0; border-bottom:1px solid var(--line); }
  .panel h3 { margin:0; color:var(--text-soft); font-size:12px; }
  .preference-panel { display:flex; align-items:center; justify-content:space-between; gap:16px; }
  .preference-panel p { margin:5px 0 0; }
  .preference-panel select { border:1px solid var(--line); border-radius:var(--radius-sm); padding:7px 9px; color:var(--text); background:var(--surface); }
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
