/**
 * Keeps the local preferences (`prefs.ts`) and the account copy in step.
 *
 * The account copy is the object under `user_prefs` in the settings blob (`GET/PATCH /users/@me/settings`,
 * RFC 7386 merge patch; the key is a plain JSON object with the same field names as `UserPrefs`). The
 * browser copy loads first so the first paint never waits; when the blob arrives (or changes on another
 * device, `SR_USER_SETTINGS_UPDATE`) the account copy wins. A first sign-in on a device that has local
 * choices and an account with none pushes the local choices up once. Saves write the whole object; entries
 * that were removed from the two record fields (`sectionThemes`, `keybinds`) go out as explicit nulls, which
 * is how a merge patch deletes a key.
 *
 * Offline or rejected writes stay in localStorage and are retried on the next change or the `online` event.
 */
import { useEffect } from 'react';
import { rest, useStore } from '../../store';
import { DEFAULT_PREFS, prefsEqual, sanitizePrefs, usePrefsStore, type UserPrefs } from './prefs';

export const PREFS_KEY = 'user_prefs';

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const RECORD_FIELDS = ['sectionThemes', 'keybinds'] as const;

/** The `user_prefs` merge patch that turns what the server holds (`before`, raw) into `next`. */
export function prefsPatch(before: unknown, next: UserPrefs): Record<string, unknown> {
  const out: Record<string, unknown> = { ...next };
  const prev = isRecord(before) ? before : {};
  for (const field of RECORD_FIELDS) {
    const entries: Record<string, unknown> = { ...next[field] };
    const old = prev[field];
    if (isRecord(old)) for (const k of Object.keys(old)) if (!(k in entries)) entries[k] = null;
    out[field] = entries;
  }
  return out;
}

/** Starts the sync; returns the stop function. Exported for tests (components use `usePrefsSync`). */
export function startPrefsSync(): () => void {
  /** The raw `user_prefs` the server is known to hold (undefined: none yet / not loaded). */
  let known: unknown;
  let loaded = false;
  let stopped = false;
  let adopting = false;
  let ownWrite = false;
  let inflight = false;
  let again = false;
  /** A save happened before the blob loaded: push it over whatever the server has. */
  let pendingLocal = false;

  const flush = async (): Promise<void> => {
    if (stopped || !loaded) return;
    if (inflight) {
      again = true;
      return;
    }
    const local = usePrefsStore.getState().prefs;
    if (isRecord(known) && prefsEqual(sanitizePrefs(known), local)) return;
    if (known === undefined && prefsEqual(local, DEFAULT_PREFS)) return;
    inflight = true;
    try {
      const saved = await rest.patchUserSettings({ [PREFS_KEY]: prefsPatch(known, local) });
      known = saved.settings[PREFS_KEY];
      ownWrite = true;
      try {
        if (!stopped) useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
      } finally {
        ownWrite = false;
      }
    } catch {
      /* offline or rejected: the browser copy keeps working; the next change or `online` retries */
    } finally {
      inflight = false;
      if (again) {
        again = false;
        void flush();
      }
    }
  };

  const adopt = (settings: Record<string, unknown> | undefined) => {
    if (!settings) return;
    const raw = settings[PREFS_KEY];
    known = raw;
    loaded = true;
    if (pendingLocal) {
      pendingLocal = false;
      void flush();
      return;
    }
    if (isRecord(raw)) {
      const next = sanitizePrefs(raw);
      if (!prefsEqual(usePrefsStore.getState().prefs, next)) {
        adopting = true;
        try {
          usePrefsStore.getState().adopt(next);
        } finally {
          adopting = false;
        }
      }
    } else {
      // The account has none yet: share this browser's choices once (a fresh account stays empty).
      void flush();
    }
  };

  const unPrefs = usePrefsStore.subscribe((s, prev) => {
    if (adopting || s.prefs === prev.prefs) return;
    if (!loaded) pendingLocal = true;
    else void flush();
  });
  let last = useStore.getState().userSettings;
  const unStore = useStore.subscribe((s) => {
    if (s.userSettings === last) return;
    last = s.userSettings;
    if (!ownWrite) adopt(s.userSettings?.settings);
  });
  const onOnline = () => void flush();
  window.addEventListener('online', onOnline);

  if (last) adopt(last.settings);
  else
    rest
      .fetchUserSettings()
      .then((blob) => {
        if (stopped) return;
        useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob);
        if (!loaded) adopt(useStore.getState().userSettings?.settings ?? blob.settings);
      })
      .catch(() => {
        /* signed out or offline: the browser copy is used */
      });

  return () => {
    stopped = true;
    unPrefs();
    unStore();
    window.removeEventListener('online', onOnline);
  };
}

let users = 0;
let stop: (() => void) | undefined;

/** Mounts the sync while any themed screen is open (shared between screens, so it starts once). */
export function usePrefsSync(): void {
  useEffect(() => {
    users += 1;
    if (users === 1) stop = startPrefsSync();
    return () => {
      users -= 1;
      if (users === 0) {
        stop?.();
        stop = undefined;
      }
    };
  }, []);
}
