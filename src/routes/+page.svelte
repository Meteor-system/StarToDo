<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import TaskWorkspace from '$lib/components/TaskWorkspace.svelte';
  import FocusWorkspace from '$lib/components/FocusWorkspace.svelte';
  import FocusMiniBar from '$lib/components/FocusMiniBar.svelte';
  import FloatingWindow from '$lib/components/FloatingWindow.svelte';
  import DiagnosticsPanel from '$lib/components/DiagnosticsPanel.svelte';
  import {
    acknowledgePendingPomodoroActivations,
    claimPendingPomodoroActivations,
    getPomodoroView,
    pausePomodoro,
    POMODORO_TASK_UNAVAILABLE_ERROR,
    reconcilePomodoroTaskSelection,
    resetPomodoro,
    resumePomodoro,
    skipPomodoro,
    startPomodoro,
    updatePomodoroSettings,
    type PomodoroMutationResult,
    type PomodoroSnapshot,
    type PomodoroTaskSummary,
    type PomodoroView,
    type PendingPomodoroActivation,
    type StartPomodoroInput,
    type UpdatePomodoroSettingsInput
  } from '$lib/pomodoro';
  import {
    acknowledgePendingActivations,
    acknowledgeReminderWarnings,
    claimPendingActivations,
    claimPendingReminderWarnings,
    reconcileReminders,
    takePendingFloatingIntent,
    toggleFloatingWindow,
    type PendingActivation,
    type ReminderReport,
    type ReminderWarningContext,
    type ReminderWarningEvent
  } from '$lib/tasks';

  type WindowMode = 'normal' | 'maximized' | 'fullscreen';
  type AppView = 'tasks' | 'focus';
  type JsonPrimitive = string | number | boolean | null;
  type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
  type JsonRecord = { [key: string]: JsonValue };
  interface WindowPreferences {
    layout: 'adaptive';
    maximized: boolean;
    normalBounds: { x: number | null; y: number | null; width: number; height: number };
    alwaysOnTop: boolean;
    lastImmersive: boolean;
  }
  interface DisplayReminderWarning extends ReminderWarningContext { id: string; }
  interface UiReadyRegistration {
    runtimeSnapshot: JsonRecord;
    reminderWarningListenerToken: number;
  }

  const reminderWarningGenerationState = globalThis as typeof globalThis & {
    __starToDoReminderWarningGeneration?: number;
  };
  const uiRunId = crypto.randomUUID();
  const windowKind: 'main' | 'floating' =
    typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('window') === 'floating'
      ? 'floating'
      : 'main';
  let tauriAvailable = $state(false);
  let reminderWarningListenerToken = $state<number | null>(null);
  let initialized = $state(false);
  let runtimeSnapshot = $state<JsonRecord | null>(null);
  let windowMode = $state<WindowMode>('maximized');
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
  let floatingWindowBusy = $state(false);
  let floatingWindowError = $state<string | null>(null);
  let reminderReconcileSequence = 0;
  let inFlightReminderReconcile: Promise<ReminderReport> | null = null;
  let activeView = $state<AppView>('tasks');
  let pomodoroSnapshot = $state<PomodoroSnapshot | null>(null);
  let pomodoroTaskSummaries = $state<PomodoroTaskSummary[]>([]);
  let pomodoroBusy = $state(false);
  let pomodoroRefreshSequence = 0;
  let pomodoroCommandWarning = $state<string | null>(null);
  let pomodoroRefreshWarning = $state<string | null>(null);
  let pomodoroActivationWarning = $state<string | null>(null);
  let pomodoroListenerWarning = $state<string | null>(null);
  let pomodoroWarning = $derived(
    pomodoroActivationWarning ?? pomodoroListenerWarning ?? pomodoroCommandWarning ?? pomodoroRefreshWarning
  );
  let selectedFocusTaskId = $state<number | null>(null);
  let pomodoroCounts = $derived(new Map(pomodoroTaskSummaries.map((summary) => [summary.taskId, summary.completedFocusCount])));

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function applyPomodoroResult(result: PomodoroMutationResult): void {
    pomodoroSnapshot = result.snapshot;
    pomodoroCommandWarning = result.notificationWarning ?? null;
  }

  function applyPomodoroView(view: PomodoroView): void {
    pomodoroSnapshot = view.snapshot;
    pomodoroTaskSummaries = view.taskSummaries;
    selectedFocusTaskId = reconcilePomodoroTaskSelection(selectedFocusTaskId, view.taskSummaries);
  }

  async function refreshPomodoro(isDisposed: () => boolean = () => false): Promise<boolean> {
    if (!tauriAvailable) return false;
    const sequence = ++pomodoroRefreshSequence;
    try {
      const view = await getPomodoroView();
      if (isDisposed() || sequence !== pomodoroRefreshSequence) return false;
      applyPomodoroView(view);
      pomodoroRefreshWarning = null;
      return true;
    } catch (error) {
      if (!isDisposed() && sequence === pomodoroRefreshSequence) {
        pomodoroRefreshWarning = `专注状态读取失败：${errorMessage(error)}`;
      }
      return false;
    }
  }

  async function runPomodoroCommand(
    command: () => Promise<PomodoroMutationResult>,
    failedTaskId: number | null = null
  ): Promise<void> {
    if (!tauriAvailable || pomodoroBusy) return;
    pomodoroRefreshSequence += 1;
    pomodoroBusy = true;
    try {
      const result = await command();
      pomodoroRefreshSequence += 1;
      applyPomodoroResult(result);
      await refreshPomodoro();
    } catch (error) {
      const message = errorMessage(error);
      if (message === POMODORO_TASK_UNAVAILABLE_ERROR) {
        if (failedTaskId !== null && selectedFocusTaskId === failedTaskId) selectedFocusTaskId = null;
        pomodoroCommandWarning = '所选任务已完成、移入回收站或被删除，请重新选择。';
        await refreshPomodoro();
      } else {
        pomodoroCommandWarning = `专注操作失败：${message}`;
      }
    } finally {
      pomodoroBusy = false;
    }
  }

  function selectFocusTask(taskId: number): void {
    selectedFocusTaskId = taskId;
    activeView = 'focus';
  }

  function handlePomodoroTasksChanged(unavailableTaskId?: number): void {
    if (unavailableTaskId !== undefined && selectedFocusTaskId === unavailableTaskId) {
      selectedFocusTaskId = null;
    }
    void refreshPomodoro();
  }

  function handleActivationId(id: number): number | null {
    if (!Number.isSafeInteger(id) || id <= 0) return null;
    notificationActivationId = id;
    notificationActivationNonce += 1;
    notificationActivationError = null;
    return notificationActivationNonce;
  }

  let floatingIntentDrain: Promise<void> = Promise.resolve();

  async function applyFloatingIntent(
    intent: { view: string; taskId: number | null },
    isDisposed: () => boolean
  ): Promise<void> {
    if (isDisposed()) return;
    if (intent.view === 'focus') {
      activeView = 'focus';
      await refreshPomodoro(isDisposed);
      return;
    }
    if (intent.view === 'tasks' && intent.taskId !== null) {
      activeView = 'tasks';
      const nonce = handleActivationId(intent.taskId);
      if (nonce !== null) await tick();
    }
  }

  function drainPendingFloatingIntent(isDisposed: () => boolean): void {
    floatingIntentDrain = floatingIntentDrain
      .then(async () => {
        if (isDisposed()) return;
        const intent = await takePendingFloatingIntent();
        if (isDisposed() || intent === null) return;
        await applyFloatingIntent(intent, isDisposed);
      })
      .catch((error) => {
        if (!isDisposed()) notificationActivationError = `悬浮窗跳转失败：${errorMessage(error)}`;
      });
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
  let pomodoroActivationDrain: Promise<void> = Promise.resolve();

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

  function hasValidPomodoroActivationId(activation: PendingPomodoroActivation): boolean {
    return Number.isSafeInteger(activation.id) && activation.id > 0;
  }

  function isValidPomodoroActivation(activation: PendingPomodoroActivation): boolean {
    return hasValidPomodoroActivationId(activation) &&
      Number.isSafeInteger(activation.sessionId) && activation.sessionId > 0 &&
      Number.isSafeInteger(activation.receivedAtUnixMs) && activation.receivedAtUnixMs >= 0;
  }

  function drainPendingPomodoroActivations(isDisposed: () => boolean): void {
    pomodoroActivationDrain = pomodoroActivationDrain
      .then(async () => {
        if (isDisposed()) return;
        const activations = await claimPendingPomodoroActivations(uiRunId);
        if (isDisposed() || activations.length === 0) return;

        const activation = activations[0];
        if (!activation) return;
        if (!isValidPomodoroActivation(activation)) {
          if (!hasValidPomodoroActivationId(activation)) {
            pomodoroActivationWarning = '收到无法安全确认的专注激活记录，请检查本地数据。';
            return;
          }
          const result = await acknowledgePendingPomodoroActivations(uiRunId, [activation.id]);
          if (isDisposed()) return;
          pomodoroActivationWarning = result.acknowledgedIds.includes(activation.id)
            ? '已忽略一条无效的专注激活记录。'
            : '无效的专注激活记录未能丢弃，将自动重试。';
          return;
        }

        activeView = 'focus';
        if (!await refreshPomodoro(isDisposed) || isDisposed()) return;

        const result = await acknowledgePendingPomodoroActivations(uiRunId, [activation.id]);
        if (isDisposed()) return;
        pomodoroActivationWarning = result.acknowledgedIds.includes(activation.id)
          ? null
          : '专注激活已显示，但确认状态未保存，将自动重试。';
      })
      .catch((error) => {
        if (!isDisposed()) pomodoroActivationWarning = `待处理专注激活消费失败：${errorMessage(error)}`;
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

  async function handleToggleFloatingWindow(): Promise<void> {
    if (!tauriAvailable) return;
    floatingWindowBusy = true;
    floatingWindowError = null;
    try {
      await toggleFloatingWindow();
    } catch (error) {
      floatingWindowError = `悬浮窗操作失败：${errorMessage(error)}`;
    } finally {
      floatingWindowBusy = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlistenActivation: (() => void) | undefined;
    let unlistenReminderWarning: (() => void) | undefined;
    let unlistenPomodoroState: (() => void) | undefined;
    let unlistenPomodoroActivation: (() => void) | undefined;
    let unlistenFloatingIntent: (() => void) | undefined;
    let pomodoroStateListenerReady = false;
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
    tauriAvailable = isTauriRuntime();
    initialized = true;
    if (windowKind === 'floating') return;
    if (!tauriAvailable) return;

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

    void refreshPomodoro(isDisposed);
    drainPendingPomodoroActivations(isDisposed);

    reminderWarningPollingTimer = window.setInterval(() => {
      drainPendingActivations(isDisposed);
      drainPendingPomodoroActivations(isDisposed);
      if (pomodoroRefreshWarning !== null || !pomodoroStateListenerReady) {
        void refreshPomodoro(isDisposed);
      }
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

      try {
        const unlisten = await listen<PomodoroSnapshot>('pomodoro-state-changed', () => {
          if (!disposed) void refreshPomodoro(isDisposed);
        });
        if (disposed) { unlisten(); return; }
        unlistenPomodoroState = unlisten;
        pomodoroStateListenerReady = true;
      } catch (error) {
        if (!disposed) pomodoroListenerWarning = `专注状态监听失败，已改用定期刷新：${errorMessage(error)}`;
      }

      try {
        const unlisten = await listen('pomodoro-activation', () => {
          if (!disposed) drainPendingPomodoroActivations(isDisposed);
        });
        if (disposed) { unlisten(); return; }
        unlistenPomodoroActivation = unlisten;
      } catch (error) {
        if (!disposed) pomodoroListenerWarning = `专注激活监听失败，已改用定期轮询：${errorMessage(error)}`;
      }

      try {
        const unlisten = await listen('floating-intent-available', () => {
          if (!disposed) drainPendingFloatingIntent(isDisposed);
        });
        if (disposed) { unlisten(); return; }
        unlistenFloatingIntent = unlisten;
      } catch (error) {
        if (!disposed) notificationActivationError = `悬浮窗跳转监听失败：${errorMessage(error)}`;
      }

      if (disposed) return;
      drainPendingActivations(isDisposed);
      drainPendingFloatingIntent(isDisposed);

      if (!disposed) await registerReminderWarningListener();
    })();

    void (async () => {
      try {
        const preferences = await invoke<WindowPreferences>('get_window_preferences');
        if (!disposed) {
          windowMode = preferences.lastImmersive
            ? 'fullscreen'
            : preferences.maximized
              ? 'maximized'
              : 'normal';
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
      unlistenPomodoroState?.();
      unlistenPomodoroActivation?.();
      unlistenFloatingIntent?.();
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

{#if windowKind === 'floating'}
  <FloatingWindow {tauriAvailable} />
{:else}
<main aria-labelledby="page-title">
  <header class="topbar">
    <div class="identity"><span class="mark" aria-hidden="true"></span><div><p class="eyebrow">STAR TODO</p><h1 id="page-title">{activeView === 'focus' ? '专注' : '任务'}</h1></div></div>
    <div class="shell-actions">
      <p class="status" aria-live="polite"><span class:offline={!tauriAvailable} class="status-dot"></span>{#if !initialized}正在初始化{:else if tauriAvailable}已连接{:else}浏览器预览{/if}</p>
      <button type="button" onclick={handleToggleFloatingWindow} disabled={!tauriAvailable || floatingWindowBusy}>
        {floatingWindowBusy ? '…' : '悬浮窗'}
      </button>
      {#if floatingWindowError}<p class="error" role="alert">{floatingWindowError}</p>{/if}
    </div>
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
  {#if tauriAvailable && activeView === 'tasks'}
    <FocusMiniBar
      {tauriAvailable}
      snapshot={pomodoroSnapshot}
      compact={false}
      busy={pomodoroBusy}
      warning={pomodoroWarning}
      onOpenFocus={() => activeView = 'focus'}
      onPause={() => runPomodoroCommand(pausePomodoro)}
      onResume={() => runPomodoroCommand(resumePomodoro)}
    />
  {/if}

  {#if activeView === 'focus'}
    <FocusWorkspace
      {tauriAvailable}
      snapshot={pomodoroSnapshot}
      tasks={pomodoroTaskSummaries}
      bind:selectedTaskId={selectedFocusTaskId}
      busy={pomodoroBusy}
      warning={pomodoroWarning}
      onStart={(input: StartPomodoroInput) => runPomodoroCommand(() => startPomodoro(input), input.taskId)}
      onPause={() => runPomodoroCommand(pausePomodoro)}
      onResume={() => runPomodoroCommand(resumePomodoro)}
      onSkip={() => runPomodoroCommand(skipPomodoro)}
      onReset={() => runPomodoroCommand(resetPomodoro)}
      onUpdateSettings={(input: UpdatePomodoroSettingsInput) => runPomodoroCommand(() => updatePomodoroSettings(input))}
    />
  {/if}

  {#if activeView === 'tasks'}
    <TaskWorkspace
      {tauriAvailable}
      {initialized}
      compact={false}
      activationId={notificationActivationId}
      activationNonce={notificationActivationNonce}
      {pomodoroCounts}
      onStartFocus={selectFocusTask}
      onPomodoroTasksChanged={handlePomodoroTasksChanged}
      onReminderReconcile={requestReminderReconcile}
      onMutationWarning={handleMutationWarning}
      onActivationResolved={acknowledgeResolvedActivation}
    />
  {/if}
  <nav class="primary-nav" aria-label="主导航">
    <button type="button" class:active={activeView === 'tasks'} aria-current={activeView === 'tasks' ? 'page' : undefined} onclick={() => activeView = 'tasks'}>任务</button>
    <button type="button" class:active={activeView === 'focus'} aria-current={activeView === 'focus' ? 'page' : undefined} onclick={() => activeView = 'focus'}>专注</button>
  </nav>
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
{/if}

<style>
  main { position:relative; width:min(100%, 920px); min-height:100vh; margin:0 auto; padding:18px clamp(14px, 4vw, 36px) 26px; background:linear-gradient(135deg, rgba(31,34,39,.88), rgba(17,18,20,.98)); border-inline:1px solid rgba(255,255,255,.05); }.topbar { display:flex; align-items:center; justify-content:space-between; gap:14px; padding-bottom:50px; border-bottom:1px solid var(--line); }.identity,.shell-actions,.status { display:flex; align-items:center; }.identity { gap:11px; }.shell-actions { gap:16px; }.shell-actions button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:5px 8px; color:var(--text-soft); background:var(--surface); font-size:12px; }.shell-actions button:hover:not(:disabled) { border-color:var(--text-soft); background:var(--surface-hover); color:var(--text); }.shell-actions .error { margin:0; color:var(--danger); font-size:11px; }.mark { width:9px; height:9px; border-radius:50%; background:var(--info); box-shadow:0 0 16px rgba(91,169,255,.62); }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h1 { margin:3px 0 0; font-size:20px; font-weight:640; }.primary-nav { position:absolute; z-index:1; top:62px; left:clamp(14px, 4vw, 36px); display:flex; gap:3px; padding:3px; border:1px solid var(--line); border-radius:var(--radius-sm); background:var(--surface); }.primary-nav button { border:0; border-radius:3px; padding:5px 9px; color:var(--muted); background:transparent; font-size:12px; }.primary-nav button.active { color:var(--text); background:var(--surface-hover); }.status { gap:7px; margin:0; color:var(--muted); font-size:12px; }.status-dot { width:7px; height:7px; border-radius:50%; background:var(--success); }.status-dot.offline { background:var(--warning, #e0ad65); }.page-alert { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-top:14px; padding:10px 11px; color:#f2d5a1; background:rgba(224,173,101,.09); border-left:2px solid #e0ad65; font-size:12px; }.page-alert p { margin:0; line-height:1.5; }.page-alert button { flex:none; border:1px solid rgba(224,173,101,.45); border-radius:var(--radius-sm); padding:5px 8px; color:#f2d5a1; background:transparent; font-size:11px; } @media (max-width:620px) { main { padding-inline:14px; }.topbar { align-items:flex-start; }.shell-actions { align-items:flex-end; flex-direction:column-reverse; gap:8px; }.page-alert { align-items:flex-start; flex-direction:column; } } @media (max-width:360px) { .topbar { gap:8px; }.status { font-size:11px; }.primary-nav button { padding-inline:7px; } }
</style>
