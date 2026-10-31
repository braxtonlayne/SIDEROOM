// @ts-check
// The seam between Land.io and the Embedded App SDK's data channel (ACT-CH-001). The game and its transport
// talk to a `Channel`; `sdk-channel.js` makes one from a real `SideRoomSDK`, `loopback.js` makes N of them
// inside one process (tests), `bc-channel.js` makes them between browser tabs (local development). Messages are
// plain JSON objects; the bytes on the wire are their UTF-8 JSON, which the SDK then encrypts under the room's
// sender key (the SFU relays opaque bytes).

/**
 * @typedef {object} Member
 * @property {string} id      the SDK's member (user) id
 * @property {string} name    display name
 */

/**
 * @typedef {object} Channel
 * @property {Member} self                                   who I am, from the SDK (never from a Land.io sign-in)
 * @property {string} hostId                                 the member who launched the instance: the only writer of `host:*` state and the one who runs the game
 * @property {boolean} isHost                                self.id === hostId
 * @property {() => Member[]} members                        everyone in the instance, me included
 * @property {(cb: (list: Member[]) => void) => () => void} onMembers
 * @property {(msg: object) => void} toHost                  guest -> host (`sdk.input.send`)
 * @property {(cb: (from: string, msg: any) => void) => () => void} onInput   host only: what members sent (`sdk.on('input')`)
 * @property {(key: string, value: unknown) => void} publish host only: write `host:<key>` in the shared state (`sdk.state`)
 * @property {(cb: (key: string, value: any) => void) => () => void} onState   everyone: the shared state changed (`sdk.on('state')`); a newcomer first hears the current values
 * @property {(to: string, msg: object) => void} sendTo      host only: an addressed reply to one member
 * @property {(cb: (msg: any) => void) => () => void} onReply  guests: what the host addressed to me
 * @property {() => void} close
 */

const enc = new TextEncoder();
const dec = new TextDecoder();

/** A message as bytes. @param {unknown} msg @returns {Uint8Array} */
export function encodeMessage(msg) {
  return enc.encode(JSON.stringify(msg));
}

/** Bytes (or an ArrayBuffer, or already a string) as a message; undefined when it is not valid JSON. @param {unknown} data */
export function decodeMessage(data) {
  try {
    if (typeof data === 'string') return JSON.parse(data);
    if (data instanceof ArrayBuffer) return JSON.parse(dec.decode(new Uint8Array(data)));
    if (ArrayBuffer.isView(data)) return JSON.parse(dec.decode(new Uint8Array(data.buffer, data.byteOffset, data.byteLength)));
    if (data && typeof data === 'object') return data; // an SDK that already parsed it
  } catch {
    /* fall through */
  }
  return undefined;
}

/** The key under which the host publishes the tick stream. */
export const TICK_KEY = 'tick';

/**
 * The data channel carries at most 16 KiB per envelope (SDK `MAX_DATA_BYTES`), and a host reply travels as
 * base64. A message whose JSON is longer than `CHUNK_CHARS` (a full snapshot of a big arena, a big capture's
 * record) is split into ordered pieces `{ __c: id, i, n, s }`; `Reassembler` puts them back. The channel is
 * ordered and reliable, so a piece that does not fit the sequence just resets the assembly (the game's
 * resync covers the loss).
 */
export const CHUNK_CHARS = 3000;
let chunkSeq = 0;

/** @param {object} msg @param {number} [max] @returns {object[]} the message itself, or its pieces */
export function chunkMessage(msg, max = CHUNK_CHARS) {
  const json = JSON.stringify(msg);
  if (json.length <= max) return [msg];
  const id = `${Date.now().toString(36)}${(chunkSeq++).toString(36)}`;
  const n = Math.ceil(json.length / max);
  return Array.from({ length: n }, (_, i) => ({ __c: id, i, n, s: json.slice(i * max, (i + 1) * max) }));
}

export class Reassembler {
  constructor() {
    /** @type {Map<string, {id: string, next: number, parts: string[]}>} one assembly per sender and stream */
    this.open = new Map();
  }

  /**
   * Feed one received object. @param {string} stream who/what it came from (a member id and a kind)
   * @param {any} obj @returns {any} the whole message when `obj` is one (or completes one), else undefined
   */
  push(stream, obj) {
    if (!obj || typeof obj !== 'object' || typeof obj.__c !== 'string') return obj;
    let a = this.open.get(stream);
    if (obj.i === 0 || !a || a.id !== obj.__c) {
      if (obj.i !== 0) {
        this.open.delete(stream);
        return undefined;
      }
      a = { id: obj.__c, next: 0, parts: [] };
      this.open.set(stream, a);
    }
    if (obj.i !== a.next || typeof obj.s !== 'string') {
      this.open.delete(stream);
      return undefined;
    }
    a.parts.push(obj.s);
    a.next++;
    if (a.next < obj.n) return undefined;
    this.open.delete(stream);
    try {
      return JSON.parse(a.parts.join(''));
    } catch {
      return undefined;
    }
  }
}
