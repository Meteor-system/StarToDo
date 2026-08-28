<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import {
    epochMsToLocalDateTime,
    localDateTimeToEpochMs,
    localTimeZone,
    PRIORITY_OPTIONS,
    priorityLabel,
    RECURRENCE_OPTIONS,
    recurrenceLabel,
    type Priority,
    type Project,
    type RecurrenceKind,
    type Task,
    type TaskChangeKind,
    type TaskCompletionMutationWithToken,
    type TaskField,
    type TaskFieldErrors,
    type TaskInput,
    type TaskMutationWithToken,
    type MutationWarningResult,
    validateTaskInput
  } from '$lib/tasks';

  interface Props {
    task: Task;
    projects?: Project[];
    highlighted?: boolean;
    onUpdate: (id: number, input: TaskInput) => Promise<TaskMutationWithToken>;
    onCompleted: (id: number, completed: boolean) => Promise<TaskCompletionMutationWithToken>;
    onSnooze?: (id: number, untilUnixMs: number) => Promise<TaskMutationWithToken>;
    onDeferToTomorrow?: (id: number) => Promise<TaskMutationWithToken>;
    onDelete: (id: number) => Promise<MutationWarningResult>;
    onChanged: (task: Task, kind: TaskChangeKind, result: TaskMutationWithToken) => Promise<boolean>;
    onRemoved: (id: number, result: MutationWarningResult) => Promise<boolean>;
    trashMode?: boolean;
    compactMode?: boolean;
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
    onUpdate,
    onCompleted,
    onSnooze,
    onDeferToTomorrow,
    onDelete,
    onChanged,
    onRemoved,
    trashMode = false,
    compactMode = false,
    onRestore,
    onPermanentlyDelete,
    onRestored,
    onPermanentlyRemoved,
    pomodoroCount = 0,
    onFocus
  }: Props = $props();
  let editing = $state(false);
  let confirmingDelete = $state(false);
  let confirmingPermanentDelete = $state(false);
  let busy = $state(false);
  let fieldErrors = $state<TaskFieldErrors>({});
  let requestError = $state<string | null>(null);
  let title = $state('');
  let notes = $state('');
  let plannedDate = $state('');
  let dueAt = $state('');
  let reminderAt = $state('');
  let priority = $state<Priority>('none');
  let recurrenceKind = $state<RecurrenceKind>('none');
  let recurrenceTimezone = $state<string | null>(null);
  let projectId = $state<number | null>(null);
  let titleInput = $state<HTMLInputElement>();
  let actionButton = $state<HTMLButtonElement>();
  let dialogTrigger = $state<HTMLButtonElement>();
  let taskProject = $derived(
    task.projectId === null ? null : projects.find((project) => project.id === task.projectId) ?? null
  );
  let assignableProjects = $derived(
    projects.filter((project) => project.archivedAtUnixMs === null || project.id === task.projectId)
  );

  $effect(() => {
    if (!editing) resetDraft();
  });

  function resetDraft(): void {
    title = task.title;
    notes = task.notes;
    plannedDate = task.plannedDate ?? '';
    dueAt = epochMsToLocalDateTime(task.dueAtUnixMs);
    reminderAt = epochMsToLocalDateTime(task.reminderAtUnixMs);
    priority = task.priority;
    recurrenceKind = task.recurrenceKind;
    recurrenceTimezone = task.recurrenceTimezone;
    projectId = task.projectId;
  }

  function clearFieldError(field: TaskField): void {
    if (!fieldErrors[field]) return;
    const next = { ...fieldErrors };
    delete next[field];
    fieldErrors = next;
  }

  function handleRecurrenceChange(): void {
    clearFieldError('recurrenceKind');
    clearFieldError('recurrenceTimezone');
    if (recurrenceKind !== 'none' && !recurrenceTimezone) recurrenceTimezone = localTimeZone();
  }

  async function cancelDelete(): Promise<void> {
    confirmingDelete = false;
    await tick();
    dialogTrigger?.focus();
  }

  async function cancelPermanentDelete(): Promise<void> {
    confirmingPermanentDelete = false;
    await tick();
    dialogTrigger?.focus();
  }

  async function openEditor(): Promise<void> {
    fieldErrors = {};
    requestError = null;
    confirmingDelete = false;
    editing = true;
    await tick();
    titleInput?.focus();
  }

  function closeEditor(): void {
    editing = false;
    fieldErrors = {};
    requestError = null;
    resetDraft();
    actionButton?.focus();
  }

  async function save(): Promise<void> {
    const dueAtUnixMs = localDateTimeToEpochMs(dueAt);
    const reminderAtUnixMs = localDateTimeToEpochMs(reminderAt);
    const input: TaskInput = {
      title,
      notes,
      plannedDate: plannedDate || null,
      dueAtUnixMs,
      reminderAtUnixMs,
      projectId,
      priority,
      recurrenceKind,
      recurrenceTimezone: recurrenceKind === 'none' ? null : recurrenceTimezone ?? localTimeZone(),
    };
    const errors = validateTaskInput(input);
    if (dueAt && dueAtUnixMs === null) errors.dueAtUnixMs = '请输入有效的本地截止时间。';
    if (reminderAt && reminderAtUnixMs === null) errors.reminderAtUnixMs = '请输入有效的本地提醒时间。';
    fieldErrors = errors;
    requestError = null;
    if (Object.keys(errors).length) return;

    busy = true;
    try {
      const changed = await onUpdate(task.id, input);
      if (await onChanged(changed.task, 'update', changed)) editing = false;
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function toggleCompleted(): Promise<void> {
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

  function openDeleteConfirm(event: MouseEvent): void {
    fieldErrors = {};
    requestError = null;
    editing = false;
    dialogTrigger = event.currentTarget as HTMLButtonElement;
    confirmingDelete = true;
  }

  async function remove(): Promise<void> {
    busy = true;
    requestError = null;
    try {
      const result = await onDelete(task.id);
      await onRemoved(task.id, result);
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

  function handleEditorKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeEditor();
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey || event.target instanceof HTMLInputElement)) {
      event.preventDefault();
      void save();
    }
  }

</script>

<article
  id={`task-${task.id}`}
  class:complete={task.completedAtUnixMs !== null}
  class:activated={highlighted}
  class:trashed={trashMode}
  class="task"
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
        {#if !compactMode}<button type="button" class="quiet" aria-label={`编辑：${task.title}`} onclick={openEditor} disabled={busy}>编辑</button>{/if}
        <button type="button" class="quiet danger" aria-label={`删除：${task.title}`} onclick={openDeleteConfirm} disabled={busy}>删除</button>
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

  {#if task.notes && !editing}<p class="notes">{task.notes}</p>{/if}

  {#if editing && !compactMode}
    <form class="editor" onsubmit={(event) => { event.preventDefault(); void save(); }}>
      <label for={`title-${task.id}`}>标题</label>
      <input
        bind:this={titleInput}
        id={`title-${task.id}`}
        bind:value={title}
        onkeydown={handleEditorKeydown}
        oninput={() => clearFieldError('title')}
        aria-invalid={Boolean(fieldErrors.title)}
        aria-describedby={fieldErrors.title ? `title-error-${task.id}` : undefined}
        disabled={busy}
      />
      {#if fieldErrors.title}<p class="error field-error" id={`title-error-${task.id}`}>{fieldErrors.title}</p>{/if}
      <label for={`notes-${task.id}`}>备注</label>
      <textarea
        id={`notes-${task.id}`}
        bind:value={notes}
        onkeydown={handleEditorKeydown}
        oninput={() => clearFieldError('notes')}
        aria-invalid={Boolean(fieldErrors.notes)}
        aria-describedby={fieldErrors.notes ? `notes-error-${task.id}` : undefined}
        disabled={busy}
      ></textarea>
      {#if fieldErrors.notes}<p class="error field-error" id={`notes-error-${task.id}`}>{fieldErrors.notes}</p>{/if}
      <label for={`planned-${task.id}`}>计划日期</label>
      <input
        id={`planned-${task.id}`}
        type="date"
        bind:value={plannedDate}
        onkeydown={handleEditorKeydown}
        oninput={() => clearFieldError('plannedDate')}
        aria-invalid={Boolean(fieldErrors.plannedDate)}
        aria-describedby={fieldErrors.plannedDate ? `planned-error-${task.id}` : undefined}
        disabled={busy}
      />
      {#if fieldErrors.plannedDate}<p class="error field-error" id={`planned-error-${task.id}`}>{fieldErrors.plannedDate}</p>{/if}
      <label for={`due-${task.id}`}>截止时间</label>
      <input
        id={`due-${task.id}`}
        type="datetime-local"
        bind:value={dueAt}
        onkeydown={handleEditorKeydown}
        oninput={() => clearFieldError('dueAtUnixMs')}
        aria-invalid={Boolean(fieldErrors.dueAtUnixMs)}
        aria-describedby={fieldErrors.dueAtUnixMs ? `due-error-${task.id}` : undefined}
        disabled={busy}
      />
      {#if fieldErrors.dueAtUnixMs}<p class="error field-error" id={`due-error-${task.id}`}>{fieldErrors.dueAtUnixMs}</p>{/if}
      <label for={`reminder-${task.id}`}>提醒时间</label>
      <input
        id={`reminder-${task.id}`}
        type="datetime-local"
        bind:value={reminderAt}
        onkeydown={handleEditorKeydown}
        oninput={() => clearFieldError('reminderAtUnixMs')}
        aria-invalid={Boolean(fieldErrors.reminderAtUnixMs)}
        aria-describedby={fieldErrors.reminderAtUnixMs ? `reminder-error-${task.id}` : undefined}
        disabled={busy}
      />
      {#if fieldErrors.reminderAtUnixMs}<p class="error field-error" id={`reminder-error-${task.id}`}>{fieldErrors.reminderAtUnixMs}</p>{/if}
      <label for={`project-${task.id}`}>项目</label>
      <select
        id={`project-${task.id}`}
        bind:value={projectId}
        onkeydown={handleEditorKeydown}
        disabled={busy}
      >
        <option value={null}>收件箱</option>
        {#each assignableProjects as project (project.id)}
          <option value={project.id}>{project.name}{project.archivedAtUnixMs !== null ? '（已归档）' : ''}</option>
        {/each}
      </select>
      {#if taskProject?.archivedAtUnixMs !== null}<p class="field-note">已归档项目仅保留既有归属；如需改属，请选择收件箱或活动项目。</p>{/if}
      <label for={`priority-${task.id}`}>优先级</label>
      <select
        id={`priority-${task.id}`}
        bind:value={priority}
        onkeydown={handleEditorKeydown}
        onchange={() => clearFieldError('priority')}
        aria-invalid={Boolean(fieldErrors.priority)}
        aria-describedby={fieldErrors.priority ? `priority-error-${task.id}` : undefined}
        disabled={busy}
      >{#each PRIORITY_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>
      {#if fieldErrors.priority}<p class="error field-error" id={`priority-error-${task.id}`}>{fieldErrors.priority}</p>{/if}
      <label for={`recurrence-${task.id}`}>重复</label>
      <select
        id={`recurrence-${task.id}`}
        bind:value={recurrenceKind}
        onkeydown={handleEditorKeydown}
        onchange={handleRecurrenceChange}
        aria-invalid={Boolean(fieldErrors.recurrenceKind || fieldErrors.recurrenceTimezone)}
        aria-describedby={fieldErrors.recurrenceKind ? `recurrence-error-${task.id}` : fieldErrors.recurrenceTimezone ? `recurrence-timezone-error-${task.id}` : undefined}
        disabled={busy}
      >{#each RECURRENCE_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>
      {#if fieldErrors.recurrenceKind}<p class="error field-error" id={`recurrence-error-${task.id}`}>{fieldErrors.recurrenceKind}</p>{/if}
      {#if fieldErrors.recurrenceTimezone}<p class="error field-error" id={`recurrence-timezone-error-${task.id}`}>{fieldErrors.recurrenceTimezone}</p>{/if}
      {#if recurrenceKind !== 'none'}<p class="field-note">将按 {recurrenceTimezone ?? localTimeZone()} 的本地日历生成后续实例。</p>{/if}
      <div class="editor-actions"><button type="submit" disabled={busy}>{busy ? '保存中…' : '保存'}</button><button type="button" class="quiet" onclick={closeEditor} disabled={busy}>取消</button></div>
    </form>
  {/if}

  <ConfirmDialog
    open={confirmingDelete && !trashMode}
    dialogId={`delete-task-${task.id}`}
    title="移入回收站"
    description={`将“${task.title}”移入回收站？之后仍可恢复。`}
    confirmLabel="确认移入"
    {busy}
    error={confirmingDelete ? requestError : null}
    onConfirm={remove}
    onCancel={cancelDelete}
  />
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
  {#if requestError && !confirmingDelete && !confirmingPermanentDelete}<p class="error" role="alert">{requestError}</p>{/if}
</article>

<style>
  .task { padding:14px 0; border-bottom:1px solid var(--line); transition:background .15s ease; }
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
  .actions,.editor-actions,.quick-actions { display:flex; gap:6px; flex-wrap:wrap; }
  .quick-actions { margin:9px 0 0 36px; }
  .focus-action { border:1px solid color-mix(in srgb, var(--accent) 72%, var(--line)); border-radius:var(--radius-sm); padding:4px 8px; color:var(--accent-hover); background:var(--accent-soft); font-size:12px; font-weight:650; }
  .focus-action:hover:not(:disabled) { color:var(--accent-ink); background:var(--accent); }
  .quiet { border:0; border-radius:var(--radius-sm); padding:4px 6px; color:var(--muted); background:transparent; font-size:12px; }
  .quiet:hover:not(:disabled) { color:var(--text-soft); background:rgba(255,255,255,.05); }
  .danger { color:var(--danger); }
  .notes { margin:8px 0 0 36px; color:var(--text-soft); white-space:pre-wrap; font-size:12px; line-height:1.55; overflow-wrap:anywhere; }
  .editor { display:grid; gap:7px; margin:13px 0 0 36px; padding:12px; border:1px solid var(--line); border-radius:var(--radius-md); background:var(--surface); }
  .editor label,.field-note { color:var(--muted); font-size:11px; }
  .field-note { margin:0; line-height:1.5; }
  .editor input,.editor textarea,.editor select { width:100%; border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:7px 8px; color:var(--text); background:#14161a; }
  .editor textarea { min-height:76px; resize:vertical; }
  .editor-actions { margin-top:4px; }
  .editor-actions button:not(.quiet) { border:1px solid var(--accent); border-radius:var(--radius-sm); padding:6px 9px; color:var(--accent-ink); background:var(--accent); font-size:12px; font-weight:650; }
  .error { margin:8px 0 0 36px; color:var(--danger); font-size:12px; }
  @media (max-width:420px) { .task-row { display:grid; grid-template-columns:26px minmax(0,1fr); } .actions { grid-column:2; } .quick-actions,.notes,.editor,.error { margin-left:36px; } }
</style>
