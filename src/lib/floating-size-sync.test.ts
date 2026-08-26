import { describe, expect, it } from 'vitest';

import {
  acceptFloatingPreferencesRead,
  acceptFloatingReconciliation,
  acceptFloatingSizeFailure,
  acceptFloatingSizeSuccess,
  beginFloatingSizeRequest,
  createFloatingSizeSyncState,
  failFloatingPreferencesRead,
  nextFloatingSizeRequest,
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
    expect(nextFloatingSizeRequest(sync)).toMatchObject({ mode: 'expanded', generation: request.generation });
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
    expect(nextFloatingSizeRequest(sync)).toEqual(request);
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
