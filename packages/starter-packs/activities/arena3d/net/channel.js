// @ts-check
// The seam between the game and whatever carries its bytes. HostSession and PredictedClient talk to a
// `Channel`; `sdk-channel.js` makes one from a real SideRoomSDK (the SIDE ROOM data channel, ACT-CH-001),
// `loopback.js` makes N of them in one process with simulated latency and loss (tests), and `bc-channel.js`
// makes them between browser tabs (local development). Payloads are binary (net/codec.js), never JSON.
//
// Shape follows Land.io's net/channel.js, with Uint8Array payloads:
//   toHost   = sdk.input.send(bytes)            peer -> host
//   sendTo   = sdk.input.reply(member, bytes)   host -> one peer
//   publish  = sdk.state.set({ 'host:<key>': base64 })   host -> everyone, latest value replayed to newcomers

/**
 * @typedef {object} Member
 * @property {string} id      the platform's member id (a lobby id for a guest)
 * @property {string} name    display name / callsign
 */

/**
 * @typedef {object} Channel
 * @property {Member} self
 * @property {string} hostId                                       who runs the authoritative simulation
 * @property {boolean} isHost
 * @property {(bytes: Uint8Array) => void} toHost                  peer -> host
 * @property {(cb: (from: string, bytes: Uint8Array) => void) => () => void} onInput   host: what peers sent
 * @property {(key: string, bytes: Uint8Array) => void} publish   host: shared state, last value wins
 * @property {(cb: (key: string, bytes: Uint8Array) => void) => () => void} onState  everyone except the host itself
 * @property {(to: string, bytes: Uint8Array) => void} sendTo     host -> one peer
 * @property {(cb: (bytes: Uint8Array) => void) => () => void} onReply   peers: what the host addressed to me
 * @property {() => void} close
 */

/** Bytes as base64 (the SDK's shared state holds JSON values, so binary travels as a string). @param {Uint8Array} bytes */
export function toBase64(bytes) {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

/** @param {string} text @returns {Uint8Array | null} */
export function fromBase64(text) {
  try {
    const s = atob(text);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** Whatever arrived (bytes, an ArrayBuffer, a view) as a Uint8Array copy, or null. @param {unknown} data */
export function asBytes(data) {
  if (data instanceof Uint8Array) return data;
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  return null;
}
