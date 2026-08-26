import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_UI_PREFERENCES,
  readUiPreferences,
  writeAutoImmersivePreference,
  writeFloatingExpansionPreference
} from './ui-preferences';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('readUiPreferences', () => {
  it('uses defaults when storage is unavailable', () => {
    vi.stubGlobal('window', undefined);

    expect(readUiPreferences()).toEqual(DEFAULT_UI_PREFERENCES);
  });

  it('uses defaults for absent or invalid values', () => {
    const getItem = vi.fn((key: string) =>
      key === 'startodo.auto-immersive' ? 'sometimes' : 'wide'
    );
    vi.stubGlobal('window', {
      localStorage: { getItem, setItem: vi.fn() }
    });

    expect(readUiPreferences()).toEqual({
      autoImmersive: 'unset',
      floatingExpansion: 'auto'
    });
  });

  it('reads valid values', () => {
    const values = new Map<string, string>([
      ['startodo.auto-immersive', 'enabled'],
      ['startodo.floating-expansion', 'always']
    ]);

    vi.stubGlobal('window', {
      localStorage: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: vi.fn()
      }
    });

    expect(readUiPreferences()).toEqual({
      autoImmersive: 'enabled',
      floatingExpansion: 'always'
    });
  });
});

describe('UI preference writes', () => {
  it('writes the exact serialized values', () => {
    const setItem = vi.fn();
    vi.stubGlobal('window', {
      localStorage: { getItem: vi.fn(), setItem }
    });

    writeAutoImmersivePreference('disabled');
    writeFloatingExpansionPreference('auto');

    expect(setItem).toHaveBeenCalledWith(
      'startodo.auto-immersive',
      'disabled'
    );
    expect(setItem).toHaveBeenCalledWith(
      'startodo.floating-expansion',
      'auto'
    );
  });

  it('does not throw when localStorage rejects writes', () => {
    const setItem = vi.fn(() => {
      throw new DOMException('Quota exceeded', 'QuotaExceededError');
    });
    vi.stubGlobal('window', {
      localStorage: { getItem: vi.fn(), setItem }
    });

    expect(() => writeAutoImmersivePreference('enabled')).not.toThrow();
    expect(() => writeFloatingExpansionPreference('always')).not.toThrow();
  });
});
