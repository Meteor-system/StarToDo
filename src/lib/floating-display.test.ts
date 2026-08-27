import { describe, expect, it } from 'vitest';

import {
  FLOATING_INTERACTION_MS,
  FLOATING_LEAVE_BUFFER_MS,
  createFloatingDisplayState,
  floatingFocusTargetSelector,
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

  it('explicit activation expands without inventing pointer or focus ownership', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'activate', at: 100 });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.pointerInside).toBe(false);
    expect(state.focusInside).toBe(false);
    expect(state.lastInteractionAt).toBe(100);
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

  it('keeps truthful ownership when activation follows pointer expansion', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'pointer-enter', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'activate', at: 150 });
    state = reduceFloatingDisplay(state, { type: 'pointer-leave', at: 200 });

    expect(state.pointerInside).toBe(false);
    expect(state.focusInside).toBe(false);
    expect(state.collapseAt).toBe(150 + FLOATING_INTERACTION_MS);
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

  it('collapses after keyboard-only focus leaves and its deadline expires', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'focus-in', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'focus-out', at: 200 });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.collapseAt).toBe(100 + FLOATING_INTERACTION_MS);

    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100 + FLOATING_INTERACTION_MS
    });
    expect(state.mode).toBe('capsule');
  });

  it('derives interaction state when always-expanded is disabled', () => {
    let state = createFloatingDisplayState(true, true);
    state = reduceFloatingDisplay(state, { type: 'pointer-enter', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'always-expanded', value: false, at: 200 });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.alwaysExpanded).toBe(false);
    expect(state.collapseAt).toBeNull();
  });

  it('updates focus snapshot while pointer remains inside and clears stale deadline', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, { type: 'pointer-enter', at: 100 });
    state = reduceFloatingDisplay(state, { type: 'pointer-leave', at: 200 });
    state = reduceFloatingDisplay(state, { type: 'snapshot', focusActive: false, at: 300 });

    expect(state.mode).toBe('expanded');
    expect(state.pointerInside).toBe(false);
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

  it('maps capsule focus targets to stable expanded controls', () => {
    expect(floatingFocusTargetSelector('open-focus')).toBe('[data-floating-focus-target="open-focus"]');
    expect(floatingFocusTargetSelector('primary')).toBe('[data-floating-focus-target="primary"]');
    expect(floatingFocusTargetSelector('expand')).toBe('[data-floating-focus-target="expand"]');
  });

  it('maps interaction-expanded to the expanded outer size', () => {
    expect(floatingSizeMode('capsule')).toBe('capsule');
    expect(floatingSizeMode('expanded')).toBe('expanded');
    expect(floatingSizeMode('interaction-expanded')).toBe('expanded');
  });
});
