// @ts-check
// A Channel over the Embedded App SDK's data channel (ACT-CH-001): `sdk.input.send` from a peer to the host,
// `sdk.input.reply` from the host to one member, `sdk.state.set({ 'host:<key>': base64 })` for what the host
// publishes to everyone, `sdk.on('state' | 'input')` for what arrives. The client page encrypts every envelope
// under the room's sender key; the SFU relays opaque bytes. Uses nothing but the data channel, so it is the same
// for members and for guests on a /play/<token> link.

import { asBytes, fromBase64, toBase64 } from './channel.js';

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./channel.js').Member} Member */

const HOST_PREFIX = 'host:';

/**
 * The slice of SideRoomSDK this needs (packages/embedded-app-sdk/src/sdk.ts).
 * @typedef {object} SdkLike
 * @property {boolean} isHost
 * @property {string | null} self
 * @property {string | null} hostId
 * @property {{ all(): Record<string, unknown>, set(patch: Record<string, unknown>): void }} state
 * @property {{ send(bytes: Uint8Array): void, reply(member: string, bytes: Uint8Array): void }} input
 * @property {(event: 'state' | 'input' | 'host', cb: (e: any) => void) => () => void} on
 */

/**
 * @param {SdkLike} sdk
 * @param {{ name: string }} opt
 * @returns {Channel}
 */
export function channelFromSdk(sdk, opt) {
  const self = { id: String(sdk.self ?? ''), name: opt.name };
  /** @type {(() => void)[]} */
  const offs = [];
  // D-388: the platform can move the host (HOST_CHANGED); the SDK follows it and says so with a 'host' event.
  let hostId = String(sdk.hostId ?? '');
  /** @type {Set<(host: string, previous: string) => void>} */
  const hostCbs = new Set();
  offs.push(
    sdk.on('host', (e) => {
      if (!e || typeof e.host !== 'string' || e.host === hostId) return;
      const previous = hostId;
      hostId = e.host;
      hostCbs.forEach((cb) => cb(e.host, previous));
    }),
  );
  return {
    self,
    get hostId() {
      return hostId;
    },
    get isHost() {
      return sdk.isHost;
    },
    onHostChange(cb) {
      hostCbs.add(cb);
      return () => hostCbs.delete(cb);
    },
    toHost: (bytes) => sdk.input.send(bytes),
    onInput(cb) {
      const off = sdk.on('input', (e) => {
        if (e.reply) return;
        const b = asBytes(e.data);
        if (b) cb(String(e.from), b);
      });
      offs.push(off);
      return off;
    },
    publish: (key, bytes) => sdk.state.set({ [HOST_PREFIX + key]: toBase64(bytes) }),
    onState(cb) {
      /** @param {string} key @param {unknown} value */
      const deliver = (key, value) => {
        if (!key.startsWith(HOST_PREFIX) || typeof value !== 'string') return;
        const b = fromBase64(value);
        if (b) cb(key.slice(HOST_PREFIX.length), b);
      };
      // A newcomer first hears the document as it stands (the SDK synced it from the host).
      for (const [key, value] of Object.entries(sdk.state.all())) deliver(key, value);
      const off = sdk.on('state', (e) => {
        for (const c of e.changes ?? []) if (!c.local) deliver(String(c.key), c.value);
      });
      offs.push(off);
      return off;
    },
    sendTo: (to, bytes) => sdk.input.reply(to, bytes),
    onReply(cb) {
      const off = sdk.on('input', (e) => {
        if (!e.reply) return;
        const b = asBytes(e.data);
        if (b) cb(b);
      });
      offs.push(off);
      return off;
    },
    close() {
      for (const off of offs.splice(0)) off();
    },
  };
}
