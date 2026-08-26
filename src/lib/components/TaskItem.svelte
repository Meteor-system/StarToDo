<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { shouldToggleTaskFromKeyboard } from '$lib/task-interaction';
  import {
    epochMsToLocalDateTime,
    priorityLabel,
    recurrenceLabel,
    type MutationWarningResult,
    type Project,
    type Task,
    type TaskChangeKind,
    type TaskCompletionMutationWithToken,
    type TaskMutationWithToken
  } from '$lib/tasks';

  interface Props {
    task: Task;
    projects?: Project[];
    highlighted?: boolean;
    onOpenDetails: (taskId: number) => void;
    onCompleted: (id: number, completed: boolean) => Promise<TaskCompletionMutationWithToken>;
    onSnooze?: (id: number, untilUnixMs: number) => Promise<TaskMutationWithToken>;
    onDeferToTomorrow?: (id: number) => Promise<TaskMutationWithToken>;
    onChanged: (task: Task, kind: TaskChangeKind, result: TaskMutationWithToken) => Promise<boolean>;
    trashMode?: boolean;
    onRestore?: (id: number) => Promise<TaskMutationWithToken>;
    onPermanentlyDelete?: (id: number) => Promise<MutationWarningResult>;
    onRestored?: (task: Task, result: TaskMutationWithToken) => Promise<boolean>;
    onPermanentlyRemoved?: (id: number, result: MutationWarningResult) => Promise<boolean>;
    pomodoroCount?: number;
    onFocus?: (taskId: number) => void;
  }

  let {
    task,
    projects = [],
    highlighted = false,
    onOpenDetails,
    onCompleted,
    onSnooze,
    onDeferToTomorrow,
    onChanged,
    trashMode = false,
    onRestore,
    onPermanentlyDelete,
    onRestored,
    onPermanentlyRemoved,
    pomodoroCount = 0,
    onFocus
  }: Props = $props();
  let confirmingPermanentDelete = $state(false);
  let busy = $state(false);
  let requestError = $state<string | null>(null);
  let actionButton = $state<HTMLButtonElement>();
  let dialogTrigger = $state<HTMLButtonElement>();
  let taskProject = $derived(
    task.projectId === null ? null : projects.find((project) => project.id === task.projectId) ?? null
  );

  async function cancelPermanentDelete(): Promise<void> {
    confirmingPermanentDelete = false;
    await tick();
    dialogTrigger?.focus();
  }

  async function toggleCompleted(): Promise<void> {
    if (trashMode || busy) return;
    const completed = task.completedAtUnixMs === null;
    busy = true;
    requestError = null;
    try {
      const changed = await onCompleted(task.id, completed);
      await onChanged(changed.task, completed ? 'complete' : 'restore', changed);
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function handleSnooze(durationMs: number): Promise<void> {
    if (!onSnooze) return;
    busy = true;
    requestError = null;
    try {
      const changed = await onSnooze(task.id, Date.now() + durationMs);
      await onChanged(changed.task, 'snooze', changed);
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function handleDeferToTomorrow(): Promise<void> {
    if (!onDeferToTomorrow) return;
    busy = true;
    requestError = null;
    try {
      const changed = await onDeferToTomorrow(task.id);
      await onChanged(changed.task, 'defer', changed);
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function restoreFromTrash(): Promise<void> {
    if (!onRestore || !onRestored) return;
    busy = true;
    requestError = null;
    try {
      const changed = await onRestore(task.id);
      await onRestored(changed.task, changed);
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function openPermanentDeleteConfirm(event: MouseEvent): void {
    requestError = null;
    dialogTrigger = event.currentTarget as HTMLButtonElement;
    confirmingPermanentDelete = true;
  }

  async function permanentlyRemove(): Promise<void> {
    if (!onPermanentlyDelete || !onPermanentlyRemoved) return;
    busy = true;
    requestError = null;
    try {
      const result = await onPermanentlyDelete(task.id);
      await onPermanentlyRemoved(task.id, result);
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  id={`task-${task.id}`}
  class:complete={task.completedAtUnixMs !== null}
  class:activated={highlighted}
  class:trashed={trashMode}
  class="task"
  tabindex="0"
  onkeydown={(event) => {
    if (
      !trashMode &&
      shouldToggleTaskFromKeyboard(
        event.key,
        event.ctrlKey,
        event.metaKey,
        event.altKey,
        event.target === event.currentTarget
      )
    ) {
      event.preventDefault();
      void toggleCompleted();
    }
  }}
  transition:fly={{ y: 8, duration: 180 }}
>
  <div class="task-row">
    {#if trashMode}
      <span class="completion-placeholder" aria-hidden="true">·</span>
    {:else}
      <button
        type="button"
        bind:this={actionButton}
        id={`task-action-${task.id}`}
        class="completion"
        aria-label={task.completedAtUnixMs === null ? `完成：${task.title}` : `恢复：${task.title}`}
        aria-pressed={task.completedAtUnixMs !== null}
        onclick={toggleCompleted}
        disabled={busy}
      >{task.completedAtUnixMs === null ? '○' : '✓'}</button>
    {/if}
    <div class="summary">
      <div class="task-title">{task.title}</div>
      <div class="metadata">
        <span>{trashMode ? '已移入回收站' : task.completedAtUnixMs === null ? '未完成' : '已完成'}</span>
        {#if trashMode && task.deletedAtUnixMs !== null}<span>删除于 {epochMsToLocalDateTime(task.deletedAtUnixMs).replace('T', ' ')}</span>{/if}
        {#if task.plannedDate}<span>计划 {task.plannedDate}</span>{/if}
        {#if task.dueAtUnixMs !== null}<span>截止 {epochMsToLocalDateTime(task.dueAtUnixMs).replace('T', ' ')}</span>{/if}
        {#if task.reminderAtUnixMs !== null}<span>提醒 {epochMsToLocalDateTime(task.reminderAtUnixMs).replace('T', ' ')}</span>{/if}
        {#if taskProject}<span>项目 {taskProject.name}{#if taskProject.archivedAtUnixMs !== null}（已归档）{/if}</span>{/if}
        {#if task.priority !== 'none'}<span>优先级 {priorityLabel(task.priority)}</span>{/if}
        {#if task.recurrenceKind !== 'none'}<span>重复 {recurrenceLabel(task.recurrenceKind)}</span>{/if}
        {#if !trashMode && task.completedAtUnixMs === null && pomodoroCount > 0}<span>累计专注 {pomodoroCount}</span>{/if}
      </div>
    </div>
    <div class="actions">
      {#if trashMode}
        <button type="button" bind:this={actionButton} id={`task-action-${task.id}`} class="quiet" aria-label={`恢复：${task.title}`} onclick={restoreFromTrash} disabled={busy}>恢复</button>
        <button type="button" class="quiet danger" aria-label={`永久删除：${task.title}`} onclick={openPermanentDeleteConfirm} disabled={busy}>永久删除</button>
      {:else}
        <button type="button" class="quiet" aria-label={`查看详情：${task.title}`} onclick={() => onOpenDetails(task.id)} disabled={busy}>详情</button>
      {/if}
    </div>
  </div>

  {#if !trashMode && task.completedAtUnixMs === null && (onFocus || onSnooze || onDeferToTomorrow)}
    <div class="quick-actions" aria-label={`${task.title} 的快捷操作`}>
      {#if onFocus}<button type="button" class="focus-action" aria-label={`为“${task.title}”预选专注`} onclick={() => onFocus?.(task.id)} disabled={busy}>专注</button>{/if}
      {#if onSnooze}
        <button type="button" class="quiet" aria-label={`15 分钟后提醒：${task.title}`} onclick={() => void handleSnooze(15 * 60 * 1_000)} disabled={busy}>15 分钟后</button>
        <button type="button" class="quiet" aria-label={`1 小时后提醒：${task.title}`} onclick={() => void handleSnooze(60 * 60 * 1_000)} disabled={busy}>1 小时后</button>
      {/if}
      {#if onDeferToTomorrow}<button type="button" class="quiet" aria-label={`延期到明天：${task.title}`} onclick={() => void handleDeferToTomorrow()} disabled={busy}>延期到明天</button>{/if}
    </div>
  {/if}

  {#if task.notes}<p class="notes">{task.notes}</p>{/if}

  <ConfirmDialog
    open={confirmingPermanentDelete && trashMode}
    dialogId={`permanent-delete-task-${task.id}`}
    title="永久删除"
    description={`永久删除“${task.title}”？此操作无法恢复。`}
    confirmLabel="确认永久删除"
    {busy}
    error={confirmingPermanentDelete ? requestError : null}
    onConfirm={permanentlyRemove}
    onCancel={cancelPermanentDelete}
  />
  {#if requestError && !confirmingPermanentDelete}<p class="error" role="alert">{requestError}</p>{/if}
</article>

<style>
  .task { padding:14px 0; border-bottom:1px solid var(--line); outline:none; transition:background .15s ease, opacity .15s ease; }
  .task:focus-visible { margin-inline:-8px; padding-inline:8px; box-shadow:inset 2px 0 var(--info); background:var(--info-soft); }
  .task.complete { opacity:.66; }
  .task.trashed { opacity:.82; }
  .completion-placeholder { display:inline-flex; width:26px; height:26px; align-items:center; justify-content:center; color:var(--muted); font-size:20px; }
  .task.activated { margin-inline:-8px; padding-inline:8px; border-left:2px solid var(--info); background:var(--info-soft); }
  .task-row { display:flex; gap:10px; align-items:flex-start; }
  .completion { width:26px; height:26px; flex:none; padding:0; border:1px solid #777f8b; border-radius:50%; color:var(--accent); background:transparent; font-weight:700; transition:border-color .15s ease, background .15s ease, color .15s ease; }
  .completion:hover:not(:disabled) { border-color:var(--accent); background:var(--accent-soft); }
  .complete .completion { border-color:var(--accent); background:var(--accent); color:var(--accent-ink); }
  .summary { min-width:0; flex:1; }
  .task-title { overflow-wrap:anywhere; color:var(--text); font-size:14px; font-weight:620; line-height:1.4; }
  .complete .task-title { text-decoration:line-through; }
  .metadata { display:flex; gap:8px; flex-wrap:wrap; margin-top:4px; color:var(--muted); font-size:11px; }
  .actions,.quick-actions { display:flex; gap:6px; flex-wrap:wrap; }
  .quick-actions { margin:9px 0 0 36px; }
  .focus-action { border:1px solid color-mix(in srgb, var(--accent) 72%, var(--line)); border-radius:var(--radius-sm); padding:4px 8px; color:var(--accent-hover); background:var(--accent-soft); font-size:12px; font-weight:650; }
  .focus-action:hover:not(:disabled) { color:var(--accent-ink); background:var(--accent); }
  .quiet { border:0; border-radius:var(--radius-sm); padding:4px 6px; color:var(--muted); background:transparent; font-size:12px; }
  .quiet:hover:not(:disabled) { color:var(--text-soft); background:rgba(255,255,255,.05); }
  .danger { color:var(--danger); }
  .notes { margin:8px 0 0 36px; color:var(--text-soft); white-space:pre-wrap; font-size:12px; line-height:1.55; overflow-wrap:anywhere; }
  .error { margin:8px 0 0 36px; color:var(--danger); font-size:12px; }
  @media (prefers-reduced-motion: reduce) { .task,.completion { transition:none; } }
  @media (max-width:420px) { .task-row { display:grid; grid-template-columns:26px minmax(0,1fr); } .actions { grid-column:2; } .quick-actions,.notes,.error { margin-left:36px; } }
</style>
