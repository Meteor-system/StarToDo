import { describe, expect, it } from 'vitest';

import {
  initialPlannerSelection,
  normalizePlannerSelection
} from './planner-selection';

describe('planner selection', () => {
  it('selects today when it is inside the displayed week', () => {
    expect(
      initialPlannerSelection(
        '2026-08-24',
        '2026-08-27'
      )
    ).toBe('2026-08-27');
  });

  it('selects week start when today is outside the week', () => {
    expect(
      initialPlannerSelection(
        '2026-08-24',
        '2026-09-03'
      )
    ).toBe('2026-08-24');
  });

  it('keeps the unscheduled selection across week changes', () => {
    expect(
      normalizePlannerSelection(
        'unscheduled',
        '2026-08-31'
      )
    ).toBe('unscheduled');
  });

  it('resets a stale date after a week change', () => {
    expect(
      normalizePlannerSelection(
        '2026-08-27',
        '2026-08-31'
      )
    ).toBe('2026-08-31');
  });
});
