import { describe, expect, it } from 'vitest';

import {
  formatPomodoroDuration,
  pomodoroPhaseLabel,
  pomodoroPhaseDurationSeconds,
  pomodoroPrimaryAction,
  pomodoroProgress,
  pomodoroStatusLabel,
  reconcilePomodoroTaskSelection,
  remainingPomodoroSeconds,
  type PomodoroSession,
  type PomodoroSnapshot,
  type PomodoroTaskSummary
} from './pomodoro';

function session(overrides: Partial<PomodoroSession> = {}): PomodoroSession {
  return {
    id: 1,
    taskId: null,
    taskTitleSnapshot: null,
    phase: 'focus',
    status: 'running',
    plannedDurationSeconds: 1_500,
    pausedRemainingSeconds: null,
    startedAtUnixMs: 0,
    targetEndsAtUnixMs: 1_000,
    pausedAtUnixMs: null,
    endedAtUnixMs: null,
    notificationTag: null,
    createdAtUnixMs: 0,
    updatedAtUnixMs: 0,
    ...overrides
  };
}

function snapshot(currentSession: PomodoroSession | null): PomodoroSnapshot {
  return {
    settings: { focusMinutes: 25, shortBreakMinutes: 5, longBreakMinutes: 15, longBreakInterval: 4, updatedAtUnixMs: 0 },
    currentSession,
    completedFocusesInCycle: 0,
    recommendedPhase: 'focus',
    completedFocusTodayCount: 0
  };
}

function taskSummary(taskId: number): PomodoroTaskSummary {
  return {
    taskId,
    title: `Task ${taskId}`,
    completedFocusCount: 0,
    completedFocusTodayCount: 0
  };
}

describe('reconcilePomodoroTaskSelection', () => {
  const tasks = [taskSummary(3), taskSummary(7)];

  it('keeps a selected task only while it remains bindable', () => {
    expect(reconcilePomodoroTaskSelection(7, tasks)).toBe(7);
    expect(reconcilePomodoroTaskSelection(5, tasks)).toBeNull();
    expect(reconcilePomodoroTaskSelection(null, tasks)).toBeNull();
  });

  it('rejects invalid task ids before they reach the desktop command', () => {
    expect(reconcilePomodoroTaskSelection(0, tasks)).toBeNull();
    expect(reconcilePomodoroTaskSelection(Number.NaN, tasks)).toBeNull();
    expect(reconcilePomodoroTaskSelection(Number.MAX_SAFE_INTEGER + 1, tasks)).toBeNull();
  });
});

describe('remainingPomodoroSeconds', () => {
  it('derives running time from the backend target clock and rounds partial seconds up', () => {
    expect(remainingPomodoroSeconds(session({ targetEndsAtUnixMs: 10_000 }), 9_001)).toBe(1);
    expect(remainingPomodoroSeconds(session({ targetEndsAtUnixMs: 10_000 }), 7_001)).toBe(3);
  });

  it('never reports negative time and freezes paused backend remaining time', () => {
    expect(remainingPomodoroSeconds(session({ targetEndsAtUnixMs: 999 }), 1_000)).toBe(0);
    expect(remainingPomodoroSeconds(session({ status: 'paused', pausedRemainingSeconds: 317, targetEndsAtUnixMs: null }), 99_999)).toBe(317);
    expect(remainingPomodoroSeconds(null, 0)).toBe(0);
  });
});

describe('pomodoro presentation helpers', () => {
  it('formats resilient mm:ss values', () => {
    expect(formatPomodoroDuration(0)).toBe('00:00');
    expect(formatPomodoroDuration(65.9)).toBe('01:05');
    expect(formatPomodoroDuration(3_723)).toBe('62:03');
    expect(formatPomodoroDuration(Number.NaN)).toBe('00:00');
  });

  it('uses Chinese labels and derives the single available main action', () => {
    expect(pomodoroPhaseLabel('shortBreak')).toBe('短休息');
    expect(pomodoroStatusLabel('paused')).toBe('已暂停');
    expect(pomodoroPrimaryAction(snapshot(session({ status: 'running' })))).toBe('pause');
    expect(pomodoroPrimaryAction(snapshot(session({ status: 'paused' })))).toBe('resume');
    expect(pomodoroPrimaryAction(snapshot(null))).toBe('start');
    expect(pomodoroPrimaryAction(null)).toBe('none');
  });

  it('derives bounded progress from the same server snapshot clock', () => {
    const running = session({ plannedDurationSeconds: 100, targetEndsAtUnixMs: 100_000 });

    expect(pomodoroProgress(running, 25_000)).toBe(0.25);
    expect(pomodoroProgress(running, 100_001)).toBe(1);
    expect(pomodoroProgress(session({ status: 'paused', plannedDurationSeconds: 100, pausedRemainingSeconds: 25 }), 999_999)).toBe(0.75);
    expect(pomodoroProgress(null)).toBe(0);
  });

  it('uses settings only to present a not-yet-started phase duration', () => {
    const settings = snapshot(null).settings;

    expect(pomodoroPhaseDurationSeconds(settings, 'focus')).toBe(1_500);
    expect(pomodoroPhaseDurationSeconds(settings, 'shortBreak')).toBe(300);
    expect(pomodoroPhaseDurationSeconds(settings, 'longBreak')).toBe(900);
  });

  it('keeps optional command warnings outside snapshot state', () => {
    const result = {
      snapshot: snapshot(null),
      session: null,
      notificationTagToCancel: null,
      notificationWarning: 'Windows 通知暂时不可用。'
    };

    expect(result.snapshot.recommendedPhase).toBe('focus');
    expect(result.notificationWarning).toContain('通知');
  });
});
