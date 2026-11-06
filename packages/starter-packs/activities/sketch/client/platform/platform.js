// @ts-check
// The only place the game learns about its host (the same seam as Land.io's). Picked by where the page runs:
// SIDE ROOM's iframe (<app id>.activities.<host> in a frame, or /_activity/<id>/ on a test deployment), else the
// local stand-in (`?user=` / `?instance=` in the query: development, the visual check and tests).

import { createFakePlatform } from './fake-platform.js';
import { createSideRoomPlatform, isSideRoom } from './sideroom-platform.js';

/**
 * @typedef {object} Identity
 * @property {string} id      the platform user id (a member id, or the lobby id the relay gave a guest)
 * @property {string} name    the nickname to start with: a member's display name (never the handle), a guest's callsign
 */

/**
 * @typedef {object} Platform
 * @property {() => Promise<void>} ready
 * @property {() => Identity} identity
 * @property {() => string} instanceId
 * @property {(text: string) => void} setActivity           Rich Presence (members only; a guest has none)
 * @property {string} kind                                  'fake' | 'sideroom'
 * @property {boolean} guest                                a guest on a /play/<token> link (data channel, storage, ready only)
 * @property {boolean} canLoadPacks                         a member whose bundle declares byos_read: Guild Prompt Packs
 * @property {boolean} canPickPacks                         canLoadPacks and the host has the app's own file picker (D-466, pickShelfObject)
 * @property {() => Promise<{ shelf_id: string, object_hash: string, name: string } | null>} pickPack
 *   the member chooses a pack file in the app's picker (JSON or text, at most one pack's size); null when they cancel. That choice is their yes for that object
 * @property {(ref: { shelf_id: string, object_hash: string }) => Promise<Uint8Array>} loadPack
 *   the bytes of one shelf object, fetched by the member page through its per-object URL (D-389, D-460); rejects for a guest or without byos_read
 * @property {import('../net/channel.js').Channel | null} channel   the data channel (after ready)
 */

/** @param {Window} win @returns {Platform} */
export function getPlatform(win) {
  if (isSideRoom(win)) return createSideRoomPlatform(win);
  return createFakePlatform(win);
}
