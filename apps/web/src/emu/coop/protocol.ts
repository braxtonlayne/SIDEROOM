/**
 * ACT-EMU-002: the wire format of co-op play. Every message is one binary packet carried by the activity data
 * channel (voice/dataPort.ts), which seals it with the call's E2EE keys; the SFU relays the ciphertext to every
 * member of the room who joined the co-op instance and adds the sender's user id, which it authenticated
 * itself. A packet therefore never says who sent it: the sender comes from the channel.
 *
 * All integers are big-endian. User ids are snowflakes, carried as u64.
 *
 *   type  name      direction        body
 *   1     ANNOUNCE  host -> all      version u8, free seats u8, host ping ms u16, title (UTF-8, rest of the packet)
 *   2     DISCOVER  guest -> all     (empty)  "any host here?"; hosts answer with an ANNOUNCE
 *   3     JOIN      guest -> host    host id u64
 *   4     SEAT      host -> guest    guest id u64, port u8 (1..3 = players 2..4, 254 = not allowed, 255 = no free seat)
 *   5     PAD       guest -> host    host id u64, sequence u16, buttons u16 (the whole pad every time)
 *   6     LEAVE     guest -> host    host id u64
 *   7     PING      guest -> host    host id u64, timestamp u32
 *   8     PONG      host -> guest    guest id u64, timestamp u32 (echoed)
 *   9     CLOSE     host -> all      (empty)  the session is over
 *   10    KICK      host -> guest    guest id u64
 *
 * The pad is the libretro RetroPad: bit n of `buttons` is joypad button n (see input.ts). Sending the whole
 * pad each time, not the change, makes a lost or reordered packet harmless: the next one repairs it.
 */

export const COOP_VERSION = 1;
/** The data-channel instance of co-op play; scoped per voice room by the SFU. Top bit set so it never meets an activity snowflake. */
export const COOP_INSTANCE = '9223372036854775809';

export const MSG = {
  ANNOUNCE: 1,
  DISCOVER: 2,
  JOIN: 3,
  SEAT: 4,
  PAD: 5,
  LEAVE: 6,
  PING: 7,
  PONG: 8,
  CLOSE: 9,
  KICK: 10,
} as const;

/** `SEAT.port` when the host has no free seat. */
export const NO_SEAT = 255;
/** `SEAT.port` when the guest may not take a seat (the owner default: only members holding `view_games` play). */
export const SEAT_NOT_ALLOWED = 254;
/** Players 2 to 4 are the core's ports 1 to 3 (port 0 is the host's own pad and can never be assigned). */
export const GUEST_PORTS = [1, 2, 3] as const;
export const MAX_TITLE_BYTES = 64;

export type CoopMessage =
  | { type: 'announce'; version: number; free: number; hostPingMs: number; title: string }
  | { type: 'discover' }
  | { type: 'join'; host: string }
  | { type: 'seat'; guest: string; port: number }
  | { type: 'pad'; host: string; seq: number; buttons: number }
  | { type: 'leave'; host: string }
  | { type: 'ping'; host: string; t: number }
  | { type: 'pong'; guest: string; t: number }
  | { type: 'close' }
  | { type: 'kick'; guest: string };

const U64_MAX = (1n << 64n) - 1n;

function putId(view: DataView, at: number, id: string): void {
  const n = BigInt(id);
  if (n < 0n || n > U64_MAX) throw new RangeError('user id out of range');
  view.setBigUint64(at, n);
}

const getId = (view: DataView, at: number): string => view.getBigUint64(at).toString();

/** Cut a title to MAX_TITLE_BYTES of UTF-8 without splitting a character. */
export function clampTitle(title: string): Uint8Array {
  const bytes = new TextEncoder().encode(title);
  if (bytes.length <= MAX_TITLE_BYTES) return bytes;
  let end = MAX_TITLE_BYTES;
  while (end > 0 && (bytes[end]! & 0xc0) === 0x80) end -= 1;
  return bytes.slice(0, end);
}

function withId(type: number, id: string, extra: number): { out: Uint8Array; view: DataView } {
  const out = new Uint8Array(9 + extra);
  const view = new DataView(out.buffer);
  out[0] = type;
  putId(view, 1, id);
  return { out, view };
}

export function encode(m: CoopMessage): Uint8Array {
  switch (m.type) {
    case 'announce': {
      const title = clampTitle(m.title);
      const out = new Uint8Array(5 + title.length);
      out[0] = MSG.ANNOUNCE;
      out[1] = m.version & 0xff;
      out[2] = Math.min(255, Math.max(0, m.free));
      new DataView(out.buffer).setUint16(
        3,
        Math.min(0xffff, Math.max(0, Math.round(m.hostPingMs))),
      );
      out.set(title, 5);
      return out;
    }
    case 'discover':
      return Uint8Array.of(MSG.DISCOVER);
    case 'close':
      return Uint8Array.of(MSG.CLOSE);
    case 'join':
      return withId(MSG.JOIN, m.host, 0).out;
    case 'leave':
      return withId(MSG.LEAVE, m.host, 0).out;
    case 'kick':
      return withId(MSG.KICK, m.guest, 0).out;
    case 'seat': {
      const { out } = withId(MSG.SEAT, m.guest, 1);
      out[9] = m.port & 0xff;
      return out;
    }
    case 'pad': {
      const { out, view } = withId(MSG.PAD, m.host, 4);
      view.setUint16(9, m.seq & 0xffff);
      view.setUint16(11, m.buttons & 0xffff);
      return out;
    }
    case 'ping': {
      const { out, view } = withId(MSG.PING, m.host, 4);
      view.setUint32(9, m.t >>> 0);
      return out;
    }
    case 'pong': {
      const { out, view } = withId(MSG.PONG, m.guest, 4);
      view.setUint32(9, m.t >>> 0);
      return out;
    }
  }
}

/** Parse one packet; `null` for anything malformed or unknown (it is dropped, never thrown). */
export function decode(bytes: Uint8Array): CoopMessage | null {
  if (bytes.length === 0) return null;
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const len = bytes.length;
  switch (bytes[0]) {
    case MSG.ANNOUNCE: {
      if (len < 5 || len > 5 + MAX_TITLE_BYTES) return null;
      let title: string;
      try {
        title = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(5));
      } catch {
        return null;
      }
      return {
        type: 'announce',
        version: bytes[1]!,
        free: bytes[2]!,
        hostPingMs: v.getUint16(3),
        title,
      };
    }
    case MSG.DISCOVER:
      return len === 1 ? { type: 'discover' } : null;
    case MSG.CLOSE:
      return len === 1 ? { type: 'close' } : null;
    case MSG.JOIN:
      return len === 9 ? { type: 'join', host: getId(v, 1) } : null;
    case MSG.LEAVE:
      return len === 9 ? { type: 'leave', host: getId(v, 1) } : null;
    case MSG.KICK:
      return len === 9 ? { type: 'kick', guest: getId(v, 1) } : null;
    case MSG.SEAT:
      return len === 10 ? { type: 'seat', guest: getId(v, 1), port: bytes[9]! } : null;
    case MSG.PAD:
      return len === 13
        ? { type: 'pad', host: getId(v, 1), seq: v.getUint16(9), buttons: v.getUint16(11) }
        : null;
    case MSG.PING:
      return len === 13 ? { type: 'ping', host: getId(v, 1), t: v.getUint32(9) } : null;
    case MSG.PONG:
      return len === 13 ? { type: 'pong', guest: getId(v, 1), t: v.getUint32(9) } : null;
    default:
      return null;
  }
}
