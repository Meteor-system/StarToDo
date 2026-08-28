import { addLocalCalendarDays } from './tasks';

export type PlannerSelection =
  | 'unscheduled'
  | string;

function inDisplayedWeek(
  date: string,
  weekStart: string
): boolean {
  const weekEnd =
    addLocalCalendarDays(weekStart, 6) ?? weekStart;
  return date >= weekStart && date <= weekEnd;
}

export function initialPlannerSelection(
  weekStart: string,
  today: string
): PlannerSelection {
  return inDisplayedWeek(today, weekStart)
    ? today
    : weekStart;
}

export function normalizePlannerSelection(
  selection: PlannerSelection,
  weekStart: string
): PlannerSelection {
  if (selection === 'unscheduled') {
    return selection;
  }

  return inDisplayedWeek(selection, weekStart)
    ? selection
    : weekStart;
}
