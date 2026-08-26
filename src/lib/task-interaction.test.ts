import { describe, expect, it } from 'vitest';

import { shouldToggleTaskFromKeyboard } from './task-interaction';

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
