// @ts-check
// The seat table (D-388, host_handoff): which member holds which slot, and which slot is the host's. The host
// publishes it under `host:seats` each time the roster changes, next to the roster itself (net/codec.js ROSTER has
// slots and names but no member ids, and the wire format stays as it is). Every peer keeps the newest copy; the
// peer the platform makes the next host rebuilds its HostSession from it plus the newest `host:snap`, so every
// player keeps the same slot (and so the same colour, spawn point and unacknowledged inputs) after the move.
//
// JSON, small (8 seats are about 300 bytes), and read defensively: it comes from the network.

/**
 * @typedef {object} Seat
 * @property {string} id    the member's id (the platform's, or a lobby id for a guest)
 * @property {number} slot
 * @property {string} name
 */

/**
 * @typedef {object} SeatTable
 * @property {number} host        the host's own slot
 * @property {Seat[]} seats       the peers (not the host)
 */

const enc = new TextEncoder();
const dec = new TextDecoder();

/** @param {SeatTable} table @returns {Uint8Array} */
export function encodeSeats(table) {
  return enc.encode(JSON.stringify({ v: 1, host: table.host, seats: table.seats.map((s) => ({ id: s.id, slot: s.slot, name: s.name })) }));
}

/** @param {Uint8Array} bytes @param {number} maxSlots slots are 0..maxSlots-1 @returns {SeatTable | null} */
export function decodeSeats(bytes, maxSlots = 8) {
  try {
    if (bytes.length > 4096) return null;
    const j = JSON.parse(dec.decode(bytes));
    if (!j || j.v !== 1 || !Array.isArray(j.seats)) return null;
    const okSlot = (/** @type {unknown} */ n) => Number.isInteger(n) && /** @type {number} */ (n) >= 0 && /** @type {number} */ (n) < maxSlots;
    if (!okSlot(j.host)) return null;
    /** @type {Seat[]} */
    const seats = [];
    const used = new Set([j.host]);
    for (const s of j.seats.slice(0, maxSlots)) {
      if (!s || typeof s.id !== 'string' || !s.id || s.id.length > 128 || !okSlot(s.slot) || used.has(s.slot)) return null;
      used.add(s.slot);
      seats.push({ id: s.id, slot: s.slot, name: String(s.name ?? 'Player').slice(0, 24) });
    }
    return { host: j.host, seats };
  } catch {
    return null;
  }
}
