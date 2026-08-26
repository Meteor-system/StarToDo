import { describe, expect, it } from 'vitest';

import {
  ownsTaskDetailsOperation,
  shouldToggleTaskFromKeyboard
} from './task-interaction';

describe('task row keyboard completion', () => {
  it('accepts an unmodified Space on the row itself', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        false,
        false,
        false,
        true
      )
    ).toBe(true);
  });

  it('rejects modified Space', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        true,
        false,
        false,
        true
      )
    ).toBe(false);
  });

  it('rejects events from nested controls', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        false,
        false,
        false,
        false
      )
    ).toBe(false);
  });

  it('rejects Enter', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        'Enter',
        false,
        false,
        false,
        true
      )
    ).toBe(false);
  });
});

describe('task details operation ownership', () => {
  it('owns an operation when task, draft generation, and operation match', () => {
    expect(
      ownsTaskDetailsOperation(7, 3, 11, 7, 3, 11)
    ).toBe(true);
  });

  it('rejects an operation after switching tasks', () => {
    expect(
      ownsTaskDetailsOperation(8, 3, 11, 7, 3, 11)
    ).toBe(false);
  });

  it('rejects an operation after reopening the same task with a newer draft generation', () => {
    expect(
      ownsTaskDetailsOperation(7, 4, 11, 7, 3, 11)
    ).toBe(false);
  });
});
