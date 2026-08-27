import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  addLocalCalendarDays,
  compareTasks,
  epochMsToLocalDateTime,
  errorMessage,
  isValidIanaTimeZone,
  isValidPlannedDate,
  localDateForEpochMs,
  localDateTimeToEpochMs,
  localTimeZone,
  priorityLabel,
  recurrenceLabel,
  sortTasks,
  todayLocalDate,
  validateTaskInput,
  weekEndLocalDate,
  weekStartLocalDate,
  type Priority,
  type RecurrenceKind,
  type Task,
  type TaskInput
} from './tasks';

const MAX_JAVASCRIPT_DATE_UNIX_MS = 8_640_000_000_000_000;

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 1,
    title: 'Test task',
    notes: '',
    plannedDate: '2024-02-15',
    dueAtUnixMs: 1_000,
    reminderAtUnixMs: null,
    reminderFiredAtUnixMs: null,
    deletedAtUnixMs: null,
    projectId: null,
    priority: 'none',
    recurrenceKind: 'none',
    recurrenceSeriesId: null,
    recurrenceSourceTaskId: null,
    recurrenceTimezone: null,
    recurrenceDstPolicy: null,
    completedAtUnixMs: null,
    createdAtUnixMs: 100,
    updatedAtUnixMs: 100,
    ...overrides
  };
}

function taskInput(overrides: Partial<TaskInput> = {}): TaskInput {
  return {
    title: 'Valid task',
    notes: '',
    plannedDate: '2024-02-15',
    dueAtUnixMs: null,
    reminderAtUnixMs: null,
    projectId: null,
    priority: 'none',
    recurrenceKind: 'none',
    recurrenceTimezone: null,
    ...overrides
  };
}

const ids = (tasks: Task[]) => sortTasks(tasks).map((item) => item.id);

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('compareTasks', () => {
  // Keep this fixture order aligned with Rust tasks::tests::list_uses_the_required_fixed_sort_order.
  it('uses the required fixed sort order for active and completed tasks', () => {
    const tasks = [
      task({ id: 1, completedAtUnixMs: 1_000 }),
      task({ id: 2, plannedDate: null, dueAtUnixMs: null }),
      task({ id: 3, plannedDate: '2024-02-16' }),
      task({ id: 4, plannedDate: '2024-02-15', dueAtUnixMs: null }),
      task({ id: 5, priority: 'high' }),
      task({ id: 6, priority: 'low' }),
      task({ id: 7, completedAtUnixMs: 2_000 })
    ];

    expect(ids(tasks)).toEqual([5, 6, 4, 3, 2, 7, 1]);
  });

  it('puts incomplete tasks before completed tasks', () => {
    expect(compareTasks(task({ completedAtUnixMs: null }), task({ completedAtUnixMs: 1 }))).toBeLessThan(0);
  });

  it('sorts active tasks by planned date ascending with null last', () => {
    expect(ids([
      task({ id: 1, plannedDate: null }),
      task({ id: 2, plannedDate: '2024-02-16' }),
      task({ id: 3, plannedDate: '2024-02-15' })
    ])).toEqual([3, 2, 1]);
  });

  it('sorts equal planned dates by due time ascending with null last', () => {
    expect(ids([
      task({ id: 1, dueAtUnixMs: null }),
      task({ id: 2, dueAtUnixMs: 200 }),
      task({ id: 3, dueAtUnixMs: 100 })
    ])).toEqual([3, 2, 1]);
  });

  it('sorts equal active dates and due times by priority descending', () => {
    expect(ids([
      task({ id: 1, priority: 'low' }),
      task({ id: 2, priority: 'high' }),
      task({ id: 3, priority: 'medium' })
    ])).toEqual([2, 3, 1]);
  });

  it('sorts active ties by creation time descending then id descending', () => {
    expect(ids([
      task({ id: 1, createdAtUnixMs: 100 }),
      task({ id: 2, createdAtUnixMs: 200 }),
      task({ id: 3, createdAtUnixMs: 100 })
    ])).toEqual([2, 3, 1]);
  });

  it('sorts completed tasks by completion time descending then creation time and id descending', () => {
    expect(ids([
      task({ id: 1, completedAtUnixMs: 100, createdAtUnixMs: 100 }),
      task({ id: 2, completedAtUnixMs: 200, createdAtUnixMs: 1 }),
      task({ id: 3, completedAtUnixMs: 100, createdAtUnixMs: 200 }),
      task({ id: 4, completedAtUnixMs: 100, createdAtUnixMs: 100 })
    ])).toEqual([2, 3, 4, 1]);
  });
});

describe('date helpers', () => {
  it('validates planned-date format, calendar dates, and leap days', () => {
    expect(isValidPlannedDate('2024-02-29')).toBe(true);
    expect(isValidPlannedDate('2023-02-29')).toBe(false);
    expect(isValidPlannedDate('2024-13-01')).toBe(false);
    expect(isValidPlannedDate('2024-2-01')).toBe(false);
    expect(isValidPlannedDate('not-a-date')).toBe(false);
  });

  it('returns today from the fake local system clock', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2024, 6, 9, 12, 0, 0));

    expect(todayLocalDate()).toBe('2024-07-09');
  });

  it('formats valid non-negative epoch values as local dates', () => {
    expect(localDateForEpochMs(new Date(2024, 0, 2, 12, 0, 0).getTime())).toBe('2024-01-02');
    expect(localDateForEpochMs(-1)).toBeNull();
    expect(localDateForEpochMs(8_640_000_000_000_001)).toBeNull();
    expect(localDateForEpochMs(Number.MAX_SAFE_INTEGER + 1)).toBeNull();
  });

  it('adds calendar days across month, leap-year, year, and negative boundaries', () => {
    expect(addLocalCalendarDays('2024-01-31', 1)).toBe('2024-02-01');
    expect(addLocalCalendarDays('2024-02-28', 1)).toBe('2024-02-29');
    expect(addLocalCalendarDays('2023-12-31', 1)).toBe('2024-01-01');
    expect(addLocalCalendarDays('2024-03-01', -1)).toBe('2024-02-29');
    expect(addLocalCalendarDays('2024-02-30', 1)).toBeNull();
    expect(addLocalCalendarDays('2024-02-01', 1.5)).toBeNull();
  });

  it('finds Monday week starts and Sunday week ends across boundaries', () => {
    expect(weekStartLocalDate('2024-01-01')).toBe('2024-01-01');
    expect(weekEndLocalDate('2024-01-01')).toBe('2024-01-07');
    expect(weekStartLocalDate('2024-01-07')).toBe('2024-01-01');
    expect(weekEndLocalDate('2024-01-07')).toBe('2024-01-07');
    expect(weekStartLocalDate('2024-01-02')).toBe('2024-01-01');
    expect(weekEndLocalDate('2023-12-31')).toBe('2023-12-31');
  });

  it('falls back to the fake current local date for invalid week inputs', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2024, 4, 20, 12, 0, 0));

    expect(weekStartLocalDate('invalid')).toBe('2024-05-20');
    expect(weekEndLocalDate('2024-02-30')).toBe('2024-05-20');
  });
});

describe('local date-time conversion', () => {
  it('round-trips a normal, minute-aligned local time without DST assumptions', () => {
    const input = '2024-02-15T12:34';
    const epoch = localDateTimeToEpochMs(input);

    expect(epoch).not.toBeNull();
    expect(epochMsToLocalDateTime(epoch)).toBe(input);
  });

  it('rejects empty and invalid local date-times and formats invalid epochs as empty strings', () => {
    expect(localDateTimeToEpochMs('')).toBeNull();
    expect(localDateTimeToEpochMs('2024-02-30T12:00')).toBeNull();
    expect(localDateTimeToEpochMs('2024-02-15T24:00')).toBeNull();
    expect(localDateTimeToEpochMs('invalid')).toBeNull();
    expect(epochMsToLocalDateTime(null)).toBe('');
    expect(epochMsToLocalDateTime(-1)).toBe('');
    expect(epochMsToLocalDateTime(Number.MAX_SAFE_INTEGER + 1)).toBe('');
  });
});

describe('validateTaskInput', () => {
  it('validates trimmed title boundaries using Unicode code points', () => {
    expect(validateTaskInput(taskInput({ title: '   ' })).title).toBe('标题需为 1 到 200 个字符。');
    expect(validateTaskInput(taskInput({ title: '中'.repeat(200) })).title).toBeUndefined();
    expect(validateTaskInput(taskInput({ title: '中'.repeat(201) })).title).toBe('标题需为 1 到 200 个字符。');
  });

  it('validates notes by Unicode code point length', () => {
    expect(validateTaskInput(taskInput({ notes: '😀'.repeat(10_000) })).notes).toBeUndefined();
    expect(validateTaskInput(taskInput({ notes: '😀'.repeat(10_001) })).notes).toBe('备注最多可包含 10,000 个字符。');
  });

  it('validates dates, due times, and reminder times including JavaScript date boundaries', () => {
    expect(validateTaskInput(taskInput({ plannedDate: '2024-02-30' })).plannedDate).toBe('计划日期无效。');
    expect(validateTaskInput(taskInput({ dueAtUnixMs: -1 })).dueAtUnixMs).toBe('截止时间无效。');
    expect(validateTaskInput(taskInput({ dueAtUnixMs: 1.5 })).dueAtUnixMs).toBe('截止时间无效。');
    expect(validateTaskInput(taskInput({ dueAtUnixMs: MAX_JAVASCRIPT_DATE_UNIX_MS })).dueAtUnixMs).toBeUndefined();
    expect(validateTaskInput(taskInput({ dueAtUnixMs: MAX_JAVASCRIPT_DATE_UNIX_MS + 1 })).dueAtUnixMs).toBe('截止时间无效。');
    expect(validateTaskInput(taskInput({ reminderAtUnixMs: -1 })).reminderAtUnixMs).toBe('提醒时间无效。');
    expect(validateTaskInput(taskInput({ reminderAtUnixMs: 1.5 })).reminderAtUnixMs).toBe('提醒时间无效。');
    expect(validateTaskInput(taskInput({ reminderAtUnixMs: MAX_JAVASCRIPT_DATE_UNIX_MS })).reminderAtUnixMs).toBeUndefined();
    expect(validateTaskInput(taskInput({ reminderAtUnixMs: MAX_JAVASCRIPT_DATE_UNIX_MS + 1 })).reminderAtUnixMs).toBe('提醒时间无效。');
  });

  it('rejects runtime-invalid priority and recurrence values', () => {
    expect(validateTaskInput(taskInput({ priority: 'urgent' as Priority })).priority).toBe('优先级无效。');
    expect(validateTaskInput(taskInput({ recurrenceKind: 'monthly' as RecurrenceKind })).recurrenceKind).toBe('重复规则无效。');
  });

  it('requires a valid planned date and IANA timezone for repeated tasks', () => {
    expect(validateTaskInput(taskInput({ recurrenceKind: 'daily', plannedDate: null, recurrenceTimezone: null }))).toMatchObject({
      plannedDate: '重复任务需要有效的计划日期。',
      recurrenceTimezone: '无法使用当前时区创建重复任务。'
    });
    expect(validateTaskInput(taskInput({ recurrenceKind: 'weekly', plannedDate: '2024-02-30', recurrenceTimezone: 'Not/AZone' }))).toMatchObject({
      plannedDate: '重复任务需要有效的计划日期。',
      recurrenceTimezone: '无法使用当前时区创建重复任务。'
    });
    expect(validateTaskInput(taskInput({ recurrenceKind: 'daily', recurrenceTimezone: 'UTC' }))).toEqual({});
    expect(validateTaskInput(taskInput({ recurrenceKind: 'weekly', recurrenceTimezone: 'America/New_York' }))).toEqual({});
  });
});

describe('labels, errors, and timezone helpers', () => {
  it('returns labels and safe fallbacks for known and unknown values', () => {
    expect(priorityLabel('high')).toBe('高');
    expect(priorityLabel('unknown' as Priority)).toBe('无优先级');
    expect(recurrenceLabel('weekly')).toBe('每周');
    expect(recurrenceLabel('unknown' as RecurrenceKind)).toBe('不重复');
  });

  it('formats Error and non-Error values', () => {
    expect(errorMessage(new Error('failed'))).toBe('failed');
    expect(errorMessage(42)).toBe('42');
  });

  it('gets a resolved timezone and falls back for empty or throwing Intl implementations', () => {
    vi.spyOn(Intl, 'DateTimeFormat').mockReturnValue({
      resolvedOptions: () => ({ timeZone: 'Asia/Shanghai' })
    } as Intl.DateTimeFormat);
    expect(localTimeZone()).toBe('Asia/Shanghai');

    vi.restoreAllMocks();
    vi.spyOn(Intl, 'DateTimeFormat').mockReturnValue({
      resolvedOptions: () => ({ timeZone: '' })
    } as Intl.DateTimeFormat);
    expect(localTimeZone()).toBe('UTC');

    vi.restoreAllMocks();
    vi.spyOn(Intl, 'DateTimeFormat').mockImplementation(() => {
      throw new Error('Intl unavailable');
    });
    expect(localTimeZone()).toBe('UTC');
  });

  it('accepts valid IANA timezone names and rejects invalid names', () => {
    expect(isValidIanaTimeZone('UTC')).toBe(true);
    expect(isValidIanaTimeZone('America/New_York')).toBe(true);
    expect(isValidIanaTimeZone('Not/AZone')).toBe(false);
  });
});
