// @ts-check
// A `Channel` (net/channel.js) over the Embedded App SDK's data channel (ACT-CH-001): `sdk.state` for what the
// host publishes to everyone, `sdk.input.send` from a guest to the host, `sdk.input.reply` from the host to one
// member, `sdk.on('state'|'input')` for what arrives. Everything is JSON in UTF-8; the host page encrypts it
// under the room's sender key and the SFU relays opaque bytes.
//
// Who I am and who hosts come from the SDK (`sdk.isHost`, the member ids of the data transport), never from a
// Land.io sign-in. The participant list is the SDK's `getInstanceConnectedParticipants` plus its update event.

import { CHUNK_CHARS, Reassembler, chunkMessage, decodeMessage, encodeMessage } from './channel.js';

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./channel.js').Member} Member */

/** The prefix the SDK reserves for the host's keys. */
const HOST_PREFIX = 'host:';

/**
 * The slice of `SideRoomSDK` this needs (see packages/embedded-app-sdk/src/sdk.ts and data.ts).
 * @typedef {object} SdkLike
 * @property {boolean} isHost
 * @property {{ get(key: string): any, all(): Record<string, unknown>, set(patch: Record<string, unknown>): void }} state
 * @property {{ send(bytes: Uint8Array): void, reply(member: string, bytes: Uint8Array): void }} input
 * @property {(event: 'state' | 'input', cb: (e: any) => void) => () => void} on
 */

/**
 * @param {SdkLike} sdk
 * @param {{ self: Member, hostId: string,
 *   participants: () => Promise<Member[]>, onParticipants: (cb: (list: Member[]) => void) => () => void,
 *   chunkChars?: number }} opt
 *   self / hostId: the member ids the SDK's data transport carries (`DataTransport.self` / `.host`)
 * @returns {Promise<Channel>}
 */
export async function channelFromSdk(sdk, opt) {
  const chunkChars = opt.chunkChars ?? CHUNK_CHARS;
  /** @type {Member[]} */
  let members = await opt.participants().catch(() => []);
  if (!members.some((m) => m.id === opt.self.id)) members = [opt.self, ...members];
  /** @type {Set<(list: Member[]) => void>} */
  const memberCbs = new Set();
  const offParticipants = opt.onParticipants((list) => {
    members = list.some((m) => m.id === opt.self.id) ? list : [opt.self, ...list];
    memberCbs.forEach((cb) => cb(members));
  });
  /** @type {(() => void)[]} */
  const offs = [offParticipants];
  const reasm = new Reassembler();

  /** @type {Channel} */
  const ch = {
    self: opt.self,
    hostId: opt.hostId,
    isHost: sdk.isHost,
    members: () => members,
    onMembers(cb) {
      memberCbs.add(cb);
      return () => memberCbs.delete(cb);
    },
    toHost(msg) {
      for (const piece of chunkMessage(msg, chunkChars)) sdk.input.send(encodeMessage(piece));
    },
    onInput(cb) {
      const off = sdk.on('input', (e) => {
        if (e.reply) return;
        const m = reasm.push(`in:${e.from}`, decodeMessage(e.data));
        if (m !== undefined) cb(e.from, m);
      });
      offs.push(off);
      return off;
    },
    publish(key, value) {
      // `sdk.state.set` replaces the key; an oversized frame goes out as ordered pieces under the same key.
      for (const piece of chunkMessage(/** @type {object} */ (value), chunkChars)) sdk.state.set({ [HOST_PREFIX + key]: piece });
    },
    onState(cb) {
      const assemble = (/** @type {string} */ key, /** @type {unknown} */ value) => {
        const v = reasm.push(`st:${key}`, value);
        if (v !== undefined) cb(key.slice(HOST_PREFIX.length), v);
      };
      // A newcomer first hears the document as it stands (the SDK already synced it from the host).
      for (const [key, value] of Object.entries(sdk.state.all())) if (key.startsWith(HOST_PREFIX)) assemble(key, value);
      const off = sdk.on('state', (e) => {
        for (const c of e.changes) if (!c.local && c.key.startsWith(HOST_PREFIX) && c.value !== undefined) assemble(c.key, c.value);
      });
      offs.push(off);
      return off;
    },
    sendTo(to, msg) {
      for (const piece of chunkMessage(msg, chunkChars)) sdk.input.reply(to, encodeMessage(piece));
    },
    onReply(cb) {
      const off = sdk.on('input', (e) => {
        if (!e.reply) return;
        const m = reasm.push(`re:${e.from}`, decodeMessage(e.data));
        if (m !== undefined) cb(m);
      });
      offs.push(off);
      return off;
    },
    close() {
      for (const off of offs.splice(0)) off();
    },
  };
  return ch;
}
