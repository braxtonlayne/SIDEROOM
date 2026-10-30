/**
 * The "Load embeds" account setting (PRF-UNF-001). Default `ask`: a link's "Play here" card loads nothing from
 * YouTube, Vimeo or the origin until the reader clicks. `always` ("then remember") skips the click for the
 * provider embeds and the direct media player; the card itself still renders without any third-party request
 * (an image still goes through the proxy or its own gate). It lives in the account's settings blob under
 * `load_embeds`, like `load_remote_images`, so it follows the user to every browser.
 */
import { rest, useStore } from '../../store';

export const LOAD_EMBEDS_KEY = 'load_embeds';

export type LoadEmbeds = 'ask' | 'always';

/** `always` only when the blob holds exactly that; anything else (missing, unknown) is `ask`. */
export const useLoadEmbeds = (): LoadEmbeds =>
  useStore((s) => (s.userSettings?.settings[LOAD_EMBEDS_KEY] === 'always' ? 'always' : 'ask'));

/** Saves the setting and records the new blob version in the store. Rejects when the save fails. */
export async function setLoadEmbeds(mode: LoadEmbeds): Promise<void> {
  const saved = await rest.patchUserSettings({ [LOAD_EMBEDS_KEY]: mode });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}
