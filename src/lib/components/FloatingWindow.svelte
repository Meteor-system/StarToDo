<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import FloatingCapsule from '$lib/components/FloatingCapsule.svelte';
  import FloatingExpandedPanel from '$lib/components/FloatingExpandedPanel.svelte';
  import {
    createFloatingDisplayState,
    floatingFocusTargetSelector,
    floatingSizeMode,
    reduceFloatingDisplay,
    type FloatingDisplayEvent,
    type FloatingFocusTarget,
    type FloatingSizeMode
  } from '$lib/floating-display';
  import {
    errorMessage,
    listProjects,
    listTasks,
    localDateForEpochMs,
    openFocusFromFloating,
    openTaskFromFloating,
    sortTasks,
    todayLocalDate,
    type Project,
    type Task
  } from '$lib/tasks';
  import {
    acceptPomodoroCommand,
    acceptPomodoroRead,
    beginPomodoroOperation,
    createPomodoroCoordinationState,
    pomodoroSnapshotAccepted
  } from '$lib/floating-coordinator';
  import {
    acceptFloatingPreferencesRead,
    acceptFloatingReconciliation,
    acceptFloatingResetFailure,
    acceptFloatingResetSuccess,
    acceptFloatingSizeFailure,
    acceptFloatingSizeSuccess,
    beginFloatingResetRequest,
    beginFloatingSizeRequest,
    createFloatingSizeSyncState,
    failFloatingPreferencesRead,
    nextFloatingResetRequest,
    nextFloatingSizeRequest,
    requestFloatingReset,
    requestFloatingSize,
    retryFloatingSize
  } from '$lib/floating-size-sync';
  import { readUiPreferences, writeFloatingExpansionPreference } from '$lib/ui-preferences';
  import {
    getFloatingWindowPreferences,
    hideFloatingWindow,
    resetFloatingAutoSize,
    setFloatingDisplayMode,
    type FloatingWindowPreferences
  } from '$lib/windowing';
  import {
    formatPomodoroDuration,
    getPomodoroView,
    pausePomodoro,
    pomodoroPhaseLabel,
    pomodoroPrimaryAction,
    pomodoroStatusLabel,
    remainingPomodoroSeconds,
    resumePomodoro,
    startPomodoro,
    type PomodoroMutationResult,
    type PomodoroSnapshot
  } from '$lib/pomodoro';

  let { tauriAvailable }: { tauriAvailable: boolean } = $props();

  let tasks = $state<Task[]>([]);
  let projects = $state<Project[]>([]);
  let pomodoroCoordination = $state(createPomodoroCoordinationState<PomodoroSnapshot>());
  let nowUnixMs = $state(Date.now());
  let pomodoroBusy = $state(false);
  let tasksWarning = $state<string | null>(null);
  let mutationWarning = $state<string | null>(null);
  let sizeWarning = $state<string | null>(null);
  const initialUiPreferences = readUiPreferences();
  let uiPreferences = $state(initialUiPreferences);
  let display = $state(createFloatingDisplayState(false, initialUiPreferences.floatingExpansion === 'always'));
  let sizeSync = $state(createFloatingSizeSyncState<FloatingWindowPreferences>());
  let root: HTMLElement;

  let unlistenTasks: (() => void) | undefined;
  let unlistenPomodoro: (() => void) | undefined;
  let runtimeInitialized = false;
  let clockTimer: ReturnType<typeof window.setInterval> | undefined;
  let tasksTimer: ReturnType<typeof window.setInterval> | undefined;
  let pomodoroTimer: ReturnType<typeof window.setInterval> | undefined;
  let collapseTimer: ReturnType<typeof window.setTimeout> | undefined;
  let disposed = false;
  let taskRefreshSequence = 0;
  let lastAcceptedFocusActive = false;
  let sizeFlushQueued = false;
  let pendingFocusRestore: FloatingFocusTarget | null = null;
  let focusRestoreSequence = 0;
  let previousHtmlMinWidth: string | undefined;
  let previousHtmlOverflow: string | undefined;
  let previousBodyMinWidth: string | undefined;
  let previousBodyOverflow: string | undefined;

  function isTauriRuntime(): boolean {
    return tauriAvailable && typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
  }

  let snapshot = $derived(pomodoroCoordination.snapshot);
  let pomodoroWarning = $derived(
    mutationWarning ?? pomodoroCoordination.notificationWarning ?? pomodoroCoordination.readWarning
  );
  let floatingPreferences = $derived(sizeSync.preferences);
  let visibleTasks = $derived(sortTasks(tasks.filter((task) => {
    if (task.deletedAtUnixMs !== null || task.completedAtUnixMs !== null) return false;
    const today = todayLocalDate();
    const dueDate = task.dueAtUnixMs === null ? null : localDateForEpochMs(task.dueAtUnixMs);
    if (task.dueAtUnixMs !== null && task.dueAtUnixMs < nowUnixMs) return true;
    return task.plannedDate === today || dueDate === today;
  })));
  let activeSession = $derived(snapshot?.currentSession ?? null);
  let focusActive = $derived(activeSession?.status === 'running' || activeSession?.status === 'paused');
  let remainingLabel = $derived(formatPomodoroDuration(remainingPomodoroSeconds(activeSession, nowUnixMs)));
  let primaryAction = $derived(pomodoroPrimaryAction(snapshot));
  let phaseLabel = $derived(snapshot ? pomodoroPhaseLabel(activeSession?.phase ?? snapshot.recommendedPhase) : '专注');
  let statusLabel = $derived(activeSession ? pomodoroStatusLabel(activeSession.status) : '未开始');
  let taskTitle = $derived(activeSession?.taskTitleSnapshot ?? null);

  function scheduleCollapse(): void {
    if (collapseTimer !== undefined) {
      window.clearTimeout(collapseTimer);
      collapseTimer = undefined;
    }
    if (display.collapseAt === null || disposed) return;
    collapseTimer = window.setTimeout(() => {
      collapseTimer = undefined;
      dispatchFloating({ type: 'timeout', at: Date.now() });
    }, Math.max(0, display.collapseAt - Date.now()));
  }

  function dispatchFloating(event: FloatingDisplayEvent): void {
    const next = reduceFloatingDisplay(display, event);
    if (next !== display) {
      display = next;
      scheduleCollapse();
    }
    requestSizeSync(floatingSizeMode(next.mode), event.type !== 'timeout');
  }

  function focusTargetFromEvent(event: FocusEvent): FloatingFocusTarget | null {
    if (!(event.target instanceof HTMLElement)) return null;
    const target = event.target.closest<HTMLElement>('[data-floating-focus-target]');
    const value = target?.dataset.floatingFocusTarget;
    return value === 'open-focus' || value === 'primary' || value === 'expand' ? value : null;
  }

  function handleFocusIn(event: FocusEvent): void {
    const target = focusTargetFromEvent(event);
    if (display.mode === 'capsule' && target !== null) {
      pendingFocusRestore = target;
      const sequence = ++focusRestoreSequence;
      dispatchFloating({ type: 'focus-in', at: Date.now() });
      void restoreLogicalFocus(target, sequence);
      return;
    }
    dispatchFloating({ type: 'focus-in', at: Date.now() });
  }

  async function restoreLogicalFocus(target: FloatingFocusTarget, sequence: number): Promise<void> {
    await tick();
    if (disposed || sequence !== focusRestoreSequence) return;
    const preferred = root.querySelector<HTMLElement>(floatingFocusTargetSelector(target));
    preferred?.focus();
    if (!root.contains(document.activeElement)) {
      root.querySelector<HTMLElement>('[data-floating-focus-target]:not(:disabled)')?.focus();
    }
    if (sequence !== focusRestoreSequence) return;
    pendingFocusRestore = null;
    if (!root.contains(document.activeElement)) {
      dispatchFloating({ type: 'focus-out', at: Date.now() });
    }
  }

  function applyAcceptedSnapshot(nextSnapshot: PomodoroSnapshot): void {
    const nextFocusActive = nextSnapshot.currentSession?.status === 'running' || nextSnapshot.currentSession?.status === 'paused';
    if (nextFocusActive !== lastAcceptedFocusActive) {
      lastAcceptedFocusActive = nextFocusActive;
      dispatchFloating({ type: 'snapshot', focusActive: nextFocusActive, at: Date.now() });
    }
  }

  async function refreshPomodoro(): Promise<void> {
    if (!isTauriRuntime()) return;
    const operation = beginPomodoroOperation(pomodoroCoordination);
    pomodoroCoordination = operation.state;
    try {
      const view = await getPomodoroView();
      if (disposed) return;
      const previous = pomodoroCoordination;
      pomodoroCoordination = acceptPomodoroRead(previous, operation.token, view.snapshot, null);
      if (pomodoroCoordination !== previous) applyAcceptedSnapshot(view.snapshot);
    } catch (error) {
      if (disposed) return;
      pomodoroCoordination = acceptPomodoroRead(
        pomodoroCoordination,
        operation.token,
        null,
        `专注状态读取失败：${errorMessage(error)}`
      );
    }
  }

  async function refreshTasks(): Promise<void> {
    if (!isTauriRuntime()) return;
    const sequence = ++taskRefreshSequence;
    try {
      const [nextTasks, nextProjects] = await Promise.all([listTasks(), listProjects()]);
      if (disposed || sequence !== taskRefreshSequence) return;
      tasks = nextTasks;
      projects = nextProjects;
      tasksWarning = null;
    } catch (error) {
      if (disposed || sequence !== taskRefreshSequence) return;
      tasksWarning = `任务读取失败：${errorMessage(error)}`;
    }
  }

  async function runPomodoroCommand(command: () => Promise<PomodoroMutationResult>): Promise<void> {
    if (!isTauriRuntime() || pomodoroBusy) return;
    pomodoroBusy = true;
    mutationWarning = null;
    const operation = beginPomodoroOperation(pomodoroCoordination);
    pomodoroCoordination = operation.state;
    try {
      const result = await command();
      if (disposed) return;
      const snapshotAccepted = pomodoroSnapshotAccepted(pomodoroCoordination, operation.token);
      pomodoroCoordination = acceptPomodoroCommand(
        pomodoroCoordination,
        operation.token,
        result.snapshot,
        result.notificationWarning ?? null
      );
      if (snapshotAccepted) applyAcceptedSnapshot(result.snapshot);
      void refreshPomodoro();
    } catch (error) {
      if (!disposed) mutationWarning = `专注操作失败：${errorMessage(error)}`;
    } finally {
      if (!disposed) pomodoroBusy = false;
    }
  }

  function handlePrimaryAction(): void {
    if (primaryAction === 'pause') void runPomodoroCommand(pausePomodoro);
    else if (primaryAction === 'resume') void runPomodoroCommand(resumePomodoro);
    else if (primaryAction === 'start') {
      void runPomodoroCommand(() => startPomodoro({ phase: snapshot?.recommendedPhase ?? 'focus', taskId: null }));
    }
  }

  async function openFocus(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await openFocusFromFloating();
    } catch (error) {
      if (!disposed) mutationWarning = `无法打开专注工作区：${errorMessage(error)}`;
    }
  }

  async function openTask(taskId: number): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await openTaskFromFloating(taskId);
    } catch (error) {
      if (!disposed) tasksWarning = `无法定位任务：${errorMessage(error)}`;
    }
  }

  async function hideWindow(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await hideFloatingWindow();
    } catch (error) {
      if (!disposed) tasksWarning = `无法隐藏悬浮窗：${errorMessage(error)}`;
    }
  }

  function changeAlwaysExpanded(value: boolean): void {
    uiPreferences = { ...uiPreferences, floatingExpansion: value ? 'always' : 'auto' };
    writeFloatingExpansionPreference(uiPreferences.floatingExpansion);
    dispatchFloating({ type: 'always-expanded', value, at: Date.now() });
  }

  async function reconcileFloatingPreferences(generation: number): Promise<void> {
    try {
      const next = await getFloatingWindowPreferences();
      if (disposed || sizeSync.desired?.generation !== generation) return;
      sizeSync = acceptFloatingReconciliation(sizeSync, { ...sizeSync.desired }, next);
    } catch {
      // The original command warning is more actionable; reconciliation is best effort.
    }
  }

  function resetAutoSize(): void {
    if (!isTauriRuntime()) return;
    sizeSync = requestFloatingReset(sizeSync);
    void flushSizeSync();
  }

  function requestSizeSync(mode: FloatingSizeMode, explicit = false): void {
    if (!isTauriRuntime()) return;
    const failedCurrent = sizeSync.failed?.generation === sizeSync.desired?.generation;
    if (explicit && failedCurrent) sizeSync = retryFloatingSize(sizeSync);
    if (sizeSync.desired?.mode !== mode) sizeSync = requestFloatingSize(sizeSync, mode);
    if (sizeFlushQueued || nextFloatingSizeRequest(sizeSync) === null) return;
    sizeFlushQueued = true;
    queueMicrotask(() => {
      sizeFlushQueued = false;
      void flushSizeSync();
    });
  }

  async function flushSizeSync(): Promise<void> {
    if (disposed || !isTauriRuntime()) return;
    if (sizeSync.inFlight !== null || sizeSync.resetInFlight !== null) return;

    const resetRequest = nextFloatingResetRequest(sizeSync);
    if (resetRequest !== null) {
      sizeSync = beginFloatingResetRequest(sizeSync, resetRequest);
      try {
        const next = await resetFloatingAutoSize();
        if (disposed) return;
        sizeSync = acceptFloatingResetSuccess(sizeSync, resetRequest, next);
        if (sizeSync.generation === resetRequest.generation) sizeWarning = null;
      } catch (error) {
        if (disposed) return;
        sizeSync = acceptFloatingResetFailure(sizeSync, resetRequest);
        if (sizeSync.generation === resetRequest.generation) {
          sizeWarning = `恢复自动尺寸失败：${errorMessage(error)}`;
        }
      } finally {
        if (!disposed) void flushSizeSync();
      }
      return;
    }

    const request = nextFloatingSizeRequest(sizeSync);
    if (request === null) return;
    sizeSync = beginFloatingSizeRequest(sizeSync, request);
    try {
      const next = await setFloatingDisplayMode(request.mode);
      if (disposed) return;
      sizeSync = acceptFloatingSizeSuccess(sizeSync, request, next);
      if (sizeSync.desired?.generation === request.generation) sizeWarning = null;
    } catch (error) {
      if (disposed) return;
      sizeSync = acceptFloatingSizeFailure(sizeSync, request);
      if (sizeSync.desired?.generation === request.generation) {
        sizeWarning = `悬浮窗尺寸同步失败：${errorMessage(error)}`;
      }
      await reconcileFloatingPreferences(request.generation);
    } finally {
      if (!disposed) void flushSizeSync();
    }
  }

  function activateFloating(): void {
    dispatchFloating({ type: 'activate', at: Date.now() });
  }

  function handleFocusOut(event: FocusEvent): void {
    if (pendingFocusRestore !== null) return;
    if (event.relatedTarget instanceof Node && root.contains(event.relatedTarget)) return;
    dispatchFloating({ type: 'focus-out', at: Date.now() });
  }

  function initializeRuntime(): void {
    if (runtimeInitialized || disposed || !isTauriRuntime()) return;
    runtimeInitialized = true;
    void refreshPomodoro();
    void refreshTasks();
    void (async () => {
      try {
        const next = await getFloatingWindowPreferences();
        if (disposed) return;
        sizeSync = acceptFloatingPreferencesRead(sizeSync, next);
      } catch (error) {
        if (disposed) return;
        sizeSync = failFloatingPreferencesRead(sizeSync);
        sizeWarning = `悬浮窗偏好读取失败：${errorMessage(error)}`;
      } finally {
        if (!disposed) requestSizeSync(floatingSizeMode(display.mode));
      }
    })();

    tasksTimer = window.setInterval(() => { void refreshTasks(); }, 60_000);
    pomodoroTimer = window.setInterval(() => { void refreshPomodoro(); }, 15_000);

    void (async () => {
      try {
        const unlisten = await listen('tasks-changed', () => { if (!disposed) void refreshTasks(); });
        if (disposed) unlisten(); else unlistenTasks = unlisten;
      } catch (error) {
        if (!disposed) tasksWarning = `任务变更监听失败，已改用定期刷新：${errorMessage(error)}`;
      }

      try {
        const unlisten = await listen('pomodoro-state-changed', () => { if (!disposed) void refreshPomodoro(); });
        if (disposed) unlisten(); else unlistenPomodoro = unlisten;
      } catch (error) {
        if (!disposed) {
          pomodoroCoordination = {
            ...pomodoroCoordination,
            readWarning: `专注状态监听失败，已改用定期刷新：${errorMessage(error)}`
          };
        }
      }
    })();
  }

  $effect(() => {
    if (tauriAvailable) initializeRuntime();
  });

  onMount(() => {
    previousHtmlMinWidth = document.documentElement.style.minWidth;
    previousHtmlOverflow = document.documentElement.style.overflow;
    previousBodyMinWidth = document.body.style.minWidth;
    previousBodyOverflow = document.body.style.overflow;
    document.documentElement.style.minWidth = '0';
    document.documentElement.style.overflow = 'hidden';
    document.body.style.minWidth = '0';
    document.body.style.overflow = 'hidden';

    scheduleCollapse();
    requestSizeSync(floatingSizeMode(display.mode));
    clockTimer = window.setInterval(() => { nowUnixMs = Date.now(); }, 1_000);
    initializeRuntime();
  });

  onDestroy(() => {
    disposed = true;
    ++taskRefreshSequence;
    pomodoroCoordination = beginPomodoroOperation(pomodoroCoordination).state;
    if (clockTimer !== undefined) window.clearInterval(clockTimer);
    if (tasksTimer !== undefined) window.clearInterval(tasksTimer);
    if (pomodoroTimer !== undefined) window.clearInterval(pomodoroTimer);
    if (collapseTimer !== undefined) window.clearTimeout(collapseTimer);
    unlistenTasks?.();
    unlistenPomodoro?.();
    if (previousHtmlMinWidth !== undefined) document.documentElement.style.minWidth = previousHtmlMinWidth;
    if (previousHtmlOverflow !== undefined) document.documentElement.style.overflow = previousHtmlOverflow;
    if (previousBodyMinWidth !== undefined) document.body.style.minWidth = previousBodyMinWidth;
    if (previousBodyOverflow !== undefined) document.body.style.overflow = previousBodyOverflow;
  });
</script>

<!-- svelte-ignore a11y_no_redundant_roles: binding brief requires the explicit region role. -->
<section
  bind:this={root}
  class="floating-window"
  role="region"
  aria-label="StarToDo 悬浮窗"
  data-display-mode={display.mode}
  onpointerenter={() => dispatchFloating({ type: 'pointer-enter', at: Date.now() })}
  onpointerleave={() => dispatchFloating({ type: 'pointer-leave', at: Date.now() })}
  onfocusin={handleFocusIn}
  onfocusout={handleFocusOut}
>
  <div class="panel" class:capsule={display.mode === 'capsule'}>
    {#if display.mode === 'capsule'}
      <FloatingCapsule
        {phaseLabel}
        {remainingLabel}
        {taskTitle}
        paused={statusLabel === '已暂停'}
        busy={pomodoroBusy || primaryAction === 'none' || !tauriAvailable}
        onOpenFocus={() => void openFocus()}
        onPrimary={handlePrimaryAction}
        onExpand={activateFloating}
      />
    {:else}
      <FloatingExpandedPanel
        {focusActive}
        {phaseLabel}
        {statusLabel}
        {remainingLabel}
        {taskTitle}
        tasks={visibleTasks}
        {projects}
        busy={pomodoroBusy || primaryAction === 'none' || !tauriAvailable}
        alwaysExpanded={display.alwaysExpanded}
        userResized={floatingPreferences?.userResized ?? false}
        onPrimary={handlePrimaryAction}
        onOpenFocus={() => void openFocus()}
        onOpenTask={(taskId) => void openTask(taskId)}
        onHide={() => void hideWindow()}
        onAlwaysExpandedChange={changeAlwaysExpanded}
        onResetAutoSize={() => void resetAutoSize()}
      />
    {/if}
  </div>
  {#if tasksWarning || pomodoroWarning || sizeWarning}
    <div class="warnings" role="alert">
      {#if tasksWarning}<p>{tasksWarning}</p>{/if}
      {#if pomodoroWarning}<p>{pomodoroWarning}</p>{/if}
      {#if sizeWarning}<p>{sizeWarning}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .floating-window {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    box-sizing: border-box;
    color: var(--text, #f1f3f5);
    background: var(--surface, #181a1e);
    border: 1px solid var(--line-strong, rgba(232, 235, 240, 0.22));
    border-radius: 10px;
    user-select: none;
  }

  .panel {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    animation: expand-in 180ms ease-out both;
  }

  .panel.capsule {
    animation: capsule-in 240ms ease-out both;
  }

  .warnings {
    position: absolute;
    right: 8px;
    bottom: 7px;
    left: 8px;
    max-height: 44px;
    overflow: auto;
    color: var(--danger, #ffaaa1);
    background: var(--surface, #181a1e);
    font-size: 10px;
    pointer-events: none;
  }

  .warnings p {
    margin: 0;
  }

  @keyframes expand-in {
    from { opacity: 0; transform: translateY(4px) scale(0.985); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  @keyframes capsule-in {
    from { opacity: 0; transform: translateY(-3px) scale(1.01); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel,
    .panel.capsule {
      animation: none;
    }
  }
</style>
