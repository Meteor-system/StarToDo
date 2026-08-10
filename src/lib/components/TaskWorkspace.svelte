<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import ProjectSidebar from '$lib/components/ProjectSidebar.svelte';
  import TaskComposer from '$lib/components/TaskComposer.svelte';
  import TaskItem from '$lib/components/TaskItem.svelte';
  import WeekPlanner from '$lib/components/WeekPlanner.svelte';
  import { readPreference, writePreference } from '$lib/preferences';
  import {
    addLocalCalendarDays,
    archiveProject as archiveProjectRecord,
    createProject as createProjectRecord,
    createTask,
    deferTaskToTomorrow,
    deleteTask,
    errorMessage,
    listDeletedTasks,
    listProjects,
    listTasks,
    localDateForEpochMs,
    localDateTimeToEpochMs,
    localTimeZone,
    permanentlyDeleteTask,
    PRIORITY_OPTIONS,
    RECURRENCE_OPTIONS,
    restoreProject as restoreProjectRecord,
    restoreTask,
    setTaskCompleted,
    setTaskPlannedDate,
    snoozeTask,
    sortTasks,
    todayLocalDate,
    updateProject as updateProjectRecord,
    updateTask,
    validateTaskInput,
    weekStartLocalDate,
    type BatchCreateResult,
    type DateFilter,
    type ExecutionView,
    type MutationWarningResult,
    type Priority,
    type Project,
    type ProjectSelection,
    type RecurrenceKind,
    type ReminderReport,
    type ReminderWarningContext,
    type StatusFilter,
    type Task,
    type TaskChangeKind,
    type TaskDraft,
    type TaskField,
    type TaskFieldErrors,
    type TaskInput,
    type TaskCompletionMutationWithToken,
    type TaskMutationWithToken,
    type TaskOperation,
    type TaskOperationToken
  } from '$lib/tasks';

  type WorkspaceView = 'list' | 'week';

  interface Props {
    tauriAvailable: boolean;
    initialized: boolean;
    compact: boolean;
    activationId: number | null;
    activationNonce: number;
    onReminderReconcile: () => Promise<ReminderReport>;
    onMutationWarning: (context: ReminderWarningContext) => void;
    onActivationResolved: (nonce: number) => void;
  }

  const STATUS_FILTER_STORAGE_KEY = 'startodo.status-filter';
  const EXECUTION_VIEW_STORAGE_KEY = 'startodo.execution-view';
  const PROJECT_SELECTION_STORAGE_KEY = 'startodo.project-selection';
  const WORKSPACE_VIEW_STORAGE_KEY = 'startodo.workspace-view';
  const LEGACY_DATE_FILTER_STORAGE_KEY = 'startodo.date-filter';

  function storedFilter<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
    const value = readPreference(key);
    return value && allowed.includes(value as T) ? value as T : fallback;
  }

  function storedExecutionView(): ExecutionView {
    const current = readPreference(EXECUTION_VIEW_STORAGE_KEY);
    if (current && ['inbox', 'today', 'overdue', 'upcoming', 'all'].includes(current)) {
      return current as ExecutionView;
    }
    const legacy = readPreference(LEGACY_DATE_FILTER_STORAGE_KEY) as DateFilter | null;
    return legacy === 'week' ? 'upcoming' : legacy === 'all' ? 'all' : 'today';
  }

  function storedProjectSelection(): ProjectSelection {
    const value = readPreference(PROJECT_SELECTION_STORAGE_KEY);
    if (value === 'all' || value === 'inbox') return value;
    const projectId = Number(value);
    return Number.isSafeInteger(projectId) && projectId > 0 ? projectId : 'all';
  }

  let {
    tauriAvailable,
    initialized,
    compact,
    activationId,
    activationNonce,
    onReminderReconcile,
    onMutationWarning,
    onActivationResolved
  }: Props = $props();
  let tasks = $state<Task[]>([]);
  let deletedTasks = $state<Task[]>([]);
  let projects = $state<Project[]>([]);
  let selectedProject = $state<ProjectSelection>(storedProjectSelection());
  let loading = $state(false);
  let trashLoading = $state(false);
  let loadedForRuntime = $state(false);
  let loadError = $state<string | null>(null);
  let trashLoadError = $state<string | null>(null);
  let createBusy = $state(false);
  let createErrors = $state<TaskFieldErrors>({});
  let createRequestError = $state<string | null>(null);
  let title = $state('');
  let notes = $state('');
  let plannedDate = $state('');
  let dueAt = $state('');
  let reminderAt = $state('');
  let priority = $state<Priority>('none');
  let recurrenceKind = $state<RecurrenceKind>('none');
  let recurrenceTimezone = $state<string | null>(null);
  let statusFilter = $state<StatusFilter>(storedFilter(STATUS_FILTER_STORAGE_KEY, ['all', 'active', 'completed'], 'all'));
  let executionView = $state<ExecutionView>(storedExecutionView());
  let workspaceView = $state<WorkspaceView>(storedFilter(WORKSPACE_VIEW_STORAGE_KEY, ['list', 'week'], 'list'));
  let plannerWeekStart = $state(weekStartLocalDate(todayLocalDate()));
  let nowUnixMs = $state(Date.now());
  let searchQuery = $state('');
  let showTrash = $state(false);
  let announcement = $state('');
  let dataRevision = 0;
  let taskOperationSequence = 0;
  let latestTaskOperationSequence = new Map<string, number>();
  let initialReminderSyncStarted = false;
  let lastFocusedActivationNonce = -1;
  let activationInProgressNonce = -1;
  let activationRetryNonce = $state<number | null>(null);
  let activationRetryTimer: ReturnType<typeof window.setTimeout> | undefined;
  let taskListReady = false;
  let loadGeneration = 0;
  let refreshRequested = false;
  let trashLoadGeneration = 0;
  let titleInput = $state<HTMLInputElement>();
  let searchInput = $state<HTMLInputElement>();

  let currentLocalDate = $derived(localDateForEpochMs(nowUnixMs) ?? todayLocalDate());
  let activeWorkspaceView = $derived(compact || showTrash ? 'list' : workspaceView);
  let currentTasks = $derived(showTrash ? deletedTasks : tasks);
  let visibleTasks = $derived(currentTasks.filter((task) =>
    (showTrash || isVisible(task)) && searchMatches(task, true)
  ));
  let activeTasks = $derived(visibleTasks.filter((task) => task.completedAtUnixMs === null));
  let completedTasks = $derived(visibleTasks.filter((task) => task.completedAtUnixMs !== null));
  let plannerTasks = $derived(tasks.filter((task) =>
    task.deletedAtUnixMs === null &&
    task.completedAtUnixMs === null &&
    projectMatches(task) &&
    searchMatches(task)
  ));
  let selectedProjectRecord = $derived(
    typeof selectedProject === 'number'
      ? projects.find((project) => project.id === selectedProject) ?? null
      : null
  );
  let selectedProjectIsArchived = $derived(
    selectedProjectRecord !== null && selectedProjectRecord.archivedAtUnixMs !== null
  );

  function markMutation(): void {
    dataRevision += 1;
    if (loading) refreshRequested = true;
  }

  function upsert(task: Task): void {
    tasks = sortTasks([...tasks.filter((candidate) => candidate.id !== task.id), task]);
  }

  function sortProjects(items: Project[]): Project[] {
    return [...items].sort((left, right) => {
      const leftArchived = left.archivedAtUnixMs !== null;
      const rightArchived = right.archivedAtUnixMs !== null;
      if (leftArchived !== rightArchived) return leftArchived ? 1 : -1;
      if (left.updatedAtUnixMs !== right.updatedAtUnixMs) {
        return right.updatedAtUnixMs - left.updatedAtUnixMs;
      }
      return right.id - left.id;
    });
  }

  function upsertProject(project: Project): void {
    projects = sortProjects([...projects.filter((candidate) => candidate.id !== project.id), project]);
  }

  function normalizeSelectedProject(): void {
    if (typeof selectedProject !== 'number') return;
    if (!projects.some((project) => project.id === selectedProject)) selectedProject = 'all';
  }

  function defaultProjectId(): number | null {
    if (typeof selectedProject !== 'number') return null;
    const project = projects.find((candidate) => candidate.id === selectedProject);
    return project?.archivedAtUnixMs === null ? project.id : null;
  }

  function projectMatches(task: Task): boolean {
    if (selectedProject === 'inbox') return task.projectId === null;
    if (typeof selectedProject === 'number') return task.projectId === selectedProject;
    if (task.projectId === null) return true;
    return projects.some((project) => project.id === task.projectId && project.archivedAtUnixMs === null);
  }

  function searchMatches(task: Task, includeActivation = false): boolean {
    if (includeActivation && activationId !== null && task.id === activationId) return true;
    const query = searchQuery.trim().toLocaleLowerCase();
    if (!query) return true;
    return `${task.title}\n${task.notes}`.toLocaleLowerCase().includes(query);
  }

  function selectProject(selection: ProjectSelection): void {
    selectedProject = selection;
    if (selection !== 'all' && selection !== 'inbox') showTrash = false;
  }

  async function createProject(name: string): Promise<void> {
    markMutation();
    const project = await createProjectRecord({ name });
    upsertProject(project);
    selectedProject = project.id;
    announcement = `已创建项目：${project.name}`;
  }

  async function renameProject(id: number, name: string): Promise<void> {
    markMutation();
    const project = await updateProjectRecord(id, { name });
    upsertProject(project);
    announcement = `已重命名项目：${project.name}`;
  }

  async function archiveProject(id: number): Promise<void> {
    markMutation();
    const project = await archiveProjectRecord(id);
    upsertProject(project);
    if (selectedProject === id) selectedProject = 'all';
    announcement = `已归档项目：${project.name}`;
  }

  async function restoreProject(id: number): Promise<void> {
    markMutation();
    const project = await restoreProjectRecord(id);
    upsertProject(project);
    announcement = `已恢复项目：${project.name}`;
  }

  function clearCreateError(field: TaskField): void {
    if (!createErrors[field]) return;
    const next = { ...createErrors };
    delete next[field];
    createErrors = next;
  }

  function handleCreateRecurrenceChange(): void {
    clearCreateError('recurrenceKind');
    clearCreateError('recurrenceTimezone');
    if (recurrenceKind !== 'none' && !recurrenceTimezone) recurrenceTimezone = localTimeZone();
  }

  function executionMatches(task: Task): boolean {
    const view = compact ? 'today' : executionView;
    if (view === 'all') return true;

    const active = task.completedAtUnixMs === null;
    const dueDate = task.dueAtUnixMs === null ? null : localDateForEpochMs(task.dueAtUnixMs);
    const today = localDateForEpochMs(nowUnixMs) ?? todayLocalDate();
    const overdue = active && task.dueAtUnixMs !== null && task.dueAtUnixMs < nowUnixMs;

    if (view === 'inbox') return active && task.projectId === null;
    if (view === 'overdue') return overdue;
    if (view === 'today') return !overdue && (task.plannedDate === today || dueDate === today);

    const start = addLocalCalendarDays(today, 1);
    const end = addLocalCalendarDays(today, 7);
    if (!start || !end || overdue) return false;
    return (task.plannedDate !== null && task.plannedDate >= start && task.plannedDate <= end) ||
      (dueDate !== null && dueDate >= start && dueDate <= end);
  }

  function isVisible(task: Task): boolean {
    if (activationId !== null && task.id === activationId) return true;
    const filter = compact ? 'active' : statusFilter;
    const statusMatches = filter === 'all' ||
      (filter === 'active' && task.completedAtUnixMs === null) ||
      (filter === 'completed' && task.completedAtUnixMs !== null);
    return projectMatches(task) && executionMatches(task) && statusMatches;
  }

  function buildInput(): TaskInput {
    const timezone = recurrenceKind === 'none' ? null : recurrenceTimezone ?? localTimeZone();
    return {
      title,
      notes,
      plannedDate: plannedDate || null,
      dueAtUnixMs: localDateTimeToEpochMs(dueAt),
      reminderAtUnixMs: localDateTimeToEpochMs(reminderAt),
      projectId: defaultProjectId(),
      priority,
      recurrenceKind,
      recurrenceTimezone: timezone,
    };
  }

  function operationKey(taskId: number | null): string {
    return taskId === null ? 'new-task' : `task-${taskId}`;
  }

  function beginOperation(operation: TaskOperation, taskId: number | null): TaskOperationToken {
    const token = { sequence: ++taskOperationSequence, operation, taskId };
    latestTaskOperationSequence.set(operationKey(taskId), token.sequence);
    markMutation();
    return token;
  }

  function isCurrentOperation(token: TaskOperationToken): boolean {
    return latestTaskOperationSequence.get(operationKey(token.taskId)) === token.sequence;
  }

  function clearActivationRetry(): void {
    if (typeof window !== 'undefined' && activationRetryTimer !== undefined) {
      window.clearTimeout(activationRetryTimer);
      activationRetryTimer = undefined;
    }
    activationRetryNonce = null;
  }

  function scheduleActivationRetry(nonce: number, delayMs = 3_000): void {
    activationRetryNonce = nonce;
    activationInProgressNonce = -1;
    if (typeof window === 'undefined') return;
    if (activationRetryTimer !== undefined) window.clearTimeout(activationRetryTimer);
    activationRetryTimer = window.setTimeout(() => {
      activationRetryTimer = undefined;
      if (activationNonce !== nonce) return;
      activationRetryNonce = null;
      if (showTrash || !taskListReady || loadError !== null) void load();
    }, delayMs);
  }

  function reportMutationWarning(token: TaskOperationToken, reminderWarning: string | null, taskId = token.taskId): void {
    if (!isCurrentOperation(token)) return;
    onMutationWarning({ source: 'mutation', ...token, taskId, message: reminderWarning });
  }

  async function loadTrash(): Promise<Task[] | null> {
    if (!tauriAvailable) return deletedTasks;
    const generation = ++trashLoadGeneration;
    const revision = dataRevision;
    trashLoading = true;
    trashLoadError = null;
    try {
      const loadedTasks = await listDeletedTasks();
      const current = generation === trashLoadGeneration && revision === dataRevision;
      if (current) deletedTasks = loadedTasks;
      return current ? loadedTasks : null;
    } catch (cause) {
      if (generation === trashLoadGeneration && revision === dataRevision) {
        trashLoadError = errorMessage(cause);
      }
      throw cause;
    } finally {
      if (generation === trashLoadGeneration) trashLoading = false;
    }
  }

  async function load(): Promise<void> {
    if (!tauriAvailable) return;
    if (showTrash) {
      const loadedTrash = await loadTrash().catch(() => null);
      if (loadedTrash !== null && activationRetryNonce === activationNonce) {
        clearActivationRetry();
        activationInProgressNonce = -1;
      }
      return;
    }
    if (loading) {
      refreshRequested = true;
      return;
    }
    const generation = ++loadGeneration;
    const revision = dataRevision;
    loading = true;
    taskListReady = false;
    loadError = null;
    try {
      const [loadedTasks, loadedProjects] = await Promise.all([listTasks(), listProjects(true)]);
      if (generation === loadGeneration && revision === dataRevision) {
        tasks = sortTasks(loadedTasks);
        projects = sortProjects(loadedProjects);
        normalizeSelectedProject();
        taskListReady = true;
        if (activationRetryNonce === activationNonce) {
          clearActivationRetry();
          activationInProgressNonce = -1;
        }
        if (!initialReminderSyncStarted) {
          initialReminderSyncStarted = true;
          void onReminderReconcile().catch(() => undefined);
        }
      } else if (generation === loadGeneration) {
        refreshRequested = true;
      }
    } catch (cause) {
      if (generation === loadGeneration && revision === dataRevision) {
        loadError = errorMessage(cause);
        if (activationId !== null && activationNonce !== lastFocusedActivationNonce) {
          scheduleActivationRetry(activationNonce);
        } else {
          activationInProgressNonce = -1;
        }
      }
    } finally {
      if (generation === loadGeneration) {
        loading = false;
        if (refreshRequested && !showTrash) {
          refreshRequested = false;
          void load();
        }
      }
    }
  }

  async function submit(): Promise<void> {
    if (selectedProjectIsArchived) {
      createRequestError = '已归档项目不能创建新任务。请先恢复项目或切换到收件箱。';
      return;
    }
    const input = buildInput();
    const errors = validateTaskInput(input);
    if (plannedDate && !input.plannedDate) errors.plannedDate = '请输入有效的计划日期。';
    if (dueAt && input.dueAtUnixMs === null) errors.dueAtUnixMs = '请输入有效的本地截止时间。';
    if (reminderAt && input.reminderAtUnixMs === null) errors.reminderAtUnixMs = '请输入有效的本地提醒时间。';
    createErrors = errors;
    createRequestError = null;
    if (Object.keys(errors).length) return;

    const token = beginOperation('create', null);
    createBusy = true;
    try {
      const result = await createTask(input);
      if (!isCurrentOperation(token)) return;
      upsert(result.task);
      reportMutationWarning(token, result.reminderWarning, result.task.id);
      title = '';
      notes = '';
      plannedDate = '';
      dueAt = '';
      reminderAt = '';
      priority = 'none';
      recurrenceKind = 'none';
      recurrenceTimezone = null;
      announcement = `已添加任务：${result.task.title}`;
      void onReminderReconcile().catch(() => undefined);
    } catch (cause) {
      if (isCurrentOperation(token)) createRequestError = errorMessage(cause);
    } finally {
      if (isCurrentOperation(token)) createBusy = false;
    }
  }

  async function handleComposerCreate(drafts: TaskDraft[]): Promise<BatchCreateResult> {
    if (selectedProjectIsArchived) {
      return {
        createdCount: 0,
        remainingDrafts: drafts,
        error: '已归档项目不能创建新任务。请先恢复项目或切换到收件箱。',
      };
    }
    const token = beginOperation('create', null);
    createBusy = true;
    let createdCount = 0;
    let firstError: string | null = null;
    const remainingDrafts: TaskDraft[] = [];
    try {
      for (let index = 0; index < drafts.length; index += 1) {
        const draft = drafts[index];
        try {
          const result = await createTask({
            title: draft.title,
            notes: draft.notes,
            plannedDate: draft.plannedDate,
            dueAtUnixMs: null,
            reminderAtUnixMs: null,
            projectId: defaultProjectId(),
            priority: draft.priority,
            recurrenceKind: 'none',
            recurrenceTimezone: null,
          });
          createdCount += 1;
          if (!isCurrentOperation(token)) {
            return {
              createdCount,
              remainingDrafts: [...remainingDrafts, ...drafts.slice(index + 1)],
              error: firstError ?? '创建操作已被更新的操作取代。',
            };
          }
          upsert(result.task);
          reportMutationWarning(token, result.reminderWarning, result.task.id);
        } catch (cause) {
          remainingDrafts.push(draft);
          firstError ??= errorMessage(cause);
          if (!isCurrentOperation(token)) {
            return {
              createdCount,
              remainingDrafts: [...remainingDrafts, ...drafts.slice(index + 1)],
              error: firstError,
            };
          }
        }
      }
      if (isCurrentOperation(token)) {
        announcement = createdCount ? `已添加 ${createdCount} 项任务。` : '';
        if (createdCount) void onReminderReconcile().catch(() => undefined);
      }
      return { createdCount, remainingDrafts, error: firstError };
    } finally {
      if (isCurrentOperation(token)) createBusy = false;
    }
  }

  async function handleUpdate(id: number, input: TaskInput): Promise<TaskMutationWithToken> {
    const operationToken = beginOperation('update', id);
    const result = await updateTask(id, input);
    return { ...result, operationToken };
  }

  async function handleCompleted(id: number, completed: boolean): Promise<TaskCompletionMutationWithToken> {
    const operationToken = beginOperation(completed ? 'complete' : 'restore', id);
    const result = await setTaskCompleted(id, completed);
    return { ...result, operationToken };
  }

  async function handleSnooze(id: number, untilUnixMs: number): Promise<TaskMutationWithToken> {
    const operationToken = beginOperation('snooze', id);
    const result = await snoozeTask(id, untilUnixMs);
    return { ...result, operationToken };
  }

  async function handleDefer(id: number): Promise<TaskMutationWithToken> {
    const operationToken = beginOperation('defer', id);
    const result = await deferTaskToTomorrow(id, localTimeZone());
    return { ...result, operationToken };
  }

  async function handleDelete(id: number): Promise<MutationWarningResult> {
    const operationToken = beginOperation('delete', id);
    const reminderWarning = await deleteTask(id);
    return { reminderWarning, operationToken };
  }

  async function handleRestore(id: number): Promise<TaskMutationWithToken> {
    const operationToken = beginOperation('restore', id);
    const result = await restoreTask(id);
    return { ...result, operationToken };
  }

  async function handlePermanentlyDelete(id: number): Promise<MutationWarningResult> {
    const operationToken = beginOperation('permanent-delete', id);
    const reminderWarning = await permanentlyDeleteTask(id);
    return { reminderWarning, operationToken };
  }

  function setWorkspaceView(view: WorkspaceView): void {
    if (compact || showTrash) return;
    workspaceView = view;
    if (view === 'week') plannerWeekStart = weekStartLocalDate(currentLocalDate);
  }

  function setPlannerWeekStart(weekStart: string): void {
    plannerWeekStart = weekStartLocalDate(weekStart);
  }

  async function handlePlannerReschedule(id: number, plannedDate: string | null): Promise<void> {
    const token = beginOperation('reschedule', id);
    const task = await setTaskPlannedDate(id, plannedDate);
    if (!isCurrentOperation(token)) return;
    upsert(task);
    announcement = task.plannedDate === null
      ? `已移回未排期：${task.title}`
      : `已安排到 ${task.plannedDate}：${task.title}`;
  }

  async function openPlannerTaskInList(id: number): Promise<void> {
    showTrash = false;
    workspaceView = 'list';
    executionView = 'all';
    statusFilter = 'active';
    searchQuery = '';
    await tick();
    const target = document.getElementById(`task-${id}`);
    target?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    document.getElementById(`task-action-${id}`)?.focus();
  }

  function toggleTrash(): void {
    if (compact) return;
    workspaceView = 'list';
    showTrash = !showTrash;
    searchQuery = '';
    if (showTrash) void loadTrash().catch(() => undefined);
  }

  async function handleChanged(task: Task, kind: TaskChangeKind, result: TaskMutationWithToken): Promise<boolean> {
    if (!isCurrentOperation(result.operationToken)) return false;
    const taskIndex = visibleTasks.findIndex((candidate) => candidate.id === task.id);
    const adjacentTaskId = taskIndex < 0
      ? undefined
      : visibleTasks[taskIndex + 1]?.id ?? visibleTasks[taskIndex - 1]?.id;
    upsert(task);
    if (kind === 'complete' && result.nextTask) upsert(result.nextTask);
    reportMutationWarning(result.operationToken, result.reminderWarning, task.id);
    if (kind === 'complete' && result.nextTask) {
      reportMutationWarning(
        result.operationToken,
        result.nextReminderWarning ?? null,
        result.nextTask.id,
      );
    }
    announcement = kind === 'complete'
      ? result.nextTask
        ? `已完成任务：${task.title}；已创建下一次：${result.nextTask.title}`
        : `已完成任务：${task.title}`
      : kind === 'restore'
        ? `已恢复任务：${task.title}`
        : kind === 'snooze'
          ? `已稍后提醒：${task.title}`
          : kind === 'defer'
            ? `已延期到明天：${task.title}`
            : `已更新任务：${task.title}`;
    void onReminderReconcile().catch(() => undefined);
    await tick();
    if (!isCurrentOperation(result.operationToken)) return false;
    if (kind === 'complete') {
      const focusId = adjacentTaskId ?? result.nextTask?.id;
      const action = focusId === undefined ? null : document.getElementById(`task-action-${focusId}`);
      if (action) action.focus();
      else searchInput?.focus();
    } else {
      const action = document.getElementById(`task-action-${task.id}`);
      if (action) action.focus();
      else searchInput?.focus();
    }
    return true;
  }

  async function handleRemoved(id: number, result: MutationWarningResult): Promise<boolean> {
    if (!isCurrentOperation(result.operationToken)) return false;
    const index = tasks.findIndex((task) => task.id === id);
    const focusId = tasks[index + 1]?.id ?? tasks[index - 1]?.id;
    tasks = tasks.filter((task) => task.id !== id);
    void loadTrash().catch(() => undefined);
    reportMutationWarning(result.operationToken, result.reminderWarning);
    announcement = '已移入回收站，可在回收站中恢复。';
    await tick();
    if (!isCurrentOperation(result.operationToken)) return false;
    if (focusId !== undefined) {
      document.getElementById(`task-action-${focusId}`)?.focus();
    } else {
      titleInput?.focus();
    }
    return isCurrentOperation(result.operationToken);
  }

  async function handleRestored(task: Task, result: TaskMutationWithToken): Promise<boolean> {
    if (!isCurrentOperation(result.operationToken)) return false;
    const index = deletedTasks.findIndex((candidate) => candidate.id === task.id);
    const focusId = deletedTasks[index + 1]?.id ?? deletedTasks[index - 1]?.id;
    deletedTasks = deletedTasks.filter((candidate) => candidate.id !== task.id);
    upsert(task);
    reportMutationWarning(result.operationToken, result.reminderWarning);
    announcement = `已从回收站恢复：${task.title}`;
    await tick();
    if (!isCurrentOperation(result.operationToken)) return false;
    if (focusId !== undefined) document.getElementById(`task-action-${focusId}`)?.focus();
    else searchInput?.focus();
    return isCurrentOperation(result.operationToken);
  }

  async function handlePermanentlyRemoved(id: number, result: MutationWarningResult): Promise<boolean> {
    if (!isCurrentOperation(result.operationToken)) return false;
    deletedTasks = deletedTasks.filter((task) => task.id !== id);
    reportMutationWarning(result.operationToken, result.reminderWarning);
    announcement = '已永久删除任务。';
    await tick();
    if (isCurrentOperation(result.operationToken)) searchInput?.focus();
    return isCurrentOperation(result.operationToken);
  }

  async function focusActivation(id: number, nonce: number, message: string): Promise<boolean> {
    if (activationNonce !== nonce) return false;
    await tick();
    if (activationNonce !== nonce) return false;
    const target = document.getElementById(`task-${id}`);
    if (!target) {
      announcement = `无法在当前列表定位任务：${id}`;
      scheduleActivationRetry(nonce);
      searchInput?.focus();
      return false;
    }

    target.scrollIntoView({ behavior: 'smooth', block: 'center' });
    document.getElementById(`task-action-${id}`)?.focus();
    announcement = message;
    lastFocusedActivationNonce = nonce;
    clearActivationRetry();
    activationInProgressNonce = -1;
    onActivationResolved(nonce);
    return true;
  }

  async function revealActivation(id: number, nonce: number): Promise<void> {
    workspaceView = 'list';
    if (tasks.some((task) => task.id === id)) {
      showTrash = false;
      searchQuery = '';
      await focusActivation(id, nonce, `已定位到任务：${id}`);
      return;
    }

    let deleted: Task[] | null;
    try {
      deleted = await loadTrash();
    } catch (cause) {
      if (activationNonce === nonce) {
        showTrash = true;
        scheduleActivationRetry(nonce);
        announcement = `无法读取回收站，暂时无法定位任务：${errorMessage(cause)}`;
        searchInput?.focus();
      }
      return;
    }
    if (activationNonce !== nonce) return;
    if (deleted === null) {
      scheduleActivationRetry(nonce);
      return;
    }

    if (deleted.some((task) => task.id === id)) {
      showTrash = true;
      searchQuery = '';
      await focusActivation(id, nonce, `任务 ${id} 已在回收站，可恢复。`);
    } else {
      announcement = `任务 ${id} 不存在或已永久删除。`;
      lastFocusedActivationNonce = nonce;
      clearActivationRetry();
      activationInProgressNonce = -1;
      searchInput?.focus();
      onActivationResolved(nonce);
    }
  }

  $effect(() => {
    writePreference(STATUS_FILTER_STORAGE_KEY, statusFilter);
    writePreference(EXECUTION_VIEW_STORAGE_KEY, executionView);
    writePreference(PROJECT_SELECTION_STORAGE_KEY, String(selectedProject));
    writePreference(WORKSPACE_VIEW_STORAGE_KEY, workspaceView);
  });

  $effect(() => {
    if (showTrash) workspaceView = 'list';
  });

  $effect(() => {
    if (recurrenceKind === 'none') recurrenceTimezone = null;
    else if (!recurrenceTimezone) recurrenceTimezone = localTimeZone();
  });

  $effect(() => {
    if (typeof window === 'undefined') return;
    const timer = window.setInterval(() => { nowUnixMs = Date.now(); }, 60_000);
    return () => window.clearInterval(timer);
  });

  onDestroy(() => {
    if (typeof window !== 'undefined' && activationRetryTimer !== undefined) {
      window.clearTimeout(activationRetryTimer);
      activationRetryTimer = undefined;
    }
  });

  $effect(() => {
    const id = activationId;
    const nonce = activationNonce;
    if (
      id === null ||
      nonce === lastFocusedActivationNonce ||
      nonce === activationInProgressNonce ||
      activationRetryNonce === nonce ||
      !initialized ||
      !tauriAvailable ||
      loading ||
      !taskListReady
    ) return;
    activationInProgressNonce = nonce;
    void revealActivation(id, nonce);
  });

  $effect(() => {
    if (!initialized || !tauriAvailable) {
      loadedForRuntime = false;
      return;
    }
    if (!loadedForRuntime) {
      loadedForRuntime = true;
      void load();
    }
  });
</script>

<section class="workspace" aria-labelledby="tasks-heading">
  <div class="heading"><div><p class="eyebrow">任务工作区</p><h2 id="tasks-heading">{showTrash ? '回收站' : activeWorkspaceView === 'week' ? '安排这一周' : '今天要推进什么？'}</h2></div>{#if tauriAvailable}<button class="quiet" onclick={load} disabled={loading || trashLoading}>{(loading || trashLoading) ? '刷新中…' : '刷新'}</button>{/if}</div>
  <div class="workspace-tools">
    <label class="search-label" for="task-search">搜索任务</label>
    <input bind:this={searchInput} id="task-search" class="search-input" bind:value={searchQuery} placeholder="搜索标题或备注" autocomplete="off" />
    {#if !compact && !showTrash}
      <div class="workspace-views" role="group" aria-label="工作区视图">
        <button type="button" class:active={activeWorkspaceView === 'list'} aria-pressed={activeWorkspaceView === 'list'} onclick={() => setWorkspaceView('list')}>列表</button>
        <button type="button" class:active={activeWorkspaceView === 'week'} aria-pressed={activeWorkspaceView === 'week'} onclick={() => setWorkspaceView('week')}>周计划</button>
      </div>
    {/if}
    {#if !compact}<button class="quiet" onclick={toggleTrash} disabled={trashLoading}>{showTrash ? '返回任务' : `回收站${deletedTasks.length ? ` (${deletedTasks.length})` : ''}`}</button>{/if}
  </div>
  <p class="sr-only" aria-live="polite">{announcement}</p>

  {#if !initialized}<p class="muted">正在初始化任务工作区…</p>
  {:else if !tauriAvailable}<p class="notice" role="status">浏览器预览已禁用任务持久化；请在桌面应用中管理任务。</p>
  {:else}
    {#if !showTrash && !compact && activeWorkspaceView === 'list'}
    <div class="filters" aria-label="任务筛选">
      <div class="execution-views" role="group" aria-label="执行视图">
        <button type="button" class:active={executionView === 'inbox'} aria-pressed={executionView === 'inbox'} onclick={() => executionView = 'inbox'}>收件箱</button>
        <button type="button" class:active={executionView === 'today'} aria-pressed={executionView === 'today'} onclick={() => executionView = 'today'}>今日</button>
        <button type="button" class:active={executionView === 'overdue'} aria-pressed={executionView === 'overdue'} onclick={() => executionView = 'overdue'}>逾期</button>
        <button type="button" class:active={executionView === 'upcoming'} aria-pressed={executionView === 'upcoming'} onclick={() => executionView = 'upcoming'}>即将到来</button>
        <button type="button" class:active={executionView === 'all'} aria-pressed={executionView === 'all'} onclick={() => executionView = 'all'}>全部</button>
      </div>
      <label>状态
        <select bind:value={statusFilter}>
          <option value="all">全部</option>
          <option value="active">进行中</option>
          <option value="completed">已完成</option>
        </select>
      </label>
    </div>
    {/if}
    {#if compact && !showTrash}<p class="filter-note">紧凑模式固定显示今天的进行中任务；通知激活的任务会临时保留在列表中。</p>{/if}
    {#if showTrash}<p class="filter-note">回收站中的任务不会参与提醒；恢复后会回到正常任务列表。</p>{/if}

    {#if !compact && !showTrash}
      <ProjectSidebar
        {projects}
        tasks={tasks}
        {selectedProject}
        disabled={loading || createBusy}
        onSelect={selectProject}
        onCreate={createProject}
        onRename={renameProject}
        onArchive={archiveProject}
        onRestore={restoreProject}
      />
      {#if activeWorkspaceView === 'list'}
      <TaskComposer tauriAvailable={tauriAvailable} disabled={createBusy || selectedProjectIsArchived} onCreate={handleComposerCreate} />
      <form class="new-task" onsubmit={(event) => { event.preventDefault(); void submit(); }}>
        <div class="form-header"><div><h3>新增任务</h3><p class="form-project">归属：{selectedProject === 'all' ? '收件箱' : selectedProject === 'inbox' ? '收件箱' : selectedProjectRecord?.name ?? '收件箱'}</p></div><button type="submit" disabled={createBusy || selectedProjectIsArchived}>{createBusy ? '添加中…' : '添加任务'}</button></div>
        {#if selectedProjectIsArchived}<p class="error" role="alert">当前项目已归档；恢复项目或切换到收件箱后才能创建任务。</p>{/if}
        <label for="new-title">标题</label>
        <input
          bind:this={titleInput}
          id="new-title"
          bind:value={title}
          placeholder="例如：整理发布清单"
          aria-invalid={Boolean(createErrors.title)}
          aria-describedby={createErrors.title ? 'new-title-error' : undefined}
          oninput={() => clearCreateError('title')}
          disabled={createBusy}
        />
        {#if createErrors.title}<p class="error" id="new-title-error">{createErrors.title}</p>{/if}
        <label for="new-notes">备注 <span>可选</span></label>
        <textarea
          id="new-notes"
          bind:value={notes}
          placeholder="补充上下文、下一步或链接"
          aria-invalid={Boolean(createErrors.notes)}
          aria-describedby={createErrors.notes ? 'new-notes-error' : undefined}
          oninput={() => clearCreateError('notes')}
          disabled={createBusy}
        ></textarea>
        {#if createErrors.notes}<p class="error" id="new-notes-error">{createErrors.notes}</p>{/if}
        <label for="new-planned">计划日期 <span>可选</span></label>
        <input
          id="new-planned"
          type="date"
          bind:value={plannedDate}
          aria-invalid={Boolean(createErrors.plannedDate)}
          aria-describedby={createErrors.plannedDate ? 'new-planned-error' : undefined}
          oninput={() => clearCreateError('plannedDate')}
          disabled={createBusy}
        />
        {#if createErrors.plannedDate}<p class="error" id="new-planned-error">{createErrors.plannedDate}</p>{/if}
        <label for="new-due">截止时间 <span>可选</span></label>
        <input
          id="new-due"
          type="datetime-local"
          bind:value={dueAt}
          aria-invalid={Boolean(createErrors.dueAtUnixMs)}
          aria-describedby={createErrors.dueAtUnixMs ? 'new-due-error' : undefined}
          oninput={() => clearCreateError('dueAtUnixMs')}
          disabled={createBusy}
        />
        {#if createErrors.dueAtUnixMs}<p class="error" id="new-due-error">{createErrors.dueAtUnixMs}</p>{/if}
        <label for="new-reminder">提醒时间 <span>可选</span></label>
        <input
          id="new-reminder"
          type="datetime-local"
          bind:value={reminderAt}
          aria-invalid={Boolean(createErrors.reminderAtUnixMs)}
          aria-describedby={createErrors.reminderAtUnixMs ? 'new-reminder-error' : undefined}
          oninput={() => clearCreateError('reminderAtUnixMs')}
          disabled={createBusy}
        />
        {#if createErrors.reminderAtUnixMs}<p class="error" id="new-reminder-error">{createErrors.reminderAtUnixMs}</p>{/if}
        <label for="new-priority">优先级</label>
        <select
          id="new-priority"
          bind:value={priority}
          aria-invalid={Boolean(createErrors.priority)}
          aria-describedby={createErrors.priority ? 'new-priority-error' : undefined}
          onchange={() => clearCreateError('priority')}
          disabled={createBusy}
        >{#each PRIORITY_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>
        {#if createErrors.priority}<p class="error" id="new-priority-error">{createErrors.priority}</p>{/if}
        <label for="new-recurrence">重复</label>
        <select
          id="new-recurrence"
          bind:value={recurrenceKind}
          aria-invalid={Boolean(createErrors.recurrenceKind || createErrors.recurrenceTimezone)}
          aria-describedby={createErrors.recurrenceKind ? 'new-recurrence-error' : createErrors.recurrenceTimezone ? 'new-recurrence-timezone-error' : undefined}
          onchange={handleCreateRecurrenceChange}
          disabled={createBusy}
        >{#each RECURRENCE_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>
        {#if createErrors.recurrenceKind}<p class="error" id="new-recurrence-error">{createErrors.recurrenceKind}</p>{/if}
        {#if createErrors.recurrenceTimezone}<p class="error" id="new-recurrence-timezone-error">{createErrors.recurrenceTimezone}</p>{/if}
        {#if recurrenceKind !== 'none'}<p class="field-note">将按 {recurrenceTimezone ?? localTimeZone()} 的本地日历生成后续实例。</p>{/if}
        {#if createRequestError}<p class="error" role="alert">{createRequestError}</p>{/if}
      </form>
      {/if}
    {:else if compact}
      <p class="compact-note">切换到完整模式可创建任务、编辑字段和调整筛选。</p>
    {/if}

    {#if showTrash}
      {#if trashLoadError}<div class="load-error" role="alert"><span>回收站未能加载：{trashLoadError}</span><button onclick={load} disabled={trashLoading}>{trashLoading ? '重试中…' : '重试'}</button></div>
      {:else if trashLoading}<p class="muted">正在读取回收站…</p>
      {:else if deletedTasks.length === 0}<p class="empty">回收站是空的。</p>
      {:else if visibleTasks.length === 0}<p class="empty">当前搜索没有匹配的已删除任务。</p>
      {:else}
        <section class="task-group trash-group" aria-labelledby="trash-heading"><h3 id="trash-heading">已删除 <span>{visibleTasks.length}</span></h3>
          {#each visibleTasks as task (task.id)}<TaskItem {task} {projects} highlighted={task.id === activationId} trashMode={true} onUpdate={handleUpdate} onCompleted={handleCompleted} onDelete={handleDelete} onChanged={handleChanged} onRemoved={handleRemoved} onRestore={handleRestore} onPermanentlyDelete={handlePermanentlyDelete} onRestored={handleRestored} onPermanentlyRemoved={handlePermanentlyRemoved} />{/each}
        </section>
      {/if}
    {:else if loadError}<div class="load-error" role="alert"><span>任务未能加载：{loadError}</span><button onclick={load} disabled={loading}>{loading ? '重试中…' : '重试'}</button></div>
    {:else if loading}<p class="muted">正在读取任务…</p>
    {:else if activeWorkspaceView === 'week'}
      <WeekPlanner
        tasks={plannerTasks}
        {projects}
        weekStart={plannerWeekStart}
        today={currentLocalDate}
        onWeekChange={setPlannerWeekStart}
        onReschedule={handlePlannerReschedule}
        onOpenTask={openPlannerTaskInList}
      />
    {:else if tasks.length === 0}<p class="empty">还没有任务。从上方写下第一件要推进的事。</p>
    {:else if visibleTasks.length === 0}<p class="empty">当前筛选没有匹配任务。</p>
    {:else}
      {#if activeTasks.length || statusFilter !== 'completed'}
        <section class="task-group" aria-labelledby="active-heading"><h3 id="active-heading">进行中 <span>{activeTasks.length}</span></h3>
          {#if activeTasks.length}{#each activeTasks as task (task.id)}<TaskItem {task} {projects} highlighted={task.id === activationId} compactMode={compact} onUpdate={handleUpdate} onCompleted={handleCompleted} onSnooze={handleSnooze} onDeferToTomorrow={handleDefer} onDelete={handleDelete} onChanged={handleChanged} onRemoved={handleRemoved} />{/each}{:else}<p class="muted">目前没有匹配的进行中任务。</p>{/if}
        </section>
      {/if}
      {#if completedTasks.length || statusFilter !== 'active'}
        <section class="task-group completed" aria-labelledby="completed-heading"><h3 id="completed-heading">已完成 <span>{completedTasks.length}</span></h3>
          {#if completedTasks.length}{#each completedTasks as task (task.id)}<TaskItem {task} {projects} highlighted={task.id === activationId} compactMode={compact} onUpdate={handleUpdate} onCompleted={handleCompleted} onSnooze={handleSnooze} onDeferToTomorrow={handleDefer} onDelete={handleDelete} onChanged={handleChanged} onRemoved={handleRemoved} />{/each}{:else}<p class="muted">目前没有匹配的已完成任务。</p>{/if}
        </section>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .workspace { padding:20px 0; } .workspace-tools { display:flex; align-items:center; gap:8px; padding-top:14px; } .workspace-views { display:flex; flex:none; gap:4px; } .workspace-views button { border:1px solid var(--line); border-radius:4px; padding:5px 7px; color:var(--muted); background:transparent; font-size:12px; } .workspace-views button.active { color:#e7eef5; border-color:rgba(91,169,255,.65); background:rgba(91,169,255,.18); } .form-project { margin:4px 0 0; color:var(--muted); font-size:11px; } .search-label { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; } .search-input { min-width:0; flex:1; border:1px solid var(--line); border-radius:4px; padding:7px 9px; color:inherit; background:rgba(0,0,0,.14); } .trash-group { padding-top:12px; }.heading,.form-header { display:flex; align-items:start; justify-content:space-between; gap:12px; }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; } h2,h3 { margin:4px 0 0; font-weight:600; } h2 { font-size:19px; } h3 { font-size:13px; }.filters { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:14px 0 4px; border-bottom:1px solid var(--line); }.execution-views { display:flex; flex-wrap:wrap; gap:5px; }.execution-views button { border:1px solid var(--line); border-radius:4px; padding:5px 7px; color:var(--muted); background:transparent; font-size:12px; }.execution-views button.active { color:#e7eef5; border-color:rgba(91,169,255,.65); background:rgba(91,169,255,.18); }.filters label { display:flex; align-items:center; gap:6px; color:var(--muted); font-size:11px; }.filters select { border:1px solid var(--line); border-radius:4px; padding:5px 7px; color:#e7eef5; background:#171a1f; font-size:12px; }.filter-note,.compact-note,.field-note { margin:8px 0 0; color:var(--muted); font-size:11px; line-height:1.5; }.task-group h3 { color:var(--muted); font-size:11px; letter-spacing:.08em; text-transform:uppercase; }.task-group h3 span { color:#d8e1eb; }.new-task { display:grid; gap:6px; padding:18px 0; border-bottom:1px solid var(--line); }.new-task label { color:var(--muted); font-size:11px; }.new-task label span { color:#707a86; }.new-task input,.new-task textarea,.new-task select { width:100%; border:1px solid var(--line); border-radius:4px; padding:8px 9px; color:inherit; background:rgba(0,0,0,.14); }.new-task textarea { min-height:68px; resize:vertical; }.task-group { padding-top:20px; }.task-group.completed { margin-top:8px; }.muted,.empty { color:var(--muted); font-size:12px; line-height:1.55; }.empty { padding:25px 0; border-bottom:1px solid var(--line); }.notice,.load-error { margin:16px 0; padding:10px 11px; color:#d9c08f; background:rgba(224,173,101,.09); border-left:2px solid #e0ad65; font-size:12px; }.load-error { display:flex; align-items:center; justify-content:space-between; gap:10px; color:#ffcece; border-color:#db7777; }.quiet { border:0; padding:4px 6px; color:var(--muted); background:transparent; font-size:12px; }.error { margin:2px 0; color:#ffaeae; font-size:12px; }.sr-only { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; } button:focus-visible,input:focus-visible,textarea:focus-visible,select:focus-visible { outline:2px solid var(--blue); outline-offset:2px; }
</style>
