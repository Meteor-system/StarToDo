<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import ContextDrawer from '$lib/components/ContextDrawer.svelte';
  import ProjectSidebar from '$lib/components/ProjectSidebar.svelte';
  import TaskCanvas from '$lib/components/TaskCanvas.svelte';
  import TaskComposer from '$lib/components/TaskComposer.svelte';
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

  type TaskDrawer = 'create' | 'filters' | 'projects' | 'planner' | 'trash' | null;

  interface Props {
    tauriAvailable: boolean;
    initialized: boolean;
    activationId: number | null;
    activationNonce: number;
    onReminderReconcile: () => Promise<ReminderReport>;
    onMutationWarning: (context: ReminderWarningContext) => void;
    onActivationResolved: (nonce: number) => void;
    pomodoroCounts?: ReadonlyMap<number, number>;
    onStartFocus?: (taskId: number) => void;
    onPomodoroTasksChanged?: (unavailableTaskId?: number) => void;
  }

  const STATUS_FILTER_STORAGE_KEY = 'startodo.status-filter';
  const EXECUTION_VIEW_STORAGE_KEY = 'startodo.execution-view';
  const PROJECT_SELECTION_STORAGE_KEY = 'startodo.project-selection';
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
    activationId,
    activationNonce,
    onReminderReconcile,
    onMutationWarning,
    onActivationResolved,
    pomodoroCounts = new Map<number, number>(),
    onStartFocus,
    onPomodoroTasksChanged
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
  let activeDrawer = $state<TaskDrawer>(null);
  let plannerWeekStart = $state(weekStartLocalDate(todayLocalDate()));
  let nowUnixMs = $state(Date.now());
  let searchQuery = $state('');
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
  let visibleTasks = $derived(tasks.filter((task) => isVisible(task) && searchMatches(task, true)));
  let visibleDeletedTasks = $derived(deletedTasks.filter((task) => searchMatches(task, true)));
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
  }

  function openTaskDrawer(drawer: Exclude<TaskDrawer, null>): void {
    activeDrawer = drawer;
    if (drawer === 'trash') void loadTrash().catch(() => undefined);
    if (drawer === 'planner') plannerWeekStart = weekStartLocalDate(currentLocalDate);
  }

  function closeTaskDrawer(): void {
    activeDrawer = null;
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
    const view = executionView;
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
    const filter = statusFilter;
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
      if (!taskListReady || loadError !== null) void load();
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
        onPomodoroTasksChanged?.();
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
        if (refreshRequested) {
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
      onPomodoroTasksChanged?.();
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
        if (createdCount) {
          onPomodoroTasksChanged?.();
          void onReminderReconcile().catch(() => undefined);
        }
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
    activeDrawer = null;
    executionView = 'all';
    statusFilter = 'active';
    searchQuery = '';
    await tick();
    const region = document.querySelector<HTMLElement>('[data-scroll-region="tasks"]');
    const target = document.getElementById(`task-${id}`);
    if (region && target) region.scrollTo({ top: Math.max(0, target.offsetTop - region.clientHeight / 2), behavior: 'smooth' });
    document.getElementById(`task-action-${id}`)?.focus();
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
    if (kind === 'complete') onPomodoroTasksChanged?.(task.id);
    else if (kind === 'restore' || kind === 'update') onPomodoroTasksChanged?.();
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
    onPomodoroTasksChanged?.(id);
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
    onPomodoroTasksChanged?.();
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
    onPomodoroTasksChanged?.(id);
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

    const region = document.querySelector<HTMLElement>('[data-scroll-region="tasks"]');
    if (region) {
      region.scrollTo({ top: Math.max(0, target.offsetTop - region.clientHeight / 2), behavior: 'smooth' });
    }
    document.getElementById(`task-action-${id}`)?.focus();
    announcement = message;
    lastFocusedActivationNonce = nonce;
    clearActivationRetry();
    activationInProgressNonce = -1;
    onActivationResolved(nonce);
    return true;
  }

  async function revealActivation(id: number, nonce: number): Promise<void> {
    activeDrawer = null;
    if (tasks.some((task) => task.id === id)) {
      searchQuery = '';
      await focusActivation(id, nonce, `已定位到任务：${id}`);
      return;
    }

    let deleted: Task[] | null;
    try {
      deleted = await loadTrash();
    } catch (cause) {
      if (activationNonce === nonce) {
        openTaskDrawer('trash');
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
      openTaskDrawer('trash');
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

<section class="task-scene" aria-labelledby="tasks-heading">
  <header class="scene-header">
    <div><p class="eyebrow">任务场景</p><h2 id="tasks-heading">今天要推进什么？</h2></div>
    {#if tauriAvailable}<button class="quiet" onclick={load} disabled={loading || trashLoading}>{(loading || trashLoading) ? '刷新中…' : '刷新'}</button>{/if}
  </header>
  <div class="scene-tools">
    <label class="search-label" for="task-search">搜索任务</label>
    <input bind:this={searchInput} id="task-search" class="search-input" bind:value={searchQuery} placeholder="搜索标题或备注" autocomplete="off" />
    <button type="button" onclick={() => openTaskDrawer('create')}>详细新建</button><button type="button" onclick={() => openTaskDrawer('filters')}>筛选</button><button type="button" onclick={() => openTaskDrawer('projects')}>项目</button><button type="button" onclick={() => openTaskDrawer('planner')}>周计划</button><button type="button" onclick={() => openTaskDrawer('trash')} disabled={trashLoading}>回收站{deletedTasks.length ? ` (${deletedTasks.length})` : ''}</button>
  </div>
  <p class="sr-only" aria-live="polite">{announcement}</p>
  {#if !initialized}<p class="muted">正在初始化任务场景…</p>{:else if !tauriAvailable}<p class="notice" role="status">浏览器预览已禁用任务持久化；请在桌面应用中管理任务。</p>{:else if loadError}<div class="load-error" role="alert"><span>任务未能加载：{loadError}</span><button onclick={load} disabled={loading}>{loading ? '重试中…' : '重试'}</button></div>{:else if loading}<p class="muted">正在读取任务…</p>{:else}
    <TaskCanvas activeTasks={activeTasks} completedTasks={completedTasks} trashTasks={visibleDeletedTasks} {projects} {activationId} {pomodoroCounts} onOpenDetails={() => undefined} onFocus={onStartFocus} onUpdate={handleUpdate} onCompleted={handleCompleted} onSnooze={handleSnooze} onDeferToTomorrow={handleDefer} onDelete={handleDelete} onChanged={handleChanged} onRemoved={handleRemoved} onRestore={handleRestore} onPermanentlyDelete={handlePermanentlyDelete} onRestored={handleRestored} onPermanentlyRemoved={handlePermanentlyRemoved} />
  {/if}
</section>

<ContextDrawer open={activeDrawer === 'create'} drawerId="create-task-drawer" title="详细新建" onClose={closeTaskDrawer}>
  <TaskComposer tauriAvailable={tauriAvailable} disabled={createBusy || selectedProjectIsArchived} onCreate={handleComposerCreate} />
  <form class="new-task" onsubmit={(event) => { event.preventDefault(); void submit(); }}>
    <div class="form-header"><div><h3>详细任务</h3><p class="form-project">归属：{selectedProject === 'all' || selectedProject === 'inbox' ? '收件箱' : selectedProjectRecord?.name ?? '收件箱'}</p></div><button type="submit" disabled={createBusy || selectedProjectIsArchived}>{createBusy ? '添加中…' : '添加任务'}</button></div>
    {#if selectedProjectIsArchived}<p class="error" role="alert">当前项目已归档；恢复项目或切换到收件箱后才能创建任务。</p>{/if}
    <label for="new-title">标题</label><input bind:this={titleInput} id="new-title" bind:value={title} placeholder="例如：整理发布清单" aria-invalid={Boolean(createErrors.title)} aria-describedby={createErrors.title ? 'new-title-error' : undefined} oninput={() => clearCreateError('title')} disabled={createBusy} />{#if createErrors.title}<p class="error" id="new-title-error">{createErrors.title}</p>{/if}
    <label for="new-notes">备注 <span>可选</span></label><textarea id="new-notes" bind:value={notes} placeholder="补充上下文、下一步或链接" aria-invalid={Boolean(createErrors.notes)} aria-describedby={createErrors.notes ? 'new-notes-error' : undefined} oninput={() => clearCreateError('notes')} disabled={createBusy}></textarea>{#if createErrors.notes}<p class="error" id="new-notes-error">{createErrors.notes}</p>{/if}
    <label for="new-planned">计划日期 <span>可选</span></label><input id="new-planned" type="date" bind:value={plannedDate} aria-invalid={Boolean(createErrors.plannedDate)} aria-describedby={createErrors.plannedDate ? 'new-planned-error' : undefined} oninput={() => clearCreateError('plannedDate')} disabled={createBusy} />{#if createErrors.plannedDate}<p class="error" id="new-planned-error">{createErrors.plannedDate}</p>{/if}
    <label for="new-due">截止时间 <span>可选</span></label><input id="new-due" type="datetime-local" bind:value={dueAt} aria-invalid={Boolean(createErrors.dueAtUnixMs)} aria-describedby={createErrors.dueAtUnixMs ? 'new-due-error' : undefined} oninput={() => clearCreateError('dueAtUnixMs')} disabled={createBusy} />{#if createErrors.dueAtUnixMs}<p class="error" id="new-due-error">{createErrors.dueAtUnixMs}</p>{/if}
    <label for="new-reminder">提醒时间 <span>可选</span></label><input id="new-reminder" type="datetime-local" bind:value={reminderAt} aria-invalid={Boolean(createErrors.reminderAtUnixMs)} aria-describedby={createErrors.reminderAtUnixMs ? 'new-reminder-error' : undefined} oninput={() => clearCreateError('reminderAtUnixMs')} disabled={createBusy} />{#if createErrors.reminderAtUnixMs}<p class="error" id="new-reminder-error">{createErrors.reminderAtUnixMs}</p>{/if}
    <label for="new-priority">优先级</label><select id="new-priority" bind:value={priority} aria-invalid={Boolean(createErrors.priority)} aria-describedby={createErrors.priority ? 'new-priority-error' : undefined} onchange={() => clearCreateError('priority')} disabled={createBusy}>{#each PRIORITY_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>{#if createErrors.priority}<p class="error" id="new-priority-error">{createErrors.priority}</p>{/if}
    <label for="new-recurrence">重复</label><select id="new-recurrence" bind:value={recurrenceKind} aria-invalid={Boolean(createErrors.recurrenceKind || createErrors.recurrenceTimezone)} aria-describedby={createErrors.recurrenceKind ? 'new-recurrence-error' : createErrors.recurrenceTimezone ? 'new-recurrence-timezone-error' : undefined} onchange={handleCreateRecurrenceChange} disabled={createBusy}>{#each RECURRENCE_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select>{#if createErrors.recurrenceKind}<p class="error" id="new-recurrence-error">{createErrors.recurrenceKind}</p>{/if}{#if createErrors.recurrenceTimezone}<p class="error" id="new-recurrence-timezone-error">{createErrors.recurrenceTimezone}</p>{/if}{#if recurrenceKind !== 'none'}<p class="field-note">将按 {recurrenceTimezone ?? localTimeZone()} 的本地日历生成后续实例。</p>{/if}{#if createRequestError}<p class="error" role="alert">{createRequestError}</p>{/if}
  </form>
</ContextDrawer>
<ContextDrawer open={activeDrawer === 'filters'} drawerId="filters-drawer" title="筛选" onClose={closeTaskDrawer}><div class="filters" aria-label="任务筛选"><div class="execution-views" role="group" aria-label="执行视图">{#each [['inbox','收件箱'],['today','今日'],['overdue','逾期'],['upcoming','即将到来'],['all','全部']] as view}<button type="button" class:active={executionView === view[0]} aria-pressed={executionView === view[0]} onclick={() => executionView = view[0] as ExecutionView}>{view[1]}</button>{/each}</div><label>状态<select bind:value={statusFilter}><option value="all">全部</option><option value="active">进行中</option><option value="completed">已完成</option></select></label></div></ContextDrawer>
<ContextDrawer open={activeDrawer === 'projects'} drawerId="projects-drawer" title="项目" onClose={closeTaskDrawer}><ProjectSidebar {projects} tasks={tasks} {selectedProject} disabled={loading || createBusy} onSelect={selectProject} onCreate={createProject} onRename={renameProject} onArchive={archiveProject} onRestore={restoreProject} /></ContextDrawer>
<ContextDrawer open={activeDrawer === 'planner'} drawerId="planner-drawer" title="周计划" size="wide" onClose={closeTaskDrawer}><WeekPlanner tasks={plannerTasks} {projects} weekStart={plannerWeekStart} today={currentLocalDate} onWeekChange={setPlannerWeekStart} onReschedule={handlePlannerReschedule} onOpenTask={openPlannerTaskInList} /></ContextDrawer>
<ContextDrawer open={activeDrawer === 'trash'} drawerId="trash-drawer" title="回收站" onClose={closeTaskDrawer}>{#if trashLoadError}<div class="load-error" role="alert"><span>回收站未能加载：{trashLoadError}</span><button onclick={() => void loadTrash()} disabled={trashLoading}>{trashLoading ? '重试中…' : '重试'}</button></div>{:else if trashLoading}<p class="muted">正在读取回收站…</p>{:else}<TaskCanvas activeTasks={[]} completedTasks={[]} trashTasks={visibleDeletedTasks} {projects} {activationId} {pomodoroCounts} trashMode={true} onOpenDetails={() => undefined} onUpdate={handleUpdate} onCompleted={handleCompleted} onSnooze={handleSnooze} onDeferToTomorrow={handleDefer} onDelete={handleDelete} onChanged={handleChanged} onRemoved={handleRemoved} onRestore={handleRestore} onPermanentlyDelete={handlePermanentlyDelete} onRestored={handleRestored} onPermanentlyRemoved={handlePermanentlyRemoved} />{/if}</ContextDrawer>

<style>
  .task-scene { height:100%; min-height:0; display:grid; grid-template-rows:auto auto minmax(0,1fr); container-type:inline-size; }
  .scene-header,.form-header { display:flex; align-items:start; justify-content:space-between; gap:12px; }
  .scene-tools { display:flex; align-items:center; gap:8px; padding:14px 0; min-width:0; }
  .scene-tools button { flex:none; border:1px solid var(--line); border-radius:var(--radius-sm); padding:7px 8px; color:var(--text-soft); background:transparent; font-size:12px; }
  .scene-tools button:hover:not(:disabled) { border-color:var(--text-soft); background:var(--surface-hover); color:var(--text); }
  .search-label { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; }
  .search-input { min-width:0; flex:1; border:1px solid var(--line); border-radius:var(--radius-sm); padding:7px 9px; color:inherit; background:rgba(0,0,0,.14); }
  .eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; }
  h2,h3 { margin:4px 0 0; font-weight:600; } h2 { font-size:19px; } h3 { font-size:13px; }
  .new-task { display:grid; gap:8px; padding-top:16px; } .new-task label { color:var(--muted); font-size:11px; } .new-task input,.new-task textarea,.new-task select { min-width:0; border:1px solid var(--line-strong); border-radius:var(--radius-sm); padding:8px; color:var(--text); background:var(--surface); } .new-task textarea { min-height:76px; resize:vertical; }
  .form-project { margin:4px 0 0; color:var(--muted); font-size:11px; } .form-header button { border:1px solid var(--accent); border-radius:var(--radius-sm); padding:7px 10px; color:var(--accent-ink); background:var(--accent); font-size:12px; }
  .filters { display:grid; gap:14px; padding-top:16px; } .execution-views { display:flex; flex-wrap:wrap; gap:5px; } .execution-views button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:6px 8px; color:var(--muted); background:transparent; font-size:12px; } .execution-views button.active { color:var(--text); border-color:rgba(91,169,255,.65); background:rgba(91,169,255,.18); } .filters label { display:flex; align-items:center; gap:8px; color:var(--muted); font-size:11px; } .filters select { border:1px solid var(--line); border-radius:var(--radius-sm); padding:6px 8px; color:var(--text); background:var(--surface); }
  .error { margin:0; color:var(--danger); font-size:12px; line-height:1.5; } .field-note { margin:0; color:var(--muted); font-size:11px; } .load-error { display:flex; align-items:center; justify-content:space-between; gap:10px; padding-top:14px; color:var(--danger); font-size:12px; } .load-error button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:5px 8px; color:var(--text-soft); background:transparent; }
  .quiet { border:0; border-radius:var(--radius-sm); padding:5px 7px; color:var(--muted); background:transparent; font-size:11px; } .sr-only { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; }
  @container (max-width:640px) { .scene-tools { flex-wrap:wrap; } .search-input { flex-basis:100%; order:-1; } .scene-tools button { flex:1; } }
</style>