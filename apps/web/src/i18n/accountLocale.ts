/**
 * One locale (G-17, P9-I18N-1). The Language picker is the only language setting: choosing a language
 *   1. switches the interface (`setLocale`),
 *   2. writes the account locale, `PATCH /users/@me {locale}`, which mails, push text and system DMs follow
 *      (the backend accepts BCP 47 tags it has catalogues for; until it does, the request is ignored and
 *      the blob copy below still carries the choice to other devices), and
 *   3. mirrors it into the settings blob (`prefs.locale`, synced by `prefsSync`) so formats and first paint
 *      follow it on every device.
 * On boot, a device with no saved language adopts the account locale.
 */
import { apiFetch } from '../api/client';
import { DEFAULT_PREFS, usePrefsStore } from '../settings/user/prefs';
import { isPreviewLocale, negotiateLocale, setLocale } from './runtime';

const STORAGE_KEY = 'sr.locale';

/** Switch the interface language and keep the account and the settings blob in step. */
export async function chooseLanguage(code: string, pseudo = false): Promise<void> {
  await setLocale(code);
  // The pseudo-locales exist only to test layouts; they never reach the account.
  if (pseudo) return;
  const { prefs, save } = usePrefsStore.getState();
  if (prefs.locale !== code) save({ ...(prefs ?? DEFAULT_PREFS), locale: code });
  await apiFetch('/users/@me', { method: 'PATCH', json: { locale: code }, retries: 0 }).catch(
    () => undefined,
  );
}

function hasSavedLocale(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) !== null;
  } catch {
    return false;
  }
}

/** Adopt the account locale on a device that has never chosen a language. Returns the locale applied. */
export async function adoptAccountLocale(account: string | undefined): Promise<string | null> {
  if (!account || hasSavedLocale() || isPreviewLocale()) return null;
  const code = negotiateLocale([account]);
  await setLocale(code);
  return code;
}
