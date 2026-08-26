import { describe, expect, it } from 'vitest';

import {
  acceptFloatingPreferencesRead,
  acceptFloatingReconciliation,
  acceptFloatingSizeFailure,
  abandonFloatingResetReconciliation,
  acceptFloatingResetFailure,
  acceptFloatingResetReconciliation,
  acceptFloatingResetSuccess,
  acceptFloatingSizeSuccess,
  beginFloatingResetRequest,
  beginFloatingSizeRequest,
  createFloatingSizeSyncState,
  failFloatingPreferencesRead,
  nextFloatingResetRequest,
  nextFloatingSizeRequest,
  requestFloatingReset,
  requestFloatingSize,
  retryFloatingSize,
  type FloatingSizeSyncState
} from './floating-size-sync';

interface Preferences {
  displayMode: 'capsule' | 'expanded';
  userResized: boolean;
  width: number;
}

const prefs = (displayMode: 'capsule' | 'expanded', width: number, userResized = false): Preferences => ({
  displayMode,
  userResized,
  width
});

function state(): FloatingSizeSyncState<Preferences> {
  return createFloatingSizeSyncState<Preferences>();
}

describe('floating size sync coordination', () => {
  it('gates requests until the initial preference read settles', () => {
    let sync = requestFloatingSize(state(), 'expanded');
    expect(nextFloatingSizeRequest(sync)).toBeNull();

    sync = acceptFloatingPreferencesRead(sync, prefs('capsule', 340));
    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'expanded' });
  });

  it('drains after an initial preference read failure', () => {
    let sync = requestFloatingSize(state(), 'expanded');
    sync = failFloatingPreferencesRead(sync);
    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'expanded' });
  });

  it('does not spin after failure and retries the same mode on a later explicit event', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const request = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, request);
    sync = acceptFloatingSizeFailure(sync, request);

    expect(nextFloatingSizeRequest(sync)).toBeNull();
    sync = retryFloatingSize(sync);
    const retry = nextFloatingSizeRequest(sync)!;
    expect(retry.mode).toBe('expanded');
    expect(retry.generation).toBeGreaterThan(request.generation);
  });

  it('does not retry a superseded failed request', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const expanded = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, expanded);
    sync = requestFloatingSize(sync, 'capsule');
    sync = acceptFloatingSizeFailure(sync, expanded);

    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'capsule' });
    expect(sync.preferences?.displayMode).toBe('capsule');
  });

  it('rejects an ABA response from the first expanded generation', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const firstExpanded = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, firstExpanded);
    sync = requestFloatingSize(sync, 'capsule');
    sync = requestFloatingSize(sync, 'expanded');

    sync = acceptFloatingSizeSuccess(sync, firstExpanded, prefs('expanded', 380, true));

    expect(sync.preferences?.width).toBe(340);
    const latest = nextFloatingSizeRequest(sync)!;
    expect(latest.mode).toBe('expanded');
    expect(latest.generation).toBeGreaterThan(firstExpanded.generation);
  });

  it('reconciles ambiguous failure preferences without marking the failed request applied', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const request = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, request);
    sync = acceptFloatingSizeFailure(sync, request);
    sync = acceptFloatingReconciliation(sync, request, prefs('expanded', 380, true));

    expect(sync.preferences).toEqual(prefs('expanded', 380, true));
    expect(nextFloatingSizeRequest(sync)).toBeNull();
    sync = retryFloatingSize(sync);
    const retry = nextFloatingSizeRequest(sync)!;
    expect(retry.mode).toBe('expanded');
    expect(retry.generation).toBeGreaterThan(request.generation);

    sync = acceptFloatingReconciliation(sync, request, prefs('expanded', 420, false));
    expect(sync.preferences).toEqual(prefs('expanded', 380, true));
    expect(nextFloatingSizeRequest(sync)).toEqual(retry);
  });

  it('rejects reconciliation preferences from an obsolete generation', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const obsolete = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, obsolete);
    sync = requestFloatingSize(sync, 'capsule');
    sync = acceptFloatingSizeFailure(sync, obsolete);
    sync = acceptFloatingReconciliation(sync, obsolete, prefs('expanded', 380, true));

    expect(sync.preferences).toEqual(prefs('capsule', 340));
    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'capsule' });
  });

  it('serializes reset behind an older set and rejects the older set preferences', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingSize(sync, 'expanded');
    const setRequest = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, setRequest);
    sync = requestFloatingReset(sync);
    expect(nextFloatingResetRequest(sync)).toBeNull();
    sync = acceptFloatingSizeSuccess(sync, setRequest, prefs('expanded', 380, true));
    const resetRequest = nextFloatingResetRequest(sync)!;
    expect(sync.preferences).toEqual(prefs('capsule', 340, true));

    sync = beginFloatingResetRequest(sync, resetRequest);
    sync = acceptFloatingResetSuccess(sync, resetRequest, prefs('expanded', 380, false));
    expect(sync.preferences).toEqual(prefs('expanded', 380, false));
    expect(sync.desired).toEqual({ mode: 'expanded', generation: resetRequest.generation });
  });

  it('rejects an older set reconciliation after reset intent exists', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingSize(sync, 'expanded');
    const setRequest = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, setRequest);
    sync = acceptFloatingSizeFailure(sync, setRequest);
    sync = requestFloatingReset(sync);

    sync = acceptFloatingReconciliation(sync, setRequest, prefs('expanded', 380, true));

    expect(sync.preferences).toEqual(prefs('capsule', 340, true));
  });

  it('rebases ambiguous reset reconciliation before issuing one current presentation retry', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingSize(sync, 'expanded');
    const oldSet = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, oldSet);
    sync = acceptFloatingSizeFailure(sync, oldSet);
    sync = requestFloatingReset(sync);
    const reset = nextFloatingResetRequest(sync)!;
    sync = beginFloatingResetRequest(sync, reset);
    sync = acceptFloatingResetFailure(sync, reset);
    sync = acceptFloatingResetReconciliation(sync, reset, prefs('expanded', 380, false));

    const presentation = nextFloatingSizeRequest(sync)!;
    expect(presentation).toEqual({ mode: 'expanded', generation: reset.generation });
    sync = beginFloatingSizeRequest(sync, presentation);
    sync = acceptFloatingSizeFailure(sync, presentation);
    expect(nextFloatingSizeRequest(sync)).toBeNull();
  });

  it('rebases failed reset reconciliation to the current UI mode without applied success', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingReset(sync);
    const reset = nextFloatingResetRequest(sync)!;
    sync = beginFloatingResetRequest(sync, reset);
    sync = acceptFloatingResetFailure(sync, reset);
    sync = abandonFloatingResetReconciliation(sync, reset, 'expanded');

    expect(sync.desired).toEqual({ mode: 'expanded', generation: reset.generation });
    expect(sync.appliedGeneration).toBeLessThan(reset.generation);
    expect(nextFloatingSizeRequest(sync)).toEqual(sync.desired);
  });

  it('never emits an obsolete desired generation', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingSize(sync, 'expanded');
    const oldSet = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, oldSet);
    sync = acceptFloatingSizeFailure(sync, oldSet);
    sync = requestFloatingReset(sync);
    sync = { ...sync, resetDesired: null };

    expect(nextFloatingSizeRequest(sync)).toBeNull();
  });

  it('keeps a newer reset pending when an obsolete reset fails', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('expanded', 380, true));
    sync = requestFloatingReset(sync);
    const firstReset = nextFloatingResetRequest(sync)!;
    sync = beginFloatingResetRequest(sync, firstReset);
    sync = requestFloatingReset(sync);
    const newerReset = sync.resetDesired!;

    sync = acceptFloatingResetFailure(sync, firstReset);

    expect(sync.resetDesired).toEqual(newerReset);
    expect(sync.resetInFlight).toBeNull();
    expect(nextFloatingResetRequest(sync)).toEqual(newerReset);
  });

  it('accepts reset failure reconciliation without satisfying reset presentation', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('expanded', 380, true));
    sync = requestFloatingReset(sync);
    const resetRequest = nextFloatingResetRequest(sync)!;
    sync = beginFloatingResetRequest(sync, resetRequest);
    sync = acceptFloatingResetFailure(sync, resetRequest);
    sync = acceptFloatingResetReconciliation(sync, resetRequest, prefs('expanded', 380, false));

    expect(sync.preferences).toEqual(prefs('expanded', 380, false));
    expect(sync.appliedGeneration).toBeLessThan(resetRequest.generation);
  });

  it('keeps reset authoritative when its completion precedes an obsolete set response', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340, true));
    sync = requestFloatingSize(sync, 'expanded');
    const setRequest = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, setRequest);
    sync = requestFloatingReset(sync);
    const resetRequest = sync.resetDesired!;

    sync = beginFloatingResetRequest(sync, resetRequest);
    sync = acceptFloatingResetSuccess(sync, resetRequest, prefs('expanded', 380, false));
    sync = acceptFloatingSizeSuccess(sync, setRequest, prefs('expanded', 380, true));

    expect(sync.preferences).toEqual(prefs('expanded', 380, false));
  });

  it('drains a newer display mode transition after reset', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('expanded', 380, true));
    sync = requestFloatingReset(sync);
    const resetRequest = sync.resetDesired!;
    sync = beginFloatingResetRequest(sync, resetRequest);
    sync = requestFloatingSize(sync, 'capsule');

    sync = acceptFloatingResetSuccess(sync, resetRequest, prefs('expanded', 380, false));

    expect(sync.preferences).toEqual(prefs('expanded', 380, true));
    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'capsule' });
  });

  it('accepts preferences only for the latest semantic request generation', () => {
    let sync = acceptFloatingPreferencesRead(state(), prefs('capsule', 340));
    sync = requestFloatingSize(sync, 'expanded');
    const request = nextFloatingSizeRequest(sync)!;
    sync = beginFloatingSizeRequest(sync, request);
    sync = acceptFloatingSizeSuccess(sync, request, prefs('expanded', 380, true));

    expect(sync.preferences).toEqual(prefs('expanded', 380, true));
    expect(nextFloatingSizeRequest(sync)).toBeNull();
  });
});
