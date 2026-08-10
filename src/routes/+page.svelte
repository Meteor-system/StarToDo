<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import TaskWorkspace from '$lib/components/TaskWorkspace.svelte';
  import DiagnosticsPanel from '$lib/components/DiagnosticsPanel.svelte';
  import {
    acknowledgePendingActivations,
    acknowledgeReminderWarnings,
    claimPendingActivations,
    claimPendingReminderWarnings,
    reconcileReminders,
    type PendingActivation,
    type ReminderReport,
    type ReminderWarningContext,
    type ReminderWarningEvent
  } from '$lib/tasks';

  type WindowMode = 'compact' | 'full';
  type JsonPrimitive = string | number | boolean | null;
  type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
  type JsonRecord = { [key: string]: JsonValue };
  interface WindowPreferences { mode: WindowMode; width: number; height: number; alwaysOnTop: boolean; }
  interface DisplayReminderWarning extends ReminderWarningContext { id: string; }
  interface UiReadyRegistration {
    runtimeSnapshot: JsonRecord;
    reminderWarningListenerToken: number;
  }

  const reminderWarningGenerationState = globalThis as typeof globalThis & {
    __starToDoReminderWarningGeneration?: number;
  };
  const uiRunId = crypto.randomUUID();
  let tauriAvailable = $state(false);
  let reminderWarningListenerToken = $state<number | null>(null);
  let initialized = $state(false);
  let runtimeSnapshot = $state<JsonRecord | null>(null);
  let windowMode = $state<WindowMode>('full');
  let alwaysOnTop = $state(false);
  let notificationActivationId = $state<number | null>(null);
  let notificationActivationNonce = $state(0);
  let notificationActivationError = $state<string | null>(null);
  let latestReminderReport = $state<ReminderReport | null>(null);
  let reminderSyncedAt = $state<number | null>(null);
  let reconcileWarning = $state<string | null>(null);
  let mutationWarnings = $state<DisplayReminderWarning[]>([]);
  let nextReminderWarningId = 0;
  let missedTaskIds = $state<number[]>([]);
  let missedCount = $state(0);
  let reminderResyncBusy = $state(false);
  let reminderReconcileSequence = 0;
  let inFlightReminderReconcile: Promise<ReminderReport> | null = null;

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function handleActivationId(id: number): number | null {
    if (!Number.isSafeInteger(id) || id <= 0) return null;
    notificationActivationId = id;
    notificationActivationNonce += 1;
    notificationActivationError = null;
    return notificationActivationNonce;
  }

  function warningOperationLabel(operation: ReminderWarningContext['operation']): string {
    if (operation === 'complete') return '完成';
    if (operation === 'restore') return '恢复';
    if (operation === 'snooze') return '稍后提醒';
    if (operation === 'defer') return '延期';
    if (operation === 'delete') return '删除';
    if (operation === 'permanent-delete') return '永久删除';
    if (operation === 'reminder-fired') return '处理提醒';
    return '保存';
  }

  function handleMutationWarning(context: ReminderWarningContext): void {
    if (context.message === null) {
      mutationWarnings = mutationWarnings.filter((warning) => !(
        warning.source === 'mutation' &&
        warning.taskId === context.taskId &&
        warning.sequence <= context.sequence
      ));
      return;
    }
    mutationWarnings = [
      ...mutationWarnings,
      { ...context, id: `mutation:${++nextReminderWarningId}` }
    ];
  }

  const displayedReminderWarningIds = new Set<number>();
  const pendingActivationIds = new Set<number>();
  const pendingActivationByNonce = new Map<number, PendingActivation>();
  const activationAckInFlight = new Set<number>();
  let activationDrain: Promise<void> = Promise.resolve();
  let activationDrainRequest: (() => void) | null = null;
  let activationDrainTimer: ReturnType<typeof setTimeout> | undefined;
  let activationAckRetryTimer: ReturnType<typeof setTimeout> | undefined;
  let activationPageDisposed = false;
  let reminderWarningDrain: Promise<void> = Promise.resolve();

  function handleReminderWarningEvent(warning: ReminderWarningEvent): void {
    if (displayedReminderWarningIds.has(warning.id)) return;
    displayedReminderWarningIds.add(warning.id);
    mutationWarnings = [
      ...mutationWarnings,
      {
        id: `event:${warning.id}`,
        source: 'event',
        operation: 'reminder-fired',
        taskId: warning.taskId,
        message: warning.message,
        sequence: 0
      }
    ];
  }

  function scheduleActivationDrain(delayMs: number): void {
    if (activationDrainTimer !== undefined) window.clearTimeout(activationDrainTimer);
    activationDrainTimer = window.setTimeout(() => {
      activationDrainTimer = undefined;
      if (!activationPageDisposed) activationDrainRequest?.();
    }, delayMs);
  }

  function scheduleActivationAckRetry(nonce: number, delayMs: number): void {
    if (activationAckRetryTimer !== undefined) window.clearTimeout(activationAckRetryTimer);
    activationAckRetryTimer = window.setTimeout(() => {
      activationAckRetryTimer = undefined;
      if (!activationPageDisposed) acknowledgeResolvedActivation(nonce);
    }, delayMs);
  }

  function drainPendingActivations(isDisposed: () => boolean): void {
    activationDrain = activationDrain
      .then(async () => {
        if (isDisposed() || pendingActivationByNonce.size > 0) return;
        const activations = await claimPendingActivations(uiRunId);
        if (isDisposed() || activations.length === 0 || pendingActivationByNonce.size > 0) return;

        const activation = activations[0];
        if (!activation || !Number.isSafeInteger(activation.id) || activation.id <= 0) {
          notificationActivationError = '收到无效的通知激活记录，无法确认其来源。';
          scheduleActivationDrain(5_000);
          return;
        }
        if (!Number.isSafeInteger(activation.taskId) || activation.taskId <= 0) {
          notificationActivationError = '收到无效的通知激活任务 ID，已尝试终结该记录。';
          try {
            const result = await acknowledgePendingActivations(uiRunId, [activation.id]);
            if (result.acknowledgedIds.includes(activation.id)) activationDrainRequest?.();
            else scheduleActivationDrain(5_000);
          } catch (error) {
            notificationActivationError = `无效通知激活终结失败：${errorMessage(error)}`;
            scheduleActivationDrain(5_000);
          }
          return;
        }
        const nonce = handleActivationId(activation.taskId);
        if (nonce === null) return;
        pendingActivationIds.add(activation.id);
        pendingActivationByNonce.set(nonce, activation);
        await tick();
      })
      .catch((error) => {
        if (!isDisposed()) {
          notificationActivationError = `待处理通知激活消费失败：${errorMessage(error)}`;
        }
      });
  }

  function acknowledgeResolvedActivation(nonce: number): void {
    if (activationPageDisposed || activationAckInFlight.has(nonce)) return;
    const activation = pendingActivationByNonce.get(nonce);
    if (!activation) return;

    activationAckInFlight.add(nonce);
    void acknowledgePendingActivations(uiRunId, [activation.id])
      .then((result) => {
        activationAckInFlight.delete(nonce);
        if (activationPageDisposed) return;
        if (!result.acknowledgedIds.includes(activation.id)) {
          pendingActivationByNonce.delete(nonce);
          pendingActivationIds.delete(activation.id);
          notificationActivationError = '通知激活定位完成，但确认租约已失效；将自动重试。';
          scheduleActivationDrain(5_000);
          return;
        }

        pendingActivationByNonce.delete(nonce);
        pendingActivationIds.delete(activation.id);
        notificationActivationError = null;
        activationDrainRequest?.();
      })
      .catch((error) => {
        activationAckInFlight.delete(nonce);
        if (!activationPageDisposed) {
          notificationActivationError = `通知激活已定位，但确认状态保存失败：${errorMessage(error)}`;
          scheduleActivationAckRetry(nonce, 5_000);
        }
      });
  }

  function drainPendingReminderWarnings(listenerToken: number | null, isDisposed: () => boolean): void {
    if (listenerToken === null) return;
    reminderWarningDrain = reminderWarningDrain
      .then(async () => {
        if (isDisposed()) return;
        const warnings = await claimPendingReminderWarnings(listenerToken);
        if (isDisposed() || warnings.length === 0) return;
        warnings.forEach(handleReminderWarningEvent);
        await tick();
        if (!isDisposed()) {
          await acknowledgeReminderWarnings(listenerToken, warnings.map((warning) => warning.id));
        }
      })
      .catch((error) => {
        if (!isDisposed() && errorMessage(error) !== 'reminder warning listener is not current') {
          notificationActivationError = `提醒告警队列消费失败：${errorMessage(error)}`;
        }
      });
  }

  function requestReminderReconcile(): Promise<ReminderReport> {
    if (inFlightReminderReconcile) return inFlightReminderReconcile;
    const sequence = ++reminderReconcileSequence;
    const flight = reconcileReminders()
      .then((report) => {
        if (inFlightReminderReconcile === flight && sequence === reminderReconcileSequence) {
          latestReminderReport = report;
          reminderSyncedAt = Date.now();
          missedTaskIds = report.missedTaskIds;
          missedCount = report.missedCount;
          reconcileWarning = report.warning;
        }
        return report;
      })
      .catch((error) => {
        if (inFlightReminderReconcile === flight && sequence === reminderReconcileSequence) {
          reconcileWarning = errorMessage(error);
        }
        throw error;
      })
      .finally(() => {
        if (inFlightReminderReconcile === flight) inFlightReminderReconcile = null;
      });
    inFlightReminderReconcile = flight;
    return flight;
  }

  function handleModeChange(mode: WindowMode): void {
    windowMode = mode;
  }

  async function resyncReminders(): Promise<void> {
    if (!tauriAvailable) return;
    reminderResyncBusy = true;
    try {
      await requestReminderReconcile();
    } finally {
      reminderResyncBusy = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlistenActivation: (() => void) | undefined;
    let unlistenReminderWarning: (() => void) | undefined;
    let reminderWarningPollingTimer: ReturnType<typeof window.setInterval> | undefined;
    let reminderWarningRegistrationInFlight: Promise<void> | undefined;
    let reminderWarningRegistrationRetryDisabled = false;
    let reminderWarningEventListenerReady = false;
    const reminderWarningGenerationKey = 'star-todo.reminder-warning-generation';
    let storedReminderWarningGeneration = 0;
    try {
      const storedValue = Number(window.localStorage.getItem(reminderWarningGenerationKey));
      if (Number.isSafeInteger(storedValue) && storedValue > 0) {
        storedReminderWarningGeneration = storedValue;
      }
    } catch {}
    const reminderWarningListenerGeneration = Math.max(
      Date.now(),
      storedReminderWarningGeneration + 1,
      (reminderWarningGenerationState.__starToDoReminderWarningGeneration ?? 0) + 1
    );
    reminderWarningGenerationState.__starToDoReminderWarningGeneration = reminderWarningListenerGeneration;
    try {
      window.localStorage.setItem(reminderWarningGenerationKey, String(reminderWarningListenerGeneration));
    } catch {}
    const isDisposed = () => disposed;
    activationPageDisposed = false;
    pendingActivationIds.clear();
    pendingActivationByNonce.clear();
    activationAckInFlight.clear();
    activationDrainRequest = () => drainPendingActivations(isDisposed);

    async function registerReminderWarningListener(): Promise<void> {
      if (disposed || reminderWarningListenerToken !== null || reminderWarningRegistrationRetryDisabled) return;
      if (reminderWarningRegistrationInFlight) return reminderWarningRegistrationInFlight;
      const registration = invoke<UiReadyRegistration>('record_ui_ready', {
        uiRunId,
        uiGeneration: reminderWarningListenerGeneration,
        eventListenerReady: reminderWarningEventListenerReady
      })
        .then(async (ready) => {
          if (disposed) {
            try {
              await invoke<void>('record_ui_not_ready', {
                listenerToken: ready.reminderWarningListenerToken
              });
            } catch {}
            return;
          }
          runtimeSnapshot = ready.runtimeSnapshot;
          reminderWarningListenerToken = ready.reminderWarningListenerToken;
          drainPendingReminderWarnings(reminderWarningListenerToken, isDisposed);
        })
        .catch((error) => {
          const message = errorMessage(error);
          if (!disposed) notificationActivationError = `提醒告警消费者登记失败：${message}`;
          if (message.includes('stale reminder warning listener registration')) {
            reminderWarningRegistrationRetryDisabled = true;
          }
        });
      reminderWarningRegistrationInFlight = registration;
      try {
        await registration;
      } finally {
        if (reminderWarningRegistrationInFlight === registration) {
          reminderWarningRegistrationInFlight = undefined;
        }
      }
    }

    tauriAvailable = isTauriRuntime();
    initialized = true;
    if (!tauriAvailable) return;

    reminderWarningPollingTimer = window.setInterval(() => {
      drainPendingActivations(isDisposed);
      if (reminderWarningListenerToken === null) {
        void registerReminderWarningListener();
      } else {
        drainPendingReminderWarnings(reminderWarningListenerToken, isDisposed);
      }
    }, 5_000);

    void (async () => {
      try {
        const unlisten = await listen('notification-activation', () => {
          if (!disposed) drainPendingActivations(isDisposed);
        });
        if (disposed) { unlisten(); return; }
        unlistenActivation = unlisten;
      } catch (error) {
        if (!disposed) notificationActivationError = `通知激活监听失败：${errorMessage(error)}`;
      }

      try {
        const unlisten = await listen<ReminderWarningEvent>('reminder-warning', () => {
          if (!disposed) {
            if (reminderWarningListenerToken === null) void registerReminderWarningListener();
            else drainPendingReminderWarnings(reminderWarningListenerToken, isDisposed);
          }
        });
        if (disposed) { unlisten(); return; }
        unlistenReminderWarning = unlisten;
        reminderWarningEventListenerReady = true;
      } catch (error) {
        if (!disposed) notificationActivationError = `提醒警告监听失败：${errorMessage(error)}`;
      }

      if (disposed) return;
      drainPendingActivations(isDisposed);

      if (!disposed) await registerReminderWarningListener();
    })();

    void (async () => {
      try {
        const preferences = await invoke<WindowPreferences>('get_window_preferences');
        if (!disposed) {
          windowMode = preferences.mode;
          alwaysOnTop = preferences.alwaysOnTop;
        }
      } catch (error) {
        if (!disposed) notificationActivationError = `窗口偏好读取失败：${errorMessage(error)}`;
      }
    })();

    return () => {
      disposed = true;
      activationPageDisposed = true;
      activationDrainRequest = null;
      if (activationDrainTimer !== undefined) {
        window.clearTimeout(activationDrainTimer);
        activationDrainTimer = undefined;
      }
      if (activationAckRetryTimer !== undefined) {
        window.clearTimeout(activationAckRetryTimer);
        activationAckRetryTimer = undefined;
      }
      if (reminderWarningPollingTimer !== undefined) window.clearInterval(reminderWarningPollingTimer);
      unlistenActivation?.();
      unlistenReminderWarning?.();
      void (async () => {
        await reminderWarningRegistrationInFlight;
        const listenerToken = reminderWarningListenerToken;
        if (listenerToken === null) return;
        try {
          await invoke<void>('record_ui_not_ready', { listenerToken });
        } catch {}
      })();
    };
  });
</script>

<svelte:head><meta name="theme-color" content="#1d2025" /></svelte:head>

<main aria-labelledby="page-title">
  <header class="topbar">
    <div class="identity"><span class="mark" aria-hidden="true"></span><div><p class="eyebrow">STAR TODO</p><h1 id="page-title">任务</h1></div></div>
    <p class="status" aria-live="polite"><span class:offline={!tauriAvailable} class="status-dot"></span>{#if !initialized}正在初始化{:else if tauriAvailable}已连接{:else}浏览器预览{/if}</p>
  </header>
  {#if reconcileWarning || mutationWarnings.length || missedCount}
    <section class="page-alert" aria-live="polite">
      {#if reconcileWarning}<p>提醒同步警告：{reconcileWarning}</p>{/if}
      {#each mutationWarnings as warning (warning.id)}
        <p>{warning.taskId === null ? '任务' : `任务 #${warning.taskId}`}{warningOperationLabel(warning.operation)}后提醒同步失败：{warning.message}</p>
      {/each}
      {#if missedCount}<p>有 {missedCount} 个提醒已错过，涉及任务：{missedTaskIds.join('、')}。</p>{/if}
      <button onclick={resyncReminders} disabled={reminderResyncBusy}>{reminderResyncBusy ? '同步中…' : '重新同步提醒'}</button>
    </section>
  {/if}
  <TaskWorkspace
    {tauriAvailable}
    {initialized}
    compact={windowMode === 'compact'}
    activationId={notificationActivationId}
    activationNonce={notificationActivationNonce}
    onReminderReconcile={requestReminderReconcile}
    onMutationWarning={handleMutationWarning}
    onActivationResolved={acknowledgeResolvedActivation}
  />
  <DiagnosticsPanel
    {tauriAvailable}
    {uiRunId}
    mode={windowMode}
    initialAlwaysOnTop={alwaysOnTop}
    activationId={notificationActivationId}
    activationError={notificationActivationError}
    initialSnapshot={runtimeSnapshot}
    {latestReminderReport}
    {reminderSyncedAt}
    {reminderWarningListenerToken}
    onModeChange={handleModeChange}
    onReminderReconcile={requestReminderReconcile}
  />
</main>

<style>
  :global(*) { box-sizing:border-box; } :global(body) { margin:0; min-width:320px; background:#1d2025; color:#edf1f6; font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; } :global(button),:global(input),:global(textarea),:global(select) { font:inherit; }
  main { --blue:#5ba9ff; --line:rgba(233,240,249,.15); --muted:#a9b2bd; width:min(100%,860px); min-height:100vh; margin:0 auto; padding:22px clamp(16px,4vw,36px) 18px; background:linear-gradient(135deg,rgba(71,76,85,.74),rgba(31,34,40,.88)); border-inline:1px solid rgba(255,255,255,.07); }.topbar { display:flex; align-items:center; justify-content:space-between; gap:14px; padding-bottom:18px; border-bottom:1px solid var(--line); }.identity { display:flex; align-items:center; gap:11px; }.mark { width:9px; height:9px; border-radius:50%; background:var(--blue); box-shadow:0 0 17px rgba(91,169,255,.75); }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h1 { margin:3px 0 0; font-size:20px; font-weight:600; }.status { display:flex; align-items:center; gap:7px; margin:0; color:var(--muted); font-size:12px; }.status-dot { width:7px; height:7px; border-radius:50%; background:#76dba2; }.status-dot.offline { background:#e0ad65; }.page-alert { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-top:14px; padding:10px 11px; color:#f2d5a1; background:rgba(224,173,101,.09); border-left:2px solid #e0ad65; font-size:12px; }.page-alert p { margin:0; line-height:1.5; }.page-alert button { flex:none; border:1px solid rgba(224,173,101,.45); border-radius:4px; padding:5px 8px; color:#f2d5a1; background:transparent; font-size:11px; } @media (max-width:620px) { main { padding-inline:16px; }.page-alert { align-items:flex-start; flex-direction:column; } } @media (prefers-reduced-motion:reduce) { :global(*),:global(*::before),:global(*::after) { transition:none !important; animation:none !important; } }
</style>
