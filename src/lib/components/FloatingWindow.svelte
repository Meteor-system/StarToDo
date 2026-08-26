<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
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
  import { hideFloatingWindow } from '$lib/windowing';
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
  let snapshot = $state<PomodoroSnapshot | null>(null);
  let nowUnixMs = $state(Date.now());
  let pomodoroBusy = $state(false);
  let tasksWarning = $state<string | null>(null);
  let pomodoroWarning = $state<string | null>(null);

  let unlistenTasks: (() => void) | undefined;
  let unlistenPomodoro: (() => void) | undefined;
  let clockTimer: ReturnType<typeof window.setInterval> | undefined;
  let tasksTimer: ReturnType<typeof window.setInterval> | undefined;
  let disposed = false;
  let previousHtmlMinWidth: string | undefined;
  let previousBodyMinWidth: string | undefined;
  let previousBodyOverflow: string | undefined;

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
  }

  function projectName(task: Task): string {
    if (task.projectId === null) return '收件箱';
    return projects.find((project) => project.id === task.projectId)?.name ?? `项目 #${task.projectId}`;
  }

  function formatClockTime(epochMs: number): string {
    const date = new Date(epochMs);
    return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
  }

  function taskTimeLabel(task: Task): string {
    const today = todayLocalDate();
    const dueDate = task.dueAtUnixMs === null ? null : localDateForEpochMs(task.dueAtUnixMs);
    if (task.dueAtUnixMs !== null && task.dueAtUnixMs < nowUnixMs) return '已逾期';
    if (dueDate === today) {
      return task.dueAtUnixMs === null ? '今天' : `今天 ${formatClockTime(task.dueAtUnixMs)}`;
    }
    if (task.plannedDate === today) return '今天计划';
    return '未安排';
  }

  let visibleTasks = $derived(
    sortTasks(tasks.filter((task) => {
      if (task.deletedAtUnixMs !== null || task.completedAtUnixMs !== null) return false;
      const today = todayLocalDate();
      const dueDate = task.dueAtUnixMs === null ? null : localDateForEpochMs(task.dueAtUnixMs);
      if (task.dueAtUnixMs !== null && task.dueAtUnixMs < nowUnixMs) return true;
      return task.plannedDate === today || dueDate === today;
    }))
  );

  let activeSession = $derived(snapshot?.currentSession ?? null);
  let remainingSeconds = $derived(remainingPomodoroSeconds(activeSession, nowUnixMs));
  let remainingLabel = $derived(formatPomodoroDuration(remainingSeconds));
  let primaryAction = $derived(pomodoroPrimaryAction(snapshot));
  let phaseLabel = $derived(
    snapshot ? pomodoroPhaseLabel(activeSession?.phase ?? snapshot.recommendedPhase) : '专注'
  );
  let statusLabel = $derived(activeSession ? pomodoroStatusLabel(activeSession.status) : '未开始');
  let primaryActionLabel = $derived(
    primaryAction === 'pause' ? '暂停' :
    primaryAction === 'resume' ? '继续' :
    primaryAction === 'start' ? `开始${phaseLabel}` : '—'
  );

  async function refreshPomodoro(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      const view = await getPomodoroView();
      snapshot = view.snapshot;
      pomodoroWarning = null;
    } catch (error) {
      pomodoroWarning = `专注状态读取失败：${errorMessage(error)}`;
    }
  }

  async function refreshTasks(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      const [nextTasks, nextProjects] = await Promise.all([listTasks(), listProjects()]);
      tasks = nextTasks;
      projects = nextProjects;
      tasksWarning = null;
    } catch (error) {
      tasksWarning = `任务读取失败：${errorMessage(error)}`;
    }
  }

  async function runPomodoroCommand(
    command: () => Promise<PomodoroMutationResult>
  ): Promise<void> {
    if (!isTauriRuntime() || pomodoroBusy) return;
    pomodoroBusy = true;
    try {
      const result = await command();
      snapshot = result.snapshot;
      pomodoroWarning = result.notificationWarning ?? null;
    } catch (error) {
      pomodoroWarning = `专注操作失败：${errorMessage(error)}`;
    } finally {
      pomodoroBusy = false;
    }
  }

  function handlePrimaryAction(): void {
    if (primaryAction === 'pause') void runPomodoroCommand(pausePomodoro);
    else if (primaryAction === 'resume') void runPomodoroCommand(resumePomodoro);
    else if (primaryAction === 'start') {
      void runPomodoroCommand(() => startPomodoro({
        phase: snapshot?.recommendedPhase ?? 'focus',
        taskId: null
      }));
    }
  }

  async function openFocus(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await openFocusFromFloating();
    } catch (error) {
      pomodoroWarning = `无法打开专注工作区：${errorMessage(error)}`;
    }
  }

  async function openTask(taskId: number): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await openTaskFromFloating(taskId);
    } catch (error) {
      tasksWarning = `无法定位任务：${errorMessage(error)}`;
    }
  }

  async function hideWindow(): Promise<void> {
    if (!isTauriRuntime()) return;
    try {
      await hideFloatingWindow();
    } catch (error) {
      tasksWarning = `无法隐藏悬浮窗：${errorMessage(error)}`;
    }
  }

  function handleTimerKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      void openFocus();
    }
  }

  onMount(() => {
    previousHtmlMinWidth = document.documentElement.style.minWidth;
    previousBodyMinWidth = document.body.style.minWidth;
    previousBodyOverflow = document.body.style.overflow;
    document.documentElement.style.minWidth = '0';
    document.body.style.minWidth = '0';
    document.body.style.overflow = 'hidden';

    void refreshPomodoro();
    void refreshTasks();

    clockTimer = window.setInterval(() => {
      nowUnixMs = Date.now();
    }, 1_000);
    tasksTimer = window.setInterval(() => {
      void refreshTasks();
    }, 60_000);

    void (async () => {
      if (!isTauriRuntime()) return;

      try {
        const unlisten = await listen('tasks-changed', () => {
          if (!disposed) void refreshTasks();
        });
        if (disposed) {
          unlisten();
          return;
        }
        unlistenTasks = unlisten;
      } catch (error) {
        tasksWarning = `任务变更监听失败：${errorMessage(error)}`;
      }

      try {
        const unlisten = await listen('pomodoro-state-changed', () => {
          if (!disposed) void refreshPomodoro();
        });
        if (disposed) {
          unlisten();
          return;
        }
        unlistenPomodoro = unlisten;
      } catch (error) {
        pomodoroWarning = `专注状态监听失败，已改用定期刷新：${errorMessage(error)}`;
      }
    })();
  });

  onDestroy(() => {
    disposed = true;
    if (clockTimer !== undefined) window.clearInterval(clockTimer);
    if (tasksTimer !== undefined) window.clearInterval(tasksTimer);
    unlistenTasks?.();
    unlistenPomodoro?.();

    if (previousHtmlMinWidth !== undefined) {
      document.documentElement.style.minWidth = previousHtmlMinWidth;
    }
    if (previousBodyMinWidth !== undefined) {
      document.body.style.minWidth = previousBodyMinWidth;
    }
    if (previousBodyOverflow !== undefined) {
      document.body.style.overflow = previousBodyOverflow;
    }
  });
</script>

<section class="floating" aria-label="StarToDo 悬浮窗" data-tauri-drag-region="deep">
  <div class="drag-strip" data-tauri-drag-region aria-hidden="true"></div>
  <header class="floating-header">
    <button type="button" class="timer" onclick={() => void openFocus()} onkeydown={handleTimerKeydown} aria-label={`打开专注工作区，剩余 ${remainingLabel}`}>
      <span class="phase">{phaseLabel}</span>
      <strong>{remainingLabel}</strong>
      <span class="status">{statusLabel}{activeSession?.taskTitleSnapshot ? ` · ${activeSession.taskTitleSnapshot}` : ''}</span>
    </button>
    <div class="actions">
      <button type="button" class="primary" onclick={handlePrimaryAction} disabled={!tauriAvailable || pomodoroBusy || primaryAction === 'none'}>
        {pomodoroBusy ? '…' : primaryActionLabel}
      </button>
      <button type="button" class="hide" onclick={() => void hideWindow()} aria-label="隐藏悬浮窗">×</button>
    </div>
  </header>

  <div class="tasks">
    <div class="tasks-heading">
      <span>今天 + 逾期</span>
      <span class="count">{visibleTasks.length}</span>
    </div>
    {#if tasksWarning}
      <p class="warning" role="alert">{tasksWarning}</p>
    {/if}
    {#if pomodoroWarning}
      <p class="warning" role="alert">{pomodoroWarning}</p>
    {/if}
    {#if visibleTasks.length === 0}
      <p class="empty">今天没有待办任务</p>
    {:else}
      <ul>
        {#each visibleTasks.slice(0, 5) as task (task.id)}
          <li>
            <button type="button" class="task" onclick={() => void openTask(task.id)} aria-label={`打开任务：${task.title}`}>
              <span class:overdue={task.dueAtUnixMs !== null && task.dueAtUnixMs < nowUnixMs} class="task-title">{task.title}</span>
              <span class="task-meta">{projectName(task)} · {taskTimeLabel(task)}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if visibleTasks.length > 5}
        <p class="more">还有 {visibleTasks.length - 5} 个任务</p>
      {/if}
    {/if}
  </div>
</section>

<style>
  .floating {
    position: relative;
    display: grid;
    gap: 8px;
    width: 100%;
    min-width: 260px;
    height: 100%;
    min-height: 120px;
    padding: 8px 11px 11px;
    box-sizing: border-box;
    color: var(--text, #f1f3f5);
    background: var(--surface, #181a1e);
    border: 1px solid var(--line-strong, rgba(232, 235, 240, 0.22));
    border-radius: 10px;
    user-select: none;
    cursor: grab;
  }
  .floating:active {
    cursor: grabbing;
  }
  .drag-strip {
    height: 10px;
    border-radius: 6px;
    background-image: radial-gradient(circle, var(--muted, #949ba6) 1.3px, transparent 1.6px);
    background-size: 16px 10px;
    background-repeat: repeat-x;
    background-position: center;
    opacity: 0.75;
    cursor: grab;
  }
  .floating-header {
    position: relative;
    display: flex;
    align-items: stretch;
    gap: 8px;
  }
  .timer {
    position: relative;
    z-index: 1;
    display: grid;
    align-content: center;
    min-width: 0;
    flex: 1;
    border: 1px solid var(--line, rgba(232, 235, 240, 0.13));
    border-radius: var(--radius-md, 8px);
    padding: 5px 9px;
    color: inherit;
    background: var(--surface-raised, #202329);
    text-align: left;
    cursor: pointer;
  }
  .phase {
    color: var(--muted, #949ba6);
    font-size: 10px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .timer strong {
    margin-top: 2px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 24px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .status {
    margin-top: 3px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted, #949ba6);
    font-size: 10px;
  }
  .actions {
    position: relative;
    z-index: 1;
    display: grid;
    align-content: stretch;
    gap: 6px;
  }
  .actions button {
    border: 1px solid var(--line, rgba(232, 235, 240, 0.13));
    border-radius: var(--radius-sm, 5px);
    padding: 6px 8px;
    color: var(--text-soft, #c5c9d0);
    background: var(--surface-raised, #202329);
    font-size: 11px;
  }
  .actions .primary {
    min-width: 58px;
    border-color: var(--accent, #f36b32);
    color: var(--accent-ink, #251007);
    background: var(--accent, #f36b32);
    font-weight: 650;
  }
  .actions .hide {
    flex: 1;
    color: var(--muted, #949ba6);
  }
  .tasks {
    min-height: 0;
    flex: 1;
  }
  .tasks-heading {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    color: var(--muted, #949ba6);
    font-size: 10px;
    letter-spacing: 0.12em;
  }
  .count {
    color: var(--accent, #f36b32);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .tasks ul {
    display: grid;
    gap: 4px;
    max-height: 118px;
    margin: 7px 0 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }
  .task {
    display: grid;
    gap: 2px;
    width: 100%;
    border: 1px solid var(--line, rgba(232, 235, 240, 0.13));
    border-radius: var(--radius-sm, 5px);
    padding: 6px 8px;
    color: inherit;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .task-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .task-title.overdue {
    color: var(--danger, #ffaaa1);
  }
  .task-meta {
    color: var(--muted, #949ba6);
    font-size: 10px;
  }
  .empty,
  .more,
  .warning {
    margin: 7px 0 0;
    color: var(--muted, #949ba6);
    font-size: 11px;
    line-height: 1.45;
  }
  .warning {
    color: var(--danger, #ffaaa1);
  }
  button:hover:not(:disabled) {
    border-color: var(--text-soft, #c5c9d0);
    background: var(--surface-hover, #272b31);
    color: var(--text, #f1f3f5);
  }
  .actions .primary:hover:not(:disabled) {
    border-color: var(--accent-hover, #ff7a42);
    background: var(--accent-hover, #ff7a42);
    color: var(--accent-ink, #251007);
  }
  button:focus-visible {
    outline: 2px solid var(--info, #5ba9ff);
    outline-offset: 2px;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      transition: none !important;
      animation: none !important;
    }
  }
</style>
