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
