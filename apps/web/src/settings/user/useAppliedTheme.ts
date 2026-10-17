import { useMediaQuery } from '@sideroom/ui';
import { useEffect, useMemo } from 'react';
import { useStoreThemes, useStoreThemeStyle } from '../../packages/themes';
import {
  applyPrefs,
  useSavedPrefs,
  type AppliedTheme,
  type ThemedSection,
  type UserPrefs,
} from './prefs';
import { usePrefsSync } from './prefsSync';

/**
 * ThemeProvider props for the saved preferences (used by the Lounge shell and the Library). Also starts the
 * account sync, so every themed section picks up choices made on another device. A theme the person turned on
 * from the Theme Store (EXT-PKG-005) is laid over the built-in theme for `section`.
 */
export function useAppliedTheme(
  prefs?: UserPrefs,
  section: ThemedSection = 'lounge',
): AppliedTheme {
  usePrefsSync();
  const loadThemes = useStoreThemes((s) => s.load);
  useEffect(() => {
    void loadThemes();
  }, [loadThemes]);
  const saved = useSavedPrefs();
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const storeStyle = useStoreThemeStyle(section);
  const p = prefs ?? saved;
  return useMemo(() => {
    const applied = applyPrefs(p, dark);
    return { ...applied, style: { ...applied.style, ...storeStyle } };
  }, [p, dark, storeStyle]);
}
