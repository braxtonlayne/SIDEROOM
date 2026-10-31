// Initiative order: a tracker of entries with a score, whose turn it is and the round. Pure reducers: each
// returns a new tracker and never mutates its input, so the host can apply one and publish the result.
//
// Tracker: { entries: [{ id, name, init, tie, tokenId, owner, out }], turn: id | null, round: number }.
// Order is by score high to low, then `tie` high to low (a dexterity-style tie-break), then entry order.

export const MAX_ENTRIES = 100;

export const emptyTracker = () => ({ entries: [], turn: null, round: 0 });

const clean = (s, n) => String(s ?? '').slice(0, n);

/** Entries in turn order. */
export function ordered(t) {
  return t.entries
    .map((e, i) => ({ e, i }))
    .sort((a, b) => b.e.init - a.e.init || b.e.tie - a.e.tie || a.i - b.i)
    .map((x) => x.e);
}

export function addEntry(t, entry) {
  if (t.entries.length >= MAX_ENTRIES || t.entries.some((e) => e.id === entry.id)) return t;
  const e = {
    id: clean(entry.id, 64),
    name: clean(entry.name, 48) || '?',
    init: Number.isFinite(entry.init) ? Math.trunc(entry.init) : 0,
    tie: Number.isFinite(entry.tie) ? Math.trunc(entry.tie) : 0,
    tokenId: entry.tokenId ?? null,
    owner: entry.owner ?? null,
    out: false,
  };
  if (!e.id) return t;
  return { ...t, entries: [...t.entries, e] };
}

export function updateEntry(t, id, patch) {
  if (!t.entries.some((e) => e.id === id)) return t;
  return {
    ...t,
    entries: t.entries.map((e) => {
      if (e.id !== id) return e;
      const n = { ...e };
      if (patch.name !== undefined) n.name = clean(patch.name, 48) || e.name;
      if (Number.isFinite(patch.init)) n.init = Math.trunc(patch.init);
      if (Number.isFinite(patch.tie)) n.tie = Math.trunc(patch.tie);
      if (typeof patch.out === 'boolean') n.out = patch.out;
      return n;
    }),
  };
}

export function removeEntry(t, id) {
  if (!t.entries.some((e) => e.id === id)) return t;
  let turn = t.turn;
  if (turn === id) {
    const live = ordered(t).filter((e) => !e.out);
    const at = live.findIndex((e) => e.id === id);
    turn = live.length > 1 ? live[(at + 1) % live.length].id : null;
  }
  const entries = t.entries.filter((e) => e.id !== id);
  return { ...t, entries, turn: entries.length ? turn : null, round: entries.length ? t.round : 0 };
}

/** Begin combat: round 1 at the top of the order (out entries skipped). */
export function start(t) {
  const first = ordered(t).find((e) => !e.out);
  return first ? { ...t, turn: first.id, round: 1 } : t;
}

/** End combat: no turn, round 0; entries and scores stay. */
export function stop(t) {
  return { ...t, turn: null, round: 0 };
}

/** Advance to the next entry that is not out; wrapping past the last starts the next round. */
export function next(t) {
  if (t.turn === null) return t;
  const live = ordered(t).filter((e) => !e.out || e.id === t.turn);
  const alive = live.filter((e) => !e.out);
  if (alive.length === 0) return { ...t, turn: null };
  const at = alive.findIndex((e) => e.id === t.turn);
  // The current entry may itself have gone out: continue from its place in the full order.
  let idx;
  if (at >= 0) idx = at + 1;
  else {
    const full = ordered(t);
    const pos = full.findIndex((e) => e.id === t.turn);
    const after = full.slice(pos + 1).find((e) => !e.out);
    idx = after ? alive.findIndex((e) => e.id === after.id) : alive.length;
  }
  const wrapped = idx >= alive.length;
  return { ...t, turn: alive[wrapped ? 0 : idx].id, round: wrapped ? t.round + 1 : t.round };
}

/** Step back; going before the first entry of round 1 stays put, earlier rounds go back a round. */
export function previous(t) {
  if (t.turn === null) return t;
  const alive = ordered(t).filter((e) => !e.out || e.id === t.turn);
  const at = alive.findIndex((e) => e.id === t.turn);
  if (at > 0) return { ...t, turn: alive[at - 1].id };
  if (t.round <= 1) return t;
  return { ...t, turn: alive[alive.length - 1].id, round: t.round - 1 };
}

/** The entries as the shared state carries them, validated (anything malformed is dropped). */
export function sanitize(raw) {
  if (!raw || typeof raw !== 'object' || !Array.isArray(raw.entries)) return emptyTracker();
  let t = emptyTracker();
  for (const e of raw.entries.slice(0, MAX_ENTRIES)) {
    if (e && typeof e === 'object') {
      t = addEntry(t, e);
      if (e.out === true) t = updateEntry(t, e.id, { out: true });
    }
  }
  const turn = t.entries.some((e) => e.id === raw.turn) ? raw.turn : null;
  const round = Number.isInteger(raw.round) && raw.round >= 0 ? Math.min(raw.round, 9999) : 0;
  return { ...t, turn, round: turn === null ? 0 : Math.max(round, 1) };
}
