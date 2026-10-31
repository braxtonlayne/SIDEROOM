// @ts-check
// The only place the game learns about its host. Everything the game needs from its host goes through this
// interface, shaped like the Embedded App SDKs. Picked by where the page runs (D-57): Discord's iframe (a
// frame_id in the query), SIDE ROOM's (<app id>.activities.<host> in a frame), the local stand-in (`?user=` or
// `?instance=` in the query: development and tests), or else a plain browser tab, where people play as guests (D-60).

import { createFakePlatform } from './fake-platform.js';
import { createDiscordPlatform, isDiscord } from './discord-platform.js';
import { createSideRoomPlatform, isSideRoom } from './sideroom-platform.js';
import { createWebPlatform } from './web-platform.js';

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
 * @property {() => string} instanceId                      one arena per instance (per channel)
 * @property {(text: string) => void} setActivity           the person's presence line ("Playing Land.io — 4.2%")
 * @property {() => string | null} ticket                   the server's signed ticket for this person (null locally)
 * @property {string} kind                                  'fake' | 'discord' | 'sideroom' | 'web'
 * @property {() => boolean} [hasName]                      web: whether the guest has picked a name yet
 * @property {(name: string) => Promise<string>} [setName]  web: become (or rename) the guest; '' or an error
 */

/** @param {Window} win @returns {Platform} */
export function getPlatform(win) {
  if (isDiscord(win)) return createDiscordPlatform(win);
  if (isSideRoom(win)) return createSideRoomPlatform(win);
  const q = new URLSearchParams(win.location.search);
  if (q.has('user') || q.has('instance')) return createFakePlatform(win);
  return createWebPlatform(win);
}
