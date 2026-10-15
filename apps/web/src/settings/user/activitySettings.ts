/**
 * Activity Privacy lives on the server (G-58, D-190): the account's settings blob holds an `activity` object
 * with `share_cinema`, `share_library`, `share_tv`, `hide_age_restricted` and `paused`. The gateway, the
 * Friends and "People you know" trending and rating scopes and the profile all read the same keys. Absent
 * keys mean the defaults below (D-120: sharing on, adult titles hidden, history not paused).
 */
import { useEffect } from 'react';
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

/** The current settings (defaults until the blob loads). */
export function useActivitySettings(): ActivitySettings {
  const blob = useStore((s) => s.userSettings?.settings[ACTIVITY_KEY]);
  useEffect(() => {
    if (useStore.getState().userSettings) return;
    rest
      .fetchUserSettings()
      .then((b) => useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', b))
      .catch(() => undefined);
  }, []);
  return readActivity(blob);
}

/** Saves some of the keys (a merge patch) and records the new blob version. Rejects when the save fails. */
export async function saveActivity(patch: Partial<ActivitySettings>): Promise<void> {
  const saved = await rest.patchUserSettings({ [ACTIVITY_KEY]: patch });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}
