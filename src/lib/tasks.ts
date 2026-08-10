import { invoke } from '@tauri-apps/api/core';

export type Priority = 'none' | 'low' | 'medium' | 'high';
export type RecurrenceKind = 'none' | 'daily' | 'weekly';
export type StatusFilter = 'all' | 'active' | 'completed';
export type DateFilter = 'today' | 'week' | 'all';
export type ExecutionView = 'inbox' | 'today' | 'overdue' | 'upcoming' | 'all';
export type ProjectSelection = 'all' | 'inbox' | number;

export interface Task {
  id: number;
  title: string;
  notes: string;
  plannedDate: string | null;
  dueAtUnixMs: number | null;
  reminderAtUnixMs: number | null;
  reminderFiredAtUnixMs: number | null;
  deletedAtUnixMs: number | null;
  projectId: number | null;
  priority: Priority;
  recurrenceKind: RecurrenceKind;
  recurrenceSeriesId: number | null;
  recurrenceSourceTaskId: number | null;
  recurrenceTimezone: string | null;
  recurrenceDstPolicy: string | null;
  completedAtUnixMs: number | null;
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
}

export interface TaskInput {
  title: string;
  notes: string;
  plannedDate: string | null;
  dueAtUnixMs: number | null;
  reminderAtUnixMs: number | null;
  projectId: number | null;
  priority: Priority;
  recurrenceKind: RecurrenceKind;
  recurrenceTimezone: string | null;
}

export interface CreateTaskInput extends TaskInput {}
export interface UpdateTaskInput extends TaskInput {}
export interface TaskMutation {
  task: Task;
  reminderWarning: string | null;
}

export interface TaskCompletionMutation extends TaskMutation {
  nextTask: Task | null;
  nextReminderWarning: string | null;
}

export type TaskOperation =
  | 'create'
  | 'update'
  | 'reschedule'
  | 'complete'
  | 'restore'
  | 'snooze'
  | 'defer'
  | 'delete'
  | 'permanent-delete';

/** UI-only identity used to discard a stale completion of the same operation. */
export interface TaskOperationToken {
  sequence: number;
  operation: TaskOperation;
  taskId: number | null;
}

export interface TaskMutationWithToken extends TaskMutation {
  operationToken: TaskOperationToken;
  nextTask?: Task | null;
  nextReminderWarning?: string | null;
}

export interface TaskCompletionMutationWithToken extends TaskCompletionMutation {
  operationToken: TaskOperationToken;
}

export interface MutationWarningResult {
  reminderWarning: string | null;
  operationToken: TaskOperationToken;
}

export interface ReminderWarningContext {
  source: 'mutation' | 'event';
  operation: TaskOperation | 'reminder-fired';
  taskId: number | null;
  message: string | null;
  sequence: number;
}

export interface ReminderWarningEvent {
  id: number;
  taskId: number;
  message: string;
}

export interface PendingActivation {
  id: number;
  taskId: number;
  receivedAtUnixMs: number;
}

export interface ActivationAckResult {
  acknowledgedIds: number[];
}

export interface Project {
  id: number;
  name: string;
  archivedAtUnixMs: number | null;
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
}

export interface ProjectInput {
  name: string;
}

export interface ReliabilityIncident {
  id: number;
  kind: 'reminder_host' | 'database';
  taskId: number | null;
  operation: string;
  message: string;
  status: 'open' | 'resolved' | 'acknowledged';
  firstSeenAtUnixMs: number;
  lastSeenAtUnixMs: number;
  occurrenceCount: number;
  resolvedAtUnixMs: number | null;
  acknowledgedAtUnixMs: number | null;
}

export interface ReminderReport {
  scheduled: number;
  cancelled: number;
  missedCount: number;
  missedTaskIds: number[];
  capability: 'osScheduled' | 'sendNow';
  warning: string | null;
}

export interface TaskDraft {
  title: string;
  notes: string;
  plannedDate: string | null;
  priority: Priority;
}

export interface BatchCreateResult {
  createdCount: number;
  remainingDrafts: TaskDraft[];
  error: string | null;
}

export type TaskField =
  | 'title'
  | 'notes'
  | 'plannedDate'
  | 'dueAtUnixMs'
  | 'reminderAtUnixMs'
  | 'priority'
  | 'recurrenceKind'
  | 'recurrenceTimezone';
export type TaskFieldErrors = Partial<Record<TaskField, string>>;
export type TaskChangeKind = 'update' | 'complete' | 'restore' | 'snooze' | 'defer';

export const PRIORITY_OPTIONS: Array<{ value: Priority; label: string }> = [
  { value: 'none', label: '无优先级' },
  { value: 'low', label: '低' },
  { value: 'medium', label: '中' },
  { value: 'high', label: '高' },
];

export const RECURRENCE_OPTIONS: Array<{ value: RecurrenceKind; label: string }> = [
  { value: 'none', label: '不重复' },
  { value: 'daily', label: '每天' },
  { value: 'weekly', label: '每周' },
];

const MAX_JAVASCRIPT_DATE_UNIX_MS = 8_640_000_000_000_000;
const unicodeLength = (value: string): number => Array.from(value).length;

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function compareNumbers(left: number, right: number): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function compareNullableStrings(left: string | null, right: string | null): number {
  if (left === right) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return left < right ? -1 : 1;
}

function priorityRank(priority: Priority): number {
  return priority === 'high' ? 3 : priority === 'medium' ? 2 : priority === 'low' ? 1 : 0;
}

export function isValidPlannedDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const [, yearText, monthText, dayText] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const date = new Date(0);
  date.setHours(0, 0, 0, 0);
  date.setFullYear(year, month - 1, day);
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day;
}

function formatLocalDate(date: Date): string {
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function todayLocalDate(): string {
  return formatLocalDate(new Date());
}

export function localDateForEpochMs(value: number): string | null {
  if (!Number.isSafeInteger(value) || value < 0) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : formatLocalDate(date);
}

export function addLocalCalendarDays(value: string, days: number): string | null {
  if (!isValidPlannedDate(value) || !Number.isInteger(days)) return null;
  const [yearText, monthText, dayText] = value.split('-');
  const date = new Date(0);
  date.setHours(0, 0, 0, 0);
  date.setFullYear(Number(yearText), Number(monthText) - 1, Number(dayText));
  date.setDate(date.getDate() + days);
  return formatLocalDate(date);
}

export function localTimeZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

export function isValidIanaTimeZone(value: string): boolean {
  try {
    Intl.DateTimeFormat(undefined, { timeZone: value });
    return true;
  } catch {
    return false;
  }
}

export function recurrenceLabel(kind: RecurrenceKind): string {
  return RECURRENCE_OPTIONS.find((option) => option.value === kind)?.label ?? '不重复';
}

export function weekStartLocalDate(value: string): string {
  if (!isValidPlannedDate(value)) return todayLocalDate();
  const [yearText, monthText, dayText] = value.split('-');
  const date = new Date(0);
  date.setHours(0, 0, 0, 0);
  date.setFullYear(Number(yearText), Number(monthText) - 1, Number(dayText));
  date.setDate(date.getDate() - ((date.getDay() + 6) % 7));
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function weekEndLocalDate(value: string): string {
  if (!isValidPlannedDate(value)) return todayLocalDate();
  const [yearText, monthText, dayText] = value.split('-');
  const date = new Date(0);
  date.setHours(0, 0, 0, 0);
  date.setFullYear(Number(yearText), Number(monthText) - 1, Number(dayText));
  date.setDate(date.getDate() + 6 - ((date.getDay() + 6) % 7));
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function priorityLabel(priority: Priority): string {
  return PRIORITY_OPTIONS.find((option) => option.value === priority)?.label ?? '无优先级';
}

export function validateTaskInput(input: TaskInput): TaskFieldErrors {
  const errors: TaskFieldErrors = {};
  const titleLength = unicodeLength(input.title.trim());
  if (titleLength < 1 || titleLength > 200) errors.title = '标题需为 1 到 200 个字符。';
  if (unicodeLength(input.notes) > 10_000) errors.notes = '备注最多可包含 10,000 个字符。';
  if (input.plannedDate !== null && !isValidPlannedDate(input.plannedDate)) {
    errors.plannedDate = '计划日期无效。';
  }
  if (
    input.dueAtUnixMs !== null &&
    (!Number.isSafeInteger(input.dueAtUnixMs) || input.dueAtUnixMs < 0 || input.dueAtUnixMs > MAX_JAVASCRIPT_DATE_UNIX_MS)
  ) {
    errors.dueAtUnixMs = '截止时间无效。';
  }
  if (
    input.reminderAtUnixMs !== null &&
    (!Number.isSafeInteger(input.reminderAtUnixMs) || input.reminderAtUnixMs < 0 || input.reminderAtUnixMs > MAX_JAVASCRIPT_DATE_UNIX_MS)
  ) {
    errors.reminderAtUnixMs = '提醒时间无效。';
  }
  if (!PRIORITY_OPTIONS.some((option) => option.value === input.priority)) {
    errors.priority = '优先级无效。';
  }
  if (!RECURRENCE_OPTIONS.some((option) => option.value === input.recurrenceKind)) {
    errors.recurrenceKind = '重复规则无效。';
  } else if (input.recurrenceKind !== 'none') {
    if (input.plannedDate === null || !isValidPlannedDate(input.plannedDate)) {
      errors.plannedDate = '重复任务需要有效的计划日期。';
    }
    if (!input.recurrenceTimezone || !isValidIanaTimeZone(input.recurrenceTimezone)) {
      errors.recurrenceTimezone = '无法使用当前时区创建重复任务。';
    }
  }
  return errors;
}

export function localDateTimeToEpochMs(value: string): number | null {
  if (!value) return null;
  const match = /^(\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,3}))?)?$/.exec(value);
  if (!match) return null;
  const [, yearText, monthText, dayText, hourText, minuteText, secondText = '0', msText = '0'] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const second = Number(secondText);
  const millisecond = Number(msText.padEnd(3, '0'));
  const date = new Date(year, month - 1, day, hour, minute, second, millisecond);
  if (
    date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day ||
    date.getHours() !== hour || date.getMinutes() !== minute || date.getSeconds() !== second ||
    !Number.isSafeInteger(date.getTime()) || date.getTime() < 0
  ) return null;
  return date.getTime();
}

export function epochMsToLocalDateTime(value: number | null): string {
  if (value === null || !Number.isSafeInteger(value) || value < 0) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Mirrors the database ORDER BY clause so optimistic local placement is stable. */
export function compareTasks(left: Task, right: Task): number {
  const leftCompleted = left.completedAtUnixMs !== null;
  const rightCompleted = right.completedAtUnixMs !== null;
  if (leftCompleted !== rightCompleted) return leftCompleted ? 1 : -1;

  if (!leftCompleted) {
    const plannedComparison = compareNullableStrings(left.plannedDate, right.plannedDate);
    if (plannedComparison !== 0) return plannedComparison;

    const leftUndue = left.dueAtUnixMs === null;
    const rightUndue = right.dueAtUnixMs === null;
    if (leftUndue !== rightUndue) return leftUndue ? 1 : -1;
    if (!leftUndue) {
      const dueComparison = compareNumbers(left.dueAtUnixMs!, right.dueAtUnixMs!);
      if (dueComparison !== 0) return dueComparison;
    }

    const priorityComparison = compareNumbers(priorityRank(right.priority), priorityRank(left.priority));
    if (priorityComparison !== 0) return priorityComparison;
    const createdComparison = compareNumbers(right.createdAtUnixMs, left.createdAtUnixMs);
    if (createdComparison !== 0) return createdComparison;
  } else {
    const completedComparison = compareNumbers(right.completedAtUnixMs!, left.completedAtUnixMs!);
    if (completedComparison !== 0) return completedComparison;
  }

  const createdComparison = compareNumbers(right.createdAtUnixMs, left.createdAtUnixMs);
  return createdComparison !== 0 ? createdComparison : compareNumbers(right.id, left.id);
}

export const sortTasks = (tasks: Task[]): Task[] => [...tasks].sort(compareTasks);
export const listProjects = (includeArchived = false): Promise<Project[]> =>
  invoke<Project[]>('list_projects', { includeArchived });
export const createProject = (input: ProjectInput): Promise<Project> => invoke<Project>('create_project', { input });
export const updateProject = (id: number, input: ProjectInput): Promise<Project> =>
  invoke<Project>('update_project', { id, input });
export const archiveProject = (id: number): Promise<Project> => invoke<Project>('archive_project', { id });
export const restoreProject = (id: number): Promise<Project> => invoke<Project>('restore_project', { id });
export const listTasks = (): Promise<Task[]> => invoke<Task[]>('list_tasks');
export const listDeletedTasks = (): Promise<Task[]> => invoke<Task[]>('list_deleted_tasks');
export const createTask = (input: CreateTaskInput): Promise<TaskMutation> => invoke<TaskMutation>('create_task', { input });
export const updateTask = (id: number, input: UpdateTaskInput): Promise<TaskMutation> => invoke<TaskMutation>('update_task', { id, input });
export const setTaskPlannedDate = (id: number, plannedDate: string | null): Promise<Task> =>
  invoke<Task>('set_task_planned_date', { id, plannedDate });
export const setTaskCompleted = (id: number, completed: boolean): Promise<TaskCompletionMutation> =>
  invoke<TaskCompletionMutation>('set_task_completed', { id, completed });
export const snoozeTask = (id: number, untilUnixMs: number): Promise<TaskMutation> =>
  invoke<TaskMutation>('snooze_task', { id, untilUnixMs });
export const deferTaskToTomorrow = (id: number, timezone: string): Promise<TaskMutation> =>
  invoke<TaskMutation>('defer_task_to_tomorrow', { id, timezone });
export const deleteTask = (id: number): Promise<string | null> => invoke<string | null>('delete_task', { id });
export const restoreTask = (id: number): Promise<TaskMutation> => invoke<TaskMutation>('restore_task', { id });
export const permanentlyDeleteTask = (id: number): Promise<string | null> =>
  invoke<string | null>('permanently_delete_task', { id });
export const reconcileReminders = (): Promise<ReminderReport> => invoke<ReminderReport>('reconcile_reminders');
export const listReliabilityIncidents = (includeResolved = false): Promise<ReliabilityIncident[]> =>
  invoke<ReliabilityIncident[]>('list_reliability_incidents', { includeResolved });
export const acknowledgeReliabilityIncidents = (ids: number[]): Promise<void> =>
  invoke<void>('acknowledge_reliability_incidents', { ids });
export const claimPendingActivations = (consumerId: string): Promise<PendingActivation[]> =>
  invoke<PendingActivation[]>('claim_pending_activations', { consumerId });
export const acknowledgePendingActivations = (consumerId: string, ids: number[]): Promise<ActivationAckResult> =>
  invoke<ActivationAckResult>('acknowledge_pending_activations', { consumerId, ids });
export const claimPendingReminderWarnings = (listenerToken: number): Promise<ReminderWarningEvent[]> =>
  invoke<ReminderWarningEvent[]>('claim_pending_reminder_warnings', { listenerToken });
export const acknowledgeReminderWarnings = (listenerToken: number, ids: number[]): Promise<void> =>
  invoke<void>('acknowledge_reminder_warnings', { listenerToken, ids });
export const parseTaskDrafts = (text: string, todayLocal: string): Promise<TaskDraft[]> =>
  invoke<TaskDraft[]>('parse_task_drafts', { text, todayLocal });
