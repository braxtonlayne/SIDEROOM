// @ts-check
// The only place the game learns about its host app. Picked by where the page runs: SIDE ROOM's iframe
// (<app id>.activities.<host>, framed), else the local stand-in (plain tab: `?user=` / `?instance=` in the
// query; development and the replay page). Same pattern as Land.io's client/platform/.

import { createFakePlatform } from './fake-platform.js';
import { createSideRoomPlatform, isSideRoom } from './sideroom-platform.js';

/**
 * A tiny key-value store with the localStorage shape. In SIDE ROOM it is the SDK's `storage` (the sandbox has
 * no localStorage of its own); elsewhere it is the tab's localStorage or memory.
 * @typedef {{ getItem(key: string): string | null, setItem(key: string, value: string): void, removeItem(key: string): void }} KeyValue
 */

/**
 * @typedef {object} Platform
 * @property {'sideroom' | 'fake'} kind
 * @property {() => Promise<void>} ready               handshake; after it `channel`, `identity` and `storage` are set
 * @property {() => { id: string, name: string }} identity
 * @property {boolean} guest                           playing on a /play/<token> link (a reduced bridge)
 * @property {import('../net/channel.js').Channel | null} channel
 * @property {KeyValue} storage
 * @property {() => string} instanceId
 * @property {(text: string) => void} setActivity     presence line; a no-op for guests
 * @property {() => void} close
 */

/** @param {Window} win @returns {Platform} */
export function getPlatform(win) {
  if (isSideRoom(win)) return createSideRoomPlatform(win);
  return createFakePlatform(win);
}

/** A KeyValue in memory (used when nothing better exists). @returns {KeyValue} */
export function memoryStore() {
  /** @type {Map<string, string>} */
  const m = new Map();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, String(v)),
    removeItem: (k) => void m.delete(k),
  };
}
