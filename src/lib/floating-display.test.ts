import { describe, expect, it } from 'vitest';

import {
  FLOATING_INTERACTION_MS,
  FLOATING_LEAVE_BUFFER_MS,
  createFloatingDisplayState,
  floatingSizeMode,
  reduceFloatingDisplay
} from './floating-display';

describe('floating display reducer', () => {
  it('uses expanded while idle and capsule while focus is active', () => {
    const idle = createFloatingDisplayState(false, false);
    expect(idle.mode).toBe('expanded');

    const active = reduceFloatingDisplay(idle, {
      type: 'snapshot',
      focusActive: true,
      at: 100
    });
    expect(active.mode).toBe('capsule');
  });

  it('keeps interaction expansion for five seconds', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'pointer-enter', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'pointer-leave', at: 200 });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.collapseAt).toBe(100 + FLOATING_INTERACTION_MS);

    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100 + FLOATING_INTERACTION_MS - 1
    });
    expect(state.mode).toBe('interaction-expanded');

    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100 + FLOATING_INTERACTION_MS
    });
    expect(state.mode).toBe('capsule');
  });

  it('adds a leave buffer after a long pointer stay', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'pointer-enter', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'pointer-leave', at: 6_000 });

    expect(state.collapseAt).toBe(6_000 + FLOATING_LEAVE_BUFFER_MS);
  });

  it('does not collapse while keyboard focus remains inside', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'focus-in', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'pointer-leave', at: 200 });
    state = reduceFloatingDisplay(state, { type: 'timeout', at: 100_000 });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.collapseAt).toBeNull();
  });

  it('keeps always-expanded mode independent of focus state', () => {
    let state = createFloatingDisplayState(true, true);
    expect(state.mode).toBe('expanded');

    state = reduceFloatingDisplay(state, {
      type: 'snapshot',
      focusActive: true,
      at: 100
    });
    expect(state.mode).toBe('expanded');
  });

  it('maps interaction-expanded to the expanded outer size', () => {
    expect(floatingSizeMode('capsule')).toBe('capsule');
    expect(floatingSizeMode('expanded')).toBe('expanded');
    expect(floatingSizeMode('interaction-expanded')).toBe('expanded');
  });
});
