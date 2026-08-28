import { afterEach, describe, expect, it, vi } from 'vitest';

import { readPreference, writePreference } from './preferences';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('preferences', () => {
  it('returns null and does not throw when window is unavailable in the node environment', () => {
    vi.stubGlobal('window', undefined);

    expect(readPreference('workspace-view')).toBeNull();
    expect(() => writePreference('workspace-view', 'today')).not.toThrow();
  });

  it('reads and writes through window.localStorage when it is available', () => {
    const getItem = vi.fn(() => 'week');
    const setItem = vi.fn();
    vi.stubGlobal('window', { localStorage: { getItem, setItem } });

    expect(readPreference('workspace-view')).toBe('week');
    writePreference('workspace-view', 'today');

    expect(getItem).toHaveBeenCalledWith('workspace-view');
    expect(setItem).toHaveBeenCalledWith('workspace-view', 'today');
  });

  it('returns null when localStorage reading throws a SecurityError', () => {
    const getItem = vi.fn(() => {
      throw new DOMException('Storage is blocked', 'SecurityError');
    });
    vi.stubGlobal('window', { localStorage: { getItem, setItem: vi.fn() } });

    expect(readPreference('workspace-view')).toBeNull();
    expect(getItem).toHaveBeenCalledWith('workspace-view');
  });

  it('does not throw when localStorage writing throws a quota error', () => {
    const setItem = vi.fn(() => {
      throw new DOMException('Quota exceeded', 'QuotaExceededError');
    });
    vi.stubGlobal('window', { localStorage: { getItem: vi.fn(), setItem } });

    expect(() => writePreference('workspace-view', 'today')).not.toThrow();
    expect(setItem).toHaveBeenCalledWith('workspace-view', 'today');
  });
});
