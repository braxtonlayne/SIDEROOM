// @ts-check
// The only place the game learns about its host. Everything the game needs from its host goes through this
// interface, shaped like the Embedded App SDKs. Picked by where the page runs: SIDE ROOM's iframe
// (<app id>.activities.<host> in a frame), else the local stand-in (`?user=` / `?instance=` in the query, or a
// plain tab: development and tests). Identity comes from the host, never from a Land.io sign-in.

import { createFakePlatform } from './fake-platform.js';
import { createSideRoomPlatform, isSideRoom } from './sideroom-platform.js';

/**
 * @typedef {object} Identity
 * @property {string} id      the platform user id
 * @property {string} name    display name
 */

/**
 * @typedef {object} Platform
 * @property {() => Promise<void>} ready                    handshake with the host
 * @property {() => Identity} identity                      who am I (after ready)
 * @property {() => Promise<Identity[]>} participants       who is in this Activity instance
 * @property {(cb: (p: Identity[]) => void) => () => void} onParticipantsChange
 * @property {() => string} instanceId                      one game per instance (per channel)
 * @property {(text: string) => void} setActivity           the person's presence line ("Playing Land.io — 4.2%")
 * @property {() => string | null} ticket                   always null: the host knows who everyone is
 * @property {string} kind                                  'fake' | 'sideroom'
 * @property {() => boolean} [hasName]                      (the old plain-browser lobby; unused here)
 * @property {(name: string) => Promise<string>} [setName]  (the old plain-browser lobby; unused here)
 * @property {import('../net/channel.js').Channel | null} [channel]   the data channel to the other members (after ready)
 */

/** @param {Window} win @returns {Platform} */
export function getPlatform(win) {
  if (isSideRoom(win)) return createSideRoomPlatform(win);
  return createFakePlatform(win);
}
