/**
 * The "Load remote images automatically" client setting (D-176, F-20). Default off. It lives in the account's
 * settings blob (`GET/PATCH /users/@me/settings`) under `load_remote_images`, so it follows the user to
 * every browser. When on, embed images still on the other site load without the per-embed "Load image"
 * click (always with `no-referrer`); proxied images load either way.
 */
import { useEffect } from 'react';
import { rest, useStore } from '../../store';

export const LOAD_REMOTE_IMAGES_KEY = 'load_remote_images';

/** True only when the blob holds an explicit `true`. */
export const useLoadRemoteImages = (): boolean =>
  useStore((s) => s.userSettings?.settings[LOAD_REMOTE_IMAGES_KEY] === true);

/** Fetches the blob when this session has not loaded it yet (Settings opened outside the Lounge). */
export function useEnsureUserSettings(): void {
  useEffect(() => {
    if (useStore.getState().userSettings) return;
    rest
      .fetchUserSettings()
      .then((blob) => useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob))
      .catch(() => undefined);
  }, []);
}

/** Saves the setting and records the new blob version in the store. Rejects when the save fails. */
export async function setLoadRemoteImages(on: boolean): Promise<void> {
  const saved = await rest.patchUserSettings({ [LOAD_REMOTE_IMAGES_KEY]: on });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}
