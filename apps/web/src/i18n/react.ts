import { useCallback, useSyncExternalStore } from 'react';
import {
  LOCALES,
  getHour12,
  getLocale,
  getVersion,
  localeInfo,
  setHour12,
  setLocale,
  subscribe,
  t,
} from './runtime';

/**
 * `const t = useT();  t('entrance.login.title')` (XC-I18N-001).
 *
 * Re-renders the component when the locale changes. The returned function is stable per locale, so it is
 * safe in dependency arrays. No provider is needed.
 */
export function useT(): typeof t {
  const version = useSyncExternalStore(subscribe, getVersion, getVersion);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- `version` is the cache key
  return useCallback(((key, values) => t(key, values)) as typeof t, [version]);
}

/**
 * Locale switching for settings screens: `const { locale, locales, setLocale } = useLocale();`.
 * p5-user-settings can render `locales` in a select and call `setLocale(code)`; the choice persists.
 */
export function useLocale() {
  useSyncExternalStore(subscribe, getVersion, getVersion);
  const info = localeInfo(getLocale());
  return {
    locale: info.code,
    dir: info.dir,
    locales: LOCALES.filter((l) => !l.pseudo || import.meta.env.DEV),
    setLocale,
    hour12: getHour12(),
    setHour12,
  };
}
