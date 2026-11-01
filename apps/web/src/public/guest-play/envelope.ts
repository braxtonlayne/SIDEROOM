// ARC-GST-002: what travels INSIDE the sealed lobby payload. The relay stamps only a seat number on a message, and
// a seat is not an account. A game needs the identity it already knows (the member's user id in the room), so the
// member who bridges the room to the lobby (the link's creator, `activities/guestBridge.ts`) wraps what it relays.
//
//   0x00 || bytes                      sent by the seat itself (every guest does this; the id is `g<seat>`)
//   0x01 || len(1) || id || bytes      relayed by a MEMBER seat on behalf of a room member (`id` is their user id)
//   0x02 || json                       a MEMBER seat's control message: { host: <user id of the game's host> }
//
// A guest cannot make its payload look relayed: the receiver honours kinds 0x01 and 0x02 only from a seat the relay
// says is a member, so a guest forging an origin is read as plain data from itself.

export const KIND_DIRECT = 0;
export const KIND_RELAYED = 1;
export const KIND_SYS = 2;

const enc = new TextEncoder();
const dec = new TextDecoder();

export const encodeDirect = (bytes: Uint8Array): Uint8Array => {
  const out = new Uint8Array(1 + bytes.length);
  out[0] = KIND_DIRECT;
  out.set(bytes, 1);
  return out;
};

export function encodeRelayed(originId: string, bytes: Uint8Array): Uint8Array {
  const id = enc.encode(originId);
  if (id.length === 0 || id.length > 255) throw new Error('bad origin id');
  const out = new Uint8Array(2 + id.length + bytes.length);
  out[0] = KIND_RELAYED;
  out[1] = id.length;
  out.set(id, 2);
  out.set(bytes, 2 + id.length);
  return out;
}

export const encodeSys = (sys: { host: string }): Uint8Array => {
  const j = enc.encode(JSON.stringify(sys));
  const out = new Uint8Array(1 + j.length);
  out[0] = KIND_SYS;
  out.set(j, 1);
  return out;
};

export type Decoded =
  | { kind: 'data'; from: string; bytes: Uint8Array }
  | { kind: 'sys'; host: string }
  | null;

/**
 * Reads an opened payload published by `seat`, whose role the relay reported. Guests are `g<seat>`; a member seat's
 * relayed messages carry the room member's id; anything malformed, or a privileged kind from a guest, is dropped
 * (a forged 0x01 from a guest is treated as data from that guest).
 */
export function decodeLobby(seat: number, role: 'guest' | 'member', bytes: Uint8Array): Decoded {
  if (bytes.length === 0) return null;
  const kind = bytes[0];
  if (kind === KIND_DIRECT || role === 'guest') {
    // A guest's bytes are data whatever their first byte; a direct message from a member seat is its own.
    const body = kind === KIND_DIRECT ? bytes.slice(1) : bytes;
    return { kind: 'data', from: role === 'guest' ? `g${seat}` : `m${seat}`, bytes: body };
  }
  if (kind === KIND_RELAYED) {
    const n = bytes[1];
    if (n === undefined || n === 0 || bytes.length < 2 + n) return null;
    return { kind: 'data', from: dec.decode(bytes.slice(2, 2 + n)), bytes: bytes.slice(2 + n) };
  }
  if (kind === KIND_SYS) {
    try {
      const j = JSON.parse(dec.decode(bytes.slice(1))) as { host?: unknown };
      return typeof j.host === 'string' && j.host.length > 0 && j.host.length <= 255
        ? { kind: 'sys', host: j.host }
        : null;
    } catch {
      return null;
    }
  }
  return null;
}

/**
 * Room -> room: guest messages the bridge re-sends into the room's own data channel so the other members see them.
 * Prefix 0xFF 'S' 'R' 'G', then len(1) and the guest id, then the bytes. Game payloads are JSON or SDK envelopes and do
 * not start with 0xFF, so members decode this and leave everything else alone.
 */
const ROOM_MAGIC = [0xff, 0x53, 0x52, 0x47];

export function wrapForRoom(guestId: string, bytes: Uint8Array): Uint8Array {
  const id = enc.encode(guestId);
  const out = new Uint8Array(5 + id.length + bytes.length);
  out.set(ROOM_MAGIC, 0);
  out[4] = id.length;
  out.set(id, 5);
  out.set(bytes, 5 + id.length);
  return out;
}

export function unwrapFromRoom(bytes: Uint8Array): { from: string; bytes: Uint8Array } | null {
  if (bytes.length < 6 || ROOM_MAGIC.some((m, i) => bytes[i] !== m)) return null;
  const n = bytes[4]!;
  if (n === 0 || bytes.length < 5 + n) return null;
  return { from: dec.decode(bytes.slice(5, 5 + n)), bytes: bytes.slice(5 + n) };
}
