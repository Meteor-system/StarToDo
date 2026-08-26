<script lang="ts">
  import { tick } from 'svelte';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import ContextDrawer from '$lib/components/ContextDrawer.svelte';
  import {
    epochMsToLocalDateTime,
    localDateTimeToEpochMs,
    localTimeZone,
    PRIORITY_OPTIONS,
    RECURRENCE_OPTIONS,
    type MutationWarningResult,
    type Priority,
    type Project,
    type RecurrenceKind,
    type Task,
    type TaskChangeKind,
    type TaskField,
    type TaskFieldErrors,
    type TaskInput,
    type TaskMutationWithToken,
    validateTaskInput
  } from '$lib/tasks';

  interface Props {
    open: boolean;
    task: Task | null;
    projects: Project[];
    onClose: () => void;
    onUpdate: (id: number, input: TaskInput) => Promise<TaskMutationWithToken>;
    onDelete: (id: number) => Promise<MutationWarningResult>;
    onChanged: (
      task: Task,
      kind: TaskChangeKind,
      result: TaskMutationWithToken
    ) => Promise<boolean>;
    onRemoved: (
      id: number,
      result: MutationWarningResult
    ) => Promise<boolean>;
  }

  let { open, task, projects, onClose, onUpdate, onDelete, onChanged, onRemoved }: Props = $props();
  let busy = $state(false);
  let confirmingDelete = $state(false);
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
  let deleteButton = $state<HTMLButtonElement>();
  let draftTaskId: number | null = null;

  let assignableProjects = $derived(
    projects.filter((project) => project.archivedAtUnixMs === null || project.id === task?.projectId)
  );
  let taskProject = $derived(
    task?.projectId === null || task?.projectId === undefined
      ? null
      : projects.find((project) => project.id === task.projectId) ?? null
  );

  $effect(() => {
    if (!open || task === null) {
      draftTaskId = null;
      confirmingDelete = false;
      return;
    }
    if (draftTaskId === task.id) return;
    resetDraft(task);
    draftTaskId = task.id;
    void tick().then(() => titleInput?.focus());
  });

  function resetDraft(currentTask: Task): void {
    title = currentTask.title;
    notes = currentTask.notes;
    plannedDate = currentTask.plannedDate ?? '';
    dueAt = epochMsToLocalDateTime(currentTask.dueAtUnixMs);
    reminderAt = epochMsToLocalDateTime(currentTask.reminderAtUnixMs);
    priority = currentTask.priority;
    recurrenceKind = currentTask.recurrenceKind;
    recurrenceTimezone = currentTask.recurrenceTimezone;
    projectId = currentTask.projectId;
    fieldErrors = {};
    requestError = null;
    confirmingDelete = false;
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

  function handleEditorKeydown(event: KeyboardEvent): void {
    if (!confirmingDelete && event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void save();
    }
  }

  async function cancelDelete(): Promise<void> {
    confirmingDelete = false;
    await tick();
    deleteButton?.focus();
  }

  async function save(): Promise<void> {
    const currentTask = task;
    if (!currentTask || busy) return;
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
      recurrenceTimezone: recurrenceKind === 'none' ? null : recurrenceTimezone ?? localTimeZone()
    };
    const errors = validateTaskInput(input);
    if (dueAt && dueAtUnixMs === null) errors.dueAtUnixMs = '请输入有效的本地截止时间。';
    if (reminderAt && reminderAtUnixMs === null) errors.reminderAtUnixMs = '请输入有效的本地提醒时间。';
    fieldErrors = errors;
    requestError = null;
    if (Object.keys(errors).length) return;

    busy = true;
    try {
      const changed = await onUpdate(currentTask.id, input);
      if (await onChanged(changed.task, 'update', changed)) onClose();
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<void> {
    const currentTask = task;
    if (!currentTask || busy) return;
    busy = true;
    requestError = null;
    try {
      const result = await onDelete(currentTask.id);
      if (await onRemoved(currentTask.id, result)) onClose();
    } catch (cause) {
      requestError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<ContextDrawer {open} drawerId="task-details-drawer" title="任务详情" {onClose}>
  {#if task}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <form class="editor" onsubmit={(event) => { event.preventDefault(); void save(); }} onkeydown={handleEditorKeydown}>
      <label for={`details-title-${task.id}`}>标题</label>
      <input bind:this={titleInput} id={`details-title-${task.id}`} bind:value={title} oninput={() => clearFieldError('title')} aria-invalid={Boolean(fieldErrors.title)} aria-describedby={fieldErrors.title ? `details-title-error-${task.id}` : undefined} disabled={busy} />
      {#if fieldErrors.title}<p class="error field-error" id={`details-title-error-${task.id}`}>{fieldErrors.title}</p>{/if}

      <label for={`details-notes-${task.id}`}>备注</label>
      <textarea id={`details-notes-${task.id}`} bind:value={notes} oninput={() => clearFieldError('notes')} aria-invalid={Boolean(fieldErrors.notes)} aria-describedby={fieldErrors.notes ? `details-notes-error-${task.id}` : undefined} disabled={busy}></textarea>
      {#if fieldErrors.notes}<p class="error field-error" id={`details-notes-error-${task.id}`}>{fieldErrors.notes}</p>{/if}

      <label for={`details-planned-${task.id}`}>计划日期</label>
      <input id={`details-planned-${task.id}`} type="date" bind:value={plannedDate} oninput={() => clearFieldError('plannedDate')} aria-invalid={Boolean(fieldErrors.plannedDate)} aria-describedby={fieldErrors.plannedDate ? `details-planned-error-${task.id}` : undefined} disabled={busy} />
      {#if fieldErrors.plannedDate}<p class="error field-error" id={`details-planned-error-${task.id}`}>{fieldErrors.plannedDate}</p>{/if}

      <label for={`details-due-${task.id}`}>截止时间</label>
      <input id={`details-due-${task.id}`} type="datetime-local" bind:value={dueAt} oninput={() => clearFieldError('dueAtUnixMs')} aria-invalid={Boolean(fieldErrors.dueAtUnixMs)} aria-describedby={fieldErrors.dueAtUnixMs ? `details-due-error-${task.id}` : undefined} disabled={busy} />
      {#if fieldErrors.dueAtUnixMs}<p class="error field-error" id={`details-due-error-${task.id}`}>{fieldErrors.dueAtUnixMs}</p>{/if}

      <label for={`details-reminder-${task.id}`}>提醒时间</label>
      <input id={`details-reminder-${task.id}`} type="datetime-local" bind:value={reminderAt} oninput={() => clearFieldError('reminderAtUnixMs')} aria-invalid={Boolean(fieldErrors.reminderAtUnixMs)} aria-describedby={fieldErrors.reminderAtUnixMs ? `details-reminder-error-${task.id}` : undefined} disabled={busy} />
      {#if fieldErrors.reminderAtUnixMs}<p class="error field-error" id={`details-reminder-error-${task.id}`}>{fieldErrors.reminderAtUnixMs}</p>{/if}

      <label for={`details-project-${task.id}`}>项目</label>
      <select id={`details-project-${task.id}`} bind:value={projectId} disabled={busy}>
        <option value={null}>收件箱</option>
        {#each assignableProjects as project (project.id)}
          <option value={project.id}>{project.name}{project.archivedAtUnixMs !== null ? '（已归档）' : ''}</option>
        {/each}
      </select>
      {#if taskProject !== null && taskProject.archivedAtUnixMs !== null}<p class="field-note">已归档项目仅保留既有归属；如需改属，请选择收件箱或活动项目。</p>{/if}

      <label for={`details-priority-${task.id}`}>优先级</label>
      <select id={`details-priority-${task.id}`} bind:value={priority} onchange={() => clearFieldError('priority')} aria-invalid={Boolean(fieldErrors.priority)} aria-describedby={fieldErrors.priority ? `details-priority-error-${task.id}` : undefined} disabled={busy}>
        {#each PRIORITY_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}
      </select>
      {#if fieldErrors.priority}<p class="error field-error" id={`details-priority-error-${task.id}`}>{fieldErrors.priority}</p>{/if}

      <label for={`details-recurrence-${task.id}`}>重复</label>
      <select id={`details-recurrence-${task.id}`} bind:value={recurrenceKind} onchange={handleRecurrenceChange} aria-invalid={Boolean(fieldErrors.recurrenceKind || fieldErrors.recurrenceTimezone)} aria-describedby={fieldErrors.recurrenceKind ? `details-recurrence-error-${task.id}` : fieldErrors.recurrenceTimezone ? `details-recurrence-timezone-error-${task.id}` : undefined} disabled={busy}>
        {#each RECURRENCE_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}
      </select>
      {#if fieldErrors.recurrenceKind}<p class="error field-error" id={`details-recurrence-error-${task.id}`}>{fieldErrors.recurrenceKind}</p>{/if}
      {#if fieldErrors.recurrenceTimezone}<p class="error field-error" id={`details-recurrence-timezone-error-${task.id}`}>{fieldErrors.recurrenceTimezone}</p>{/if}
      {#if recurrenceKind !== 'none'}<p class="field-note">将按 {recurrenceTimezone ?? localTimeZone()} 的本地日历生成后续实例。</p>{/if}

      {#if requestError && !confirmingDelete}<p class="error" role="alert">{requestError}</p>{/if}
      <div class="editor-actions">
        <button type="submit" disabled={busy}>{busy ? '保存中…' : '保存'}</button>
        <button bind:this={deleteButton} type="button" class="quiet danger" onclick={() => { requestError = null; confirmingDelete = true; }} disabled={busy}>删除</button>
      </div>
    </form>

    <ConfirmDialog
      open={confirmingDelete}
      dialogId={`delete-task-${task.id}`}
      title="移入回收站"
      description={`将“${task.title}”移入回收站？之后仍可恢复。`}
      confirmLabel="确认移入"
      {busy}
      error={confirmingDelete ? requestError : null}
      onConfirm={remove}
      onCancel={cancelDelete}
    />
  {/if}
</ContextDrawer>

<style>
  .editor { display:grid; gap:8px; padding-top:16px; }
  .editor label,.field-note { color:var(--muted); font-size:11px; }
  .field-note,.error { margin:0; line-height:1.5; }
  .editor input,.editor textarea,.editor select { width:100%; min-width:0; border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:8px; color:var(--text); background:var(--surface); }
  .editor textarea { min-height:100px; resize:vertical; }
  .editor-actions { display:flex; justify-content:space-between; gap:8px; margin-top:8px; }
  .editor-actions button { border:1px solid var(--accent); border-radius:var(--radius-sm); padding:7px 10px; color:var(--accent-ink); background:var(--accent); font-size:12px; font-weight:650; }
  .editor-actions .quiet { border-color:transparent; color:var(--muted); background:transparent; }
  .editor-actions .danger { color:var(--danger); }
  .error { color:var(--danger); font-size:12px; }
</style>
