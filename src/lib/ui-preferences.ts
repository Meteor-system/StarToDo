import { readPreference, writePreference } from './preferences';

export type AutoImmersivePreference = 'unset' | 'enabled' | 'disabled';
export type FloatingExpansionPreference = 'auto' | 'always';

export interface UiPreferences {
  autoImmersive: AutoImmersivePreference;
  floatingExpansion: FloatingExpansionPreference;
}

const AUTO_IMMERSIVE_KEY = 'startodo.auto-immersive';
const FLOATING_EXPANSION_KEY = 'startodo.floating-expansion';

export const DEFAULT_UI_PREFERENCES: UiPreferences = {
  autoImmersive: 'unset',
  floatingExpansion: 'auto'
};

function oneOf<T extends string>(
  value: string | null,
  allowed: readonly T[],
  fallback: T
): T {
  return value !== null && allowed.includes(value as T)
    ? (value as T)
    : fallback;
}

export function readUiPreferences(): UiPreferences {
  return {
    autoImmersive: oneOf(
      readPreference(AUTO_IMMERSIVE_KEY),
      ['unset', 'enabled', 'disabled'] as const,
      DEFAULT_UI_PREFERENCES.autoImmersive
    ),
    floatingExpansion: oneOf(
      readPreference(FLOATING_EXPANSION_KEY),
      ['auto', 'always'] as const,
      DEFAULT_UI_PREFERENCES.floatingExpansion
    )
  };
}

export function writeAutoImmersivePreference(
  value: AutoImmersivePreference
): void {
  writePreference(AUTO_IMMERSIVE_KEY, value);
}

export function writeFloatingExpansionPreference(
  value: FloatingExpansionPreference
): void {
  writePreference(FLOATING_EXPANSION_KEY, value);
}
