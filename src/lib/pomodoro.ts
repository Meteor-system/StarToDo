import { invoke } from '@tauri-apps/api/core';

/** Matches Rust `PomodoroPhase` serialized with `rename_all = "camelCase"`. */
export type PomodoroPhase = 'focus' | 'shortBreak' | 'longBreak';
/** Matches Rust `PomodoroStatus` serialized with `rename_all = "camelCase"`. */
export type PomodoroStatus = 'running' | 'paused' | 'completed' | 'skipped' | 'cancelled';

export interface PomodoroSettings {
  focusMinutes: number;
  shortBreakMinutes: number;
  longBreakMinutes: number;
  longBreakInterval: number;
  updatedAtUnixMs: number;
}

export interface UpdatePomodoroSettingsInput {
  focusMinutes: number;
  shortBreakMinutes: number;
  longBreakMinutes: number;
  longBreakInterval: number;
}

export interface StartPomodoroInput {
  phase: PomodoroPhase;
  taskId: number | null;
}

export interface PomodoroSession {
  id: number;
  taskId: number | null;
  taskTitleSnapshot: string | null;
  phase: PomodoroPhase;
  status: PomodoroStatus;
  plannedDurationSeconds: number;
  pausedRemainingSeconds: number | null;
  startedAtUnixMs: number | null;
  targetEndsAtUnixMs: number | null;
  pausedAtUnixMs: number | null;
  endedAtUnixMs: number | null;
  notificationTag: string | null;
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
}

export interface PomodoroTaskSummary {
  taskId: number;
  title: string;
  completedFocusCount: number;
  completedFocusTodayCount: number;
}

export interface PomodoroSnapshot {
  settings: PomodoroSettings;
  currentSession: PomodoroSession | null;
  completedFocusesInCycle: number;
  recommendedPhase: PomodoroPhase;
  completedFocusTodayCount: number;
}

export interface PomodoroView {
  snapshot: PomodoroSnapshot;
  taskSummaries: PomodoroTaskSummary[];
}

export const POMODORO_TASK_UNAVAILABLE_ERROR = 'task is not available for pomodoro binding';

export function reconcilePomodoroTaskSelection(
  selectedTaskId: number | null,
  taskSummaries: readonly PomodoroTaskSummary[]
): number | null {
  if (selectedTaskId === null || !Number.isSafeInteger(selectedTaskId) || selectedTaskId <= 0) return null;
  return taskSummaries.some((task) => task.taskId === selectedTaskId) ? selectedTaskId : null;
}

/**
 * Mirrors the Rust command result. A notification failure does not roll back the
 * SQLite-authoritative timer state; surface it to the shell as a warning instead.
 */
export interface PomodoroMutationResult {
  snapshot: PomodoroSnapshot;
  session: PomodoroSession | null;
  notificationTagToCancel: string | null;
  notificationWarning?: string | null;
}

/** Matches Rust `PendingPomodoroActivation` serialized with `rename_all = "camelCase"`. */
export interface PendingPomodoroActivation {
  id: number;
  sessionId: number;
  receivedAtUnixMs: number;
}

/** Matches Rust `PomodoroActivationAckResult` serialized with `rename_all = "camelCase"`. */
export interface PomodoroActivationAcknowledgement {
  acknowledgedIds: number[];
}

export type PomodoroPrimaryAction = 'start' | 'pause' | 'resume' | 'none';

const PHASE_LABELS: Record<PomodoroPhase, string> = {
  focus: '专注',
  shortBreak: '短休息',
  longBreak: '长休息'
};

const STATUS_LABELS: Record<PomodoroStatus, string> = {
  running: '进行中',
  paused: '已暂停',
  completed: '已完成',
  skipped: '已跳过',
  cancelled: '已重置'
};

export function pomodoroPhaseLabel(phase: PomodoroPhase): string {
  return PHASE_LABELS[phase] ?? '专注';
}

export function pomodoroStatusLabel(status: PomodoroStatus): string {
  return STATUS_LABELS[status] ?? '未知状态';
}

/**
 * Derives display time from the SQLite-authoritative target wall clock.
 * Refresh the input with `Date.now()`; never persist this derived value.
 */
export function remainingPomodoroSeconds(session: PomodoroSession | null, nowUnixMs = Date.now()): number {
  if (!session) return 0;
  if (session.status === 'paused') return Math.max(0, session.pausedRemainingSeconds ?? 0);
  if (session.status !== 'running' || session.targetEndsAtUnixMs === null) return 0;
  return Math.max(0, Math.ceil((session.targetEndsAtUnixMs - nowUnixMs) / 1_000));
}

/**
 * Returns a bounded, display-only completion ratio. The session snapshot stays
 * authoritative: this helper never advances or stores timer state.
 */
export function pomodoroProgress(session: PomodoroSession | null, nowUnixMs = Date.now()): number {
  if (!session || session.plannedDurationSeconds <= 0) return 0;
  const remaining = remainingPomodoroSeconds(session, nowUnixMs);
  return Math.min(1, Math.max(0, 1 - remaining / session.plannedDurationSeconds));
}

export function pomodoroPhaseDurationSeconds(settings: PomodoroSettings, phase: PomodoroPhase): number {
  switch (phase) {
    case 'focus':
      return settings.focusMinutes * 60;
    case 'shortBreak':
      return settings.shortBreakMinutes * 60;
    case 'longBreak':
      return settings.longBreakMinutes * 60;
  }
}

export function formatPomodoroDuration(totalSeconds: number): string {
  const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

export function pomodoroPrimaryAction(snapshot: PomodoroSnapshot | null): PomodoroPrimaryAction {
  const status = snapshot?.currentSession?.status;
  if (status === 'running') return 'pause';
  if (status === 'paused') return 'resume';
  return snapshot ? 'start' : 'none';
}

export const getPomodoro = (): Promise<PomodoroSnapshot> => invoke<PomodoroSnapshot>('get_pomodoro');
export const getPomodoroView = (): Promise<PomodoroView> => invoke<PomodoroView>('get_pomodoro_view');
export const updatePomodoroSettings = (input: UpdatePomodoroSettingsInput): Promise<PomodoroMutationResult> =>
  invoke<PomodoroMutationResult>('update_pomodoro_settings', { input });
export const startPomodoro = (input: StartPomodoroInput): Promise<PomodoroMutationResult> =>
  invoke<PomodoroMutationResult>('start_pomodoro', { input });
export const pausePomodoro = (): Promise<PomodoroMutationResult> => invoke<PomodoroMutationResult>('pause_pomodoro');
export const resumePomodoro = (): Promise<PomodoroMutationResult> => invoke<PomodoroMutationResult>('resume_pomodoro');
export const skipPomodoro = (): Promise<PomodoroMutationResult> => invoke<PomodoroMutationResult>('skip_pomodoro');
export const resetPomodoro = (): Promise<PomodoroMutationResult> => invoke<PomodoroMutationResult>('reset_pomodoro');
export const listPomodoroTaskSummaries = (): Promise<PomodoroTaskSummary[]> =>
  invoke<PomodoroTaskSummary[]>('list_pomodoro_task_summaries');
export const claimPendingPomodoroActivations = (consumerId: string): Promise<PendingPomodoroActivation[]> =>
  invoke<PendingPomodoroActivation[]>('claim_pending_pomodoro_activations', { consumerId });
export const acknowledgePendingPomodoroActivations = (
  consumerId: string,
  ids: number[]
): Promise<PomodoroActivationAcknowledgement> =>
  invoke<PomodoroActivationAcknowledgement>('acknowledge_pending_pomodoro_activations', { consumerId, ids });
