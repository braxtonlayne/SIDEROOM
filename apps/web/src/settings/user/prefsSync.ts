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

/**
 * Set while this browser holds a change the account copy has not taken yet (made before the blob loaded, or
 * whose write failed). It survives a reload, so the next load pushes the browser copy up instead of letting
 * the older account copy overwrite it (SEC-AUD-037). The mark names the account it was made under ('1' while
 * the account is not known yet): on a shared browser one account's waiting change never reaches another's.
 */
export const UNSYNCED_KEY = 'sr.user.prefs.unsynced';
function markUnsynced(on: boolean): void {
  try {
    if (on) localStorage.setItem(UNSYNCED_KEY, useStore.getState().currentUserId ?? '1');
    else localStorage.removeItem(UNSYNCED_KEY);
  } catch {
    /* private mode: the in-memory flag still covers this page */
  }
}
/** Whether the mark is this account's. Another account's mark is dropped: its change is not this account's. */
function wasUnsynced(): boolean {
  try {
    const mark = localStorage.getItem(UNSYNCED_KEY);
    if (mark === null) return false;
    const me = useStore.getState().currentUserId;
    if (mark === '1' || !me || mark === me) return true;
    localStorage.removeItem(UNSYNCED_KEY);
    return false;
  } catch {
    return false;
  }
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
  let fetching = false;
  /** A save happened before the blob loaded (in this page or an earlier one): push it over what the server has. */
  let pendingLocal = wasUnsynced();
  /** A change was made in this page before the blob loaded (always this account's). */
  let changedHere = false;

  const flush = async (): Promise<void> => {
    if (stopped || !loaded) return;
    if (inflight) {
      again = true;
      return;
    }
    const local = usePrefsStore.getState().prefs;
    if (
      (isRecord(known) && prefsEqual(sanitizePrefs(known), local)) ||
      (known === undefined && prefsEqual(local, DEFAULT_PREFS))
    ) {
      markUnsynced(false);
      return;
    }
    inflight = true;
    markUnsynced(true);
    try {
      const saved = await rest.patchUserSettings({ [PREFS_KEY]: prefsPatch(known, local) });
      known = saved.settings[PREFS_KEY];
      if (prefsEqual(usePrefsStore.getState().prefs, local)) markUnsynced(false);
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
    // A change made in this page counts; a mark from an earlier page counts only if it is this account's (the
    // account is known by now, it may not have been when the sync started).
    if (pendingLocal && (changedHere || wasUnsynced())) {
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
    if (!loaded) {
      pendingLocal = true;
      changedHere = true;
      markUnsynced(true);
      fetchBlob();
    } else void flush();
  });
  let last = useStore.getState().userSettings;
  const unStore = useStore.subscribe((s) => {
    if (s.userSettings === last) return;
    last = s.userSettings;
    if (!ownWrite) adopt(s.userSettings?.settings);
  });
  // A load that failed (offline at page load) is tried again when the network returns or the member changes
  // something; until it succeeds nothing is written, and the change waits in the browser copy (SEC-AUD-037).
  function fetchBlob(): void {
    if (stopped || loaded || fetching) return;
    fetching = true;
    rest
      .fetchUserSettings()
      .then((blob) => {
        if (stopped) return;
        useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob);
        if (!loaded) adopt(useStore.getState().userSettings?.settings ?? blob.settings);
      })
      .catch(() => {
        /* signed out or offline: the browser copy is used */
      })
      .finally(() => {
        fetching = false;
      });
  }
  const onOnline = () => {
    if (loaded) void flush();
    else fetchBlob();
  };
  window.addEventListener('online', onOnline);

  if (last) adopt(last.settings);
  else fetchBlob();

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
