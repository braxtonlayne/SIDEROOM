/**
 * Activity Privacy lives on the server (G-58, D-190): the account's settings blob holds an `activity` object
 * with `share_cinema`, `share_library`, `share_tv`, `hide_age_restricted` and `paused`. The gateway, the
 * Friends and "People you know" trending and rating scopes and the profile all read the same keys. Absent
 * keys mean the defaults below (D-120: sharing on, adult titles hidden, history not paused).
 */
import { useEffect, useState } from 'react';
import { rest, useStore } from '../../store';

export const ACTIVITY_KEY = 'activity';

export interface ActivitySettings {
  share_cinema: boolean;
  share_library: boolean;
  share_tv: boolean;
  hide_age_restricted: boolean;
  paused: boolean;
}

export const ACTIVITY_DEFAULTS: ActivitySettings = {
  share_cinema: true,
  share_library: true,
  share_tv: true,
  hide_age_restricted: true,
  paused: false,
};

/** The stored object merged over the defaults; anything that is not a boolean is ignored. */
export function readActivity(blob: unknown): ActivitySettings {
  const raw = typeof blob === 'object' && blob !== null ? (blob as Record<string, unknown>) : {};
  const out = { ...ACTIVITY_DEFAULTS };
  for (const k of Object.keys(out) as (keyof ActivitySettings)[]) {
    if (typeof raw[k] === 'boolean') out[k] = raw[k];
  }
  return out;
}

/** `library.share_reading` (D-229): who sees what you read. Absent means friends. */
export type ShareReading = 'off' | 'friends' | 'everyone';
export const SHARE_READING_KEY = 'library';

/** The stored level; anything unknown is the default, friends. */
export function readShareReading(blob: unknown): ShareReading {
  const v =
    typeof blob === 'object' && blob !== null
      ? (blob as Record<string, unknown>).share_reading
      : undefined;
  return v === 'off' || v === 'everyone' ? v : 'friends';
}

export function useShareReading(): ShareReading {
  return readShareReading(useStore((s) => s.userSettings?.settings[SHARE_READING_KEY]));
}

/** Saves the reading audience and records the new blob version. Rejects when the save fails. */
export async function saveShareReading(level: ShareReading): Promise<void> {
  const saved = await rest.patchUserSettings({ [SHARE_READING_KEY]: { share_reading: level } });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}

let blobLoad: Promise<boolean> | null = null;
/** Reads the settings blob once (shared by every caller); resolves false when the read failed. */
function ensureSettingsBlob(): Promise<boolean> {
  if (useStore.getState().userSettings) return Promise.resolve(true);
  blobLoad ??= rest
    .fetchUserSettings()
    .then((b) => {
      useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', b);
      return true;
    })
    .catch(() => false)
    .finally(() => {
      blobLoad = null;
    });
  return blobLoad;
}

/** The current settings (defaults until the blob loads). */
export function useActivitySettings(): ActivitySettings {
  const blob = useStore((s) => s.userSettings?.settings[ACTIVITY_KEY]);
  useEffect(() => {
    void ensureSettingsBlob();
  }, []);
  return readActivity(blob);
}

/**
 * Whether the switches show the account's real choices (SEC-AUD-037): until the blob has loaded they show the
 * defaults, so the screen keeps them off; a failed read says so and offers Try again.
 */
export function useActivityLoad(): { loaded: boolean; failed: boolean; retry: () => void } {
  const loaded = useStore((s) => !!s.userSettings);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    setFailed(false);
    void ensureSettingsBlob().then((ok) => live && setFailed(!ok));
    return () => {
      live = false;
    };
  }, [attempt]);
  return { loaded, failed: failed && !loaded, retry: () => setAttempt((n) => n + 1) };
}

/** Saves some of the keys (a merge patch) and records the new blob version. Rejects when the save fails. */
export async function saveActivity(patch: Partial<ActivitySettings>): Promise<void> {
  const saved = await rest.patchUserSettings({ [ACTIVITY_KEY]: patch });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}
