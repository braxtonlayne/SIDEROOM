import { useMediaQuery } from '@sideroom/ui';
import { useMemo } from 'react';
import { applyPrefs, useSavedPrefs, type AppliedTheme, type UserPrefs } from './prefs';
import { usePlatformThemeDefaults } from './platformThemes';
import { usePrefsSync } from './prefsSync';

/**
 * ThemeProvider props for the saved preferences (used by the Lounge shell and the Library). Also starts the
 * account sync, so every themed section picks up choices made on another device.
 */
export function useAppliedTheme(prefs?: UserPrefs): AppliedTheme {
  usePrefsSync();
  const saved = useSavedPrefs();
  const platform = usePlatformThemeDefaults();
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const p = prefs ?? saved;
  return useMemo(() => applyPrefs(p, dark, platform), [p, dark, platform]);
}
